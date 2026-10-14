package backup

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"os"
	"path"
	"path/filepath"
	"time"

	"github.com/ZeroDevi1/gamesave-manager/internal/config"
	"github.com/ZeroDevi1/gamesave-manager/internal/pathutil"
	"github.com/ZeroDevi1/gamesave-manager/internal/storage"
)

// SyncAction 是启动前同步执行的动作。
type SyncAction string

const (
	SyncSkipped SyncAction = "skipped"
	SyncPulled  SyncAction = "pulled"
	SyncPushed  SyncAction = "pushed"
)

// SyncResult 是启动前同步的结果。
type SyncResult struct {
	Action       SyncAction `json:"action"`
	Message      string     `json:"message"`
	FilesChanged int        `json:"files_changed"`
}

// PreLaunchSync 比对云端 manifest.json 与本地存档，按时间戳决定同步方向：
// 云端比本地最新清单新（或本地从未备份）则拉取，否则推送有差异的文件，两边一致时跳过。
// 云端没有清单时跳过。
//
// 拉取按 "<云端目录>/<相对路径>" 下载，而备份实际位于 full/ 的 ZIP 与 incremental/<时间戳>/ 目录中，
// 这一路径约定沿用自 Tauri 版，单个文件下载失败只记录日志。
func PreLaunchSync(ctx context.Context, gameID string) (SyncResult, error) {
	cfg, game, err := loadGame(gameID)
	if err != nil {
		return SyncResult{}, err
	}
	remote, err := downloadRemoteManifest(ctx, &cfg, game)
	if err != nil {
		return SyncResult{}, err
	}
	if remote == nil {
		slog.Info("[启动同步] 无远端清单，跳过同步", "game", game.Name)
		return SyncResult{Action: SyncSkipped, Message: "远端无备份清单，跳过同步"}, nil
	}

	local := scanLocal(game)
	latest, err := LatestManifest(gameID)
	if err != nil {
		return SyncResult{}, err
	}

	if latest == nil || remote.Timestamp.After(latest.Timestamp) {
		n, err := pullFromRemote(ctx, &cfg, game, remote.Files)
		if err != nil {
			return SyncResult{}, err
		}
		slog.Info("[启动同步] 已从远端拉取", "game", game.Name, "files", n)
		return SyncResult{
			Action:       SyncPulled,
			Message:      fmt.Sprintf("远端存档更新（%s），已拉取 %d 个文件到本地", remote.Timestamp.UTC().Format("2006-01-02 15:04"), n),
			FilesChanged: n,
		}, nil
	}

	remoteFiles := map[string]FileEntry{}
	for _, f := range remote.Files {
		remoteFiles[f.RelativePath] = f
	}
	diff := 0
	for rel, l := range local {
		r, ok := remoteFiles[rel]
		if !ok || l.ModifiedTime.After(r.ModifiedTime) || l.SHA256 != r.SHA256 {
			diff++
		}
	}
	if diff == 0 {
		return SyncResult{Action: SyncSkipped, Message: "本地与远端存档一致，无需同步"}, nil
	}
	if err := pushToRemote(ctx, &cfg, game, local); err != nil {
		return SyncResult{}, err
	}
	slog.Info("[启动同步] 已推送到远端", "game", game.Name, "files", diff)
	return SyncResult{Action: SyncPushed, Message: fmt.Sprintf("本地存档更新，已推送 %d 个文件到远端", diff), FilesChanged: diff}, nil
}

// localBase 返回第一个存档路径展开后的本地目录，同步时以它为相对路径的根。
func localBase(g *config.GameConfig) (string, error) {
	if len(g.SavePaths) == 0 {
		return "", errors.New("游戏无存档路径")
	}
	return pathutil.ExpandEnv(g.SavePaths[0]), nil
}

func pullFromRemote(ctx context.Context, cfg *config.AppConfig, g *config.GameConfig, files []FileEntry) (int, error) {
	base, err := localBase(g)
	if err != nil {
		return 0, err
	}
	backend, err := storage.FromConfig(cfg)
	if err != nil {
		return 0, err
	}
	remoteBase := cfg.GameRemotePath(g)
	n := 0
	for _, f := range files {
		local := filepath.Join(base, filepath.FromSlash(f.RelativePath))
		if err := os.MkdirAll(filepath.Dir(local), 0o755); err != nil {
			return n, err
		}
		if err := backend.DownloadFile(ctx, remoteDir(remoteBase)+f.RelativePath, local); err != nil {
			slog.Warn("[启动同步] 下载失败", "file", f.RelativePath, "err", err)
			continue
		}
		n++
	}
	storage.PersistSession(backend)

	// 让本地最新清单与远端一致，下次比对不再重复拉取
	err = saveManifest(BackupManifest{
		GameID: g.ID, BackupType: BackupIncremental, Timestamp: time.Now().UTC(),
		Files: files, TargetPath: remoteBase,
	})
	return n, err
}

func pushToRemote(ctx context.Context, cfg *config.AppConfig, g *config.GameConfig, local map[string]FileEntry) error {
	base, err := localBase(g)
	if err != nil {
		return err
	}
	backend, err := storage.FromConfig(cfg)
	if err != nil {
		return err
	}
	now := time.Now().UTC()
	dir := remoteDir(cfg.GameRemotePath(g), "incremental", FormatTimestamp(now))
	files := make([]FileEntry, 0, len(local))
	for _, f := range local {
		files = append(files, f)
		p := filepath.Join(base, filepath.FromSlash(f.RelativePath))
		if _, err := os.Stat(p); err != nil {
			continue
		}
		remotePath := dir + f.RelativePath
		_ = backend.Mkdir(ctx, path.Dir(remotePath))
		if err := backend.UploadFile(ctx, p, remotePath); err != nil {
			slog.Warn("[启动同步] 上传失败", "file", f.RelativePath, "err", err)
		}
	}
	storage.PersistSession(backend)
	return uploadRemoteManifest(ctx, cfg, g, files, now)
}
