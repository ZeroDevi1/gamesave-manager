package game

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"strings"
	"time"

	"github.com/ZeroDevi1/gamesave-manager/internal/backup"
	"github.com/ZeroDevi1/gamesave-manager/internal/config"
	"github.com/ZeroDevi1/gamesave-manager/internal/pathutil"
)

// OpenURL 用系统默认程序打开 URL（用于 steam:// 启动协议），由 main 在启动时注入。
var OpenURL func(url string) error

// GetGames 返回游戏列表，并为缺少远程封面的游戏补上 Steam CDN 竖版海报地址：
// 优先用游戏自身的 AppID，没有时按名称在游戏数据库中匹配借用。有补全时写回配置（写回失败忽略）。
func GetGames() ([]config.GameConfig, error) {
	cfg, err := config.Load()
	if err != nil {
		return nil, err
	}
	db, err := LoadDB()
	if err != nil {
		db = defaultDB()
	}
	dirty := false
	for i := range cfg.Games {
		g := &cfg.Games[i]
		if g.LogoPath != nil && strings.HasPrefix(*g.LogoPath, "http") {
			continue
		}
		appid := g.SteamAppID
		if appid == nil {
			if matches := SearchEntries(&db, g.Name); len(matches) > 0 {
				appid = matches[0].SteamAppID
			}
		}
		if appid == nil {
			continue
		}
		g.LogoPath = str(fmt.Sprintf("https://steamcdn-a.akamaihd.net/steam/apps/%d/library_600x900_2x.jpg", *appid))
		if g.SteamAppID == nil {
			g.SteamAppID = appid
		}
		dirty = true
	}
	if dirty {
		_ = config.Save(&cfg)
	}
	return cfg.Games, nil
}

// AddGame 新增一个手动配置的游戏；ID 为“小写名称-毫秒时间戳”，云端目录为 /GameSaves/<name>。
func AddGame(name string, savePaths []string) (config.GameConfig, error) {
	cfg, err := config.Load()
	if err != nil {
		return config.GameConfig{}, err
	}
	if savePaths == nil {
		savePaths = []string{}
	}
	g := config.GameConfig{
		ID:         fmt.Sprintf("%s_%d", strings.ReplaceAll(strings.ToLower(name), " ", "-"), time.Now().UnixMilli()),
		Name:       name,
		SavePaths:  savePaths,
		RemotePath: "/GameSaves/" + name,
	}
	cfg.Games = append(cfg.Games, g)
	return g, config.Save(&cfg)
}

// RemoveGame 从配置中删除游戏（不删除本地清单与云端备份）。
func RemoveGame(id string) error {
	cfg, err := config.Load()
	if err != nil {
		return err
	}
	kept := cfg.Games[:0]
	for _, g := range cfg.Games {
		if g.ID != id {
			kept = append(kept, g)
		}
	}
	cfg.Games = kept
	return config.Save(&cfg)
}

// SaveFile 是扫描到的一个本地存档文件。
type SaveFile struct {
	Path         string `json:"path"`
	RelativePath string `json:"relative_path"`
	Size         uint64 `json:"size"`
	// ModifiedTime 是精确到秒的 RFC 3339 时间（UTC）。
	ModifiedTime string `json:"modified_time"`
}

// ScanSaves 列出游戏全部存档路径下的文件；遇到无法读取的条目时返回错误。
func ScanSaves(id string) ([]SaveFile, error) {
	cfg, err := config.Load()
	if err != nil {
		return nil, err
	}
	g := cfg.FindGame(id)
	if g == nil {
		return nil, fmt.Errorf("未找到游戏: %s", id)
	}
	out := []SaveFile{}
	err = pathutil.WalkSaveFiles(g.SavePaths, true, func(f pathutil.LocalFile) error {
		out = append(out, SaveFile{
			Path:         f.Path,
			RelativePath: f.Rel,
			Size:         uint64(f.Info.Size()),
			ModifiedTime: f.Info.ModTime().UTC().Truncate(time.Second).Format("2006-01-02T15:04:05+00:00"),
		})
		return nil
	})
	return out, err
}

// CreateFromDB 用游戏数据库条目创建本地游戏：展开存档路径占位符，exe 名取第一个别名，默认开启自动备份。
// 封面优先继承条目已有的本地缓存（如从 exe 提取的图标），否则按 AppID 下载；封面获取失败不影响创建。
func CreateFromDB(ctx context.Context, dbID string) (config.GameConfig, error) {
	db, err := LoadDB()
	if err != nil {
		return config.GameConfig{}, err
	}
	var entry *GameDbEntry
	for i := range db.Entries {
		if db.Entries[i].ID == dbID {
			entry = &db.Entries[i]
			break
		}
	}
	if entry == nil {
		return config.GameConfig{}, errors.New("未找到该游戏数据库条目")
	}

	paths := make([]string, 0, len(entry.SavePaths))
	for _, p := range entry.SavePaths {
		paths = append(paths, pathutil.ExpandEnv(p))
	}
	id := fmt.Sprintf("%s_%d", entry.ID, time.Now().UnixMilli())

	logo := copyLogo(entry.ID, id)
	if logo == nil && entry.SteamAppID != nil {
		logo, _ = FetchLogo(ctx, id, entry.SteamAppID)
	}

	var exe *string
	if len(entry.Aliases) > 0 {
		exe = str(entry.Aliases[0])
	}
	autoBackup := true
	g := config.GameConfig{
		ID:                id,
		Name:              entry.Name,
		SavePaths:         paths,
		RemotePath:        "/GameSaves/" + entry.Name,
		LogoPath:          logo,
		SteamAppID:        entry.SteamAppID,
		ExeName:           exe,
		AutoBackupEnabled: &autoBackup,
	}
	cfg, err := config.Load()
	if err != nil {
		return config.GameConfig{}, err
	}
	cfg.Games = append(cfg.Games, g)
	return g, config.Save(&cfg)
}

// DbPathUpdate 是刷新数据库存档路径时一个条目的变化。
type DbPathUpdate struct {
	Name       string `json:"name"`
	Added      int    `json:"added"`
	TotalPaths int    `json:"total_paths"`
}

// RefreshDBSavePaths 对每个带 AppID 的数据库条目，从 PCGamingWiki 拉取 Windows 存档路径并追加新路径（不删除旧路径）。
// 单个条目查询失败只记录日志；有变化时写回数据库，返回各条目的新增数量。
func RefreshDBSavePaths(ctx context.Context) ([]DbPathUpdate, error) {
	db, err := LoadDB()
	if err != nil {
		return nil, err
	}
	out := []DbPathUpdate{}
	for i := range db.Entries {
		e := &db.Entries[i]
		if e.SteamAppID == nil {
			continue
		}
		results, err := SearchPcgwBySteamAppID(ctx, *e.SteamAppID)
		if err != nil {
			slog.Warn("[RefreshDB] PCGW 查询失败", "game", e.Name, "appid", *e.SteamAppID, "err", err)
			continue
		}
		added := 0
		for _, r := range results {
			d, err := FetchSavePaths(ctx, r.PageName)
			if err != nil {
				slog.Warn("[RefreshDB] 获取路径失败", "game", e.Name, "page", r.PageName, "err", err)
				continue
			}
			for _, p := range d.WindowsSavePaths {
				norm := strings.ReplaceAll(p, `\`, "/")
				exists := false
				for _, old := range e.SavePaths {
					if strings.ReplaceAll(old, `\`, "/") == norm {
						exists = true
						break
					}
				}
				if !exists {
					e.SavePaths = append(e.SavePaths, norm)
					added++
				}
			}
		}
		if added > 0 {
			slog.Info("[RefreshDB] 新增路径", "game", e.Name, "added", added, "total", len(e.SavePaths))
			out = append(out, DbPathUpdate{Name: e.Name, Added: added, TotalPaths: len(e.SavePaths)})
		}
	}
	if len(out) > 0 {
		return out, SaveDB(&db)
	}
	return out, nil
}

// UpdateGameSettings 更新单个游戏的 exe 名与自动备份/确认覆盖设置；参数为 nil 的字段保持不变。
func UpdateGameSettings(id string, exeName *string, autoBackup, confirm *bool) error {
	cfg, err := config.Load()
	if err != nil {
		return err
	}
	g := cfg.FindGame(id)
	if g == nil {
		return errors.New("未找到游戏")
	}
	if exeName != nil {
		g.ExeName = exeName
	}
	if autoBackup != nil {
		g.AutoBackupEnabled = autoBackup
	}
	if confirm != nil {
		g.ConfirmBeforeSync = confirm
	}
	return config.Save(&cfg)
}

// Launch 通过 steam://rungameid 启动游戏。
func Launch(steamAppID *uint64) error {
	if steamAppID == nil {
		return errors.New("该游戏未配置 Steam AppID，无法自动启动")
	}
	if OpenURL == nil {
		return errors.New("启动失败: 未初始化 URL 打开器")
	}
	if err := OpenURL(fmt.Sprintf("steam://rungameid/%d", *steamAppID)); err != nil {
		return fmt.Errorf("启动失败: %v", err)
	}
	return nil
}

// LaunchWithSyncResult 是带同步的启动结果。
type LaunchWithSyncResult struct {
	Launched         bool   `json:"launched"`
	SyncMessage      string `json:"sync_message"`
	SyncFilesChanged int    `json:"sync_files_changed"`
	// WatcherActive 表示已开始监控游戏进程，进程退出后会自动增量备份。
	WatcherActive bool `json:"watcher_active"`
}

// LaunchWithSync 先执行启动前同步，再经 Steam 启动游戏（未配置 AppID 时跳过启动）；
// 自动备份开启且配置了 exe 名时开始监控进程，退出后自动增量备份。
func LaunchWithSync(ctx context.Context, id string) (LaunchWithSyncResult, error) {
	cfg, err := config.Load()
	if err != nil {
		return LaunchWithSyncResult{}, err
	}
	gp := cfg.FindGame(id)
	if gp == nil {
		return LaunchWithSyncResult{}, errors.New("未找到游戏")
	}
	g := *gp

	sync, err := backup.PreLaunchSync(ctx, id)
	if err != nil {
		return LaunchWithSyncResult{}, err
	}
	slog.Info("[启动] 同步完成", "game", g.Name, "action", sync.Action, "message", sync.Message)

	launched := false
	if g.SteamAppID != nil {
		if err := Launch(g.SteamAppID); err != nil {
			return LaunchWithSyncResult{}, err
		}
		launched = true
	} else {
		slog.Warn("[启动] 未配置 Steam AppID，跳过自动启动", "game", g.Name)
	}

	auto := cfg.Settings.AutoBackup
	if g.AutoBackupEnabled != nil {
		auto = *g.AutoBackupEnabled
	}
	watching := false
	if auto && g.ExeName != nil {
		watching = true
		WatchProcess(*g.ExeName, func() {
			slog.Info("[自动备份] 游戏进程退出，开始增量备份", "game", g.Name)
			// 进程退出时调用方的请求早已结束，使用独立的上下文
			res, err := backup.IncrementalBackup(context.Background(), id)
			if err != nil {
				slog.Error("[自动备份] 失败", "game", g.Name, "err", err)
				return
			}
			slog.Info("[自动备份] 完成", "game", g.Name, "message", res.Message)
		})
	}
	return LaunchWithSyncResult{Launched: launched, SyncMessage: sync.Message, SyncFilesChanged: sync.FilesChanged, WatcherActive: watching}, nil
}
