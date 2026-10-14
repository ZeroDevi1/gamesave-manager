package main

import (
	"context"

	"github.com/egoist/mygo"

	"github.com/ZeroDevi1/gamesave-manager/internal/alist"
	"github.com/ZeroDevi1/gamesave-manager/internal/backup"
	"github.com/ZeroDevi1/gamesave-manager/internal/config"
	"github.com/ZeroDevi1/gamesave-manager/internal/game"
	"github.com/ZeroDevi1/gamesave-manager/internal/storage"
	"github.com/ZeroDevi1/gamesave-manager/internal/update"
)

// AlistService 直接调用 Alist / OpenList 服务端接口，供设置页登录和浏览目录。
type AlistService struct{}

// Login 登录 Alist 并返回 Token。
func (AlistService) Login(ctx context.Context, url, username, password string) (alist.LoginResult, error) {
	return alist.Login(ctx, url, username, password)
}

// ListDir 列出 Alist 目录。
func (AlistService) ListDir(ctx context.Context, url, token, path string) ([]alist.FileEntry, error) {
	return alist.ListDir(ctx, url, token, path)
}

// Upload 上传本地文件到 Alist。
func (AlistService) Upload(ctx context.Context, url, token, localPath, remotePath string) error {
	return alist.UploadFile(ctx, url, token, localPath, remotePath)
}

// Mkdir 在 Alist 上创建目录。
func (AlistService) Mkdir(ctx context.Context, url, token, path string) error {
	return alist.Mkdir(ctx, url, token, path)
}

// StorageService 是统一云端存储的操作与网盘授权流程。
type StorageService struct{}

// TestConnection 用尚未保存的临时配置列出根目录，验证连通性与凭证。
func (StorageService) TestConnection(ctx context.Context, cfg config.StorageConfig) (bool, error) {
	if err := storage.TestConnection(ctx, cfg); err != nil {
		return false, err
	}
	return true, nil
}

// ListDir 列出云端目录；cfg 为 null 时使用已保存的存储配置。
func (StorageService) ListDir(ctx context.Context, path string, cfg *config.StorageConfig) ([]storage.RemoteFileEntry, error) {
	return storage.ListDir(ctx, path, cfg)
}

// RefreshAllTokens 静默刷新直连网盘令牌并写回配置，有更新时返回 true。
func (StorageService) RefreshAllTokens(ctx context.Context) (bool, error) {
	return storage.RefreshAllTokens(ctx)
}

// QuarkTVGetQRCode 获取夸克 TV 扫码登录二维码。
func (StorageService) QuarkTVGetQRCode(ctx context.Context) (storage.QuarkTVQRCode, error) {
	return storage.QuarkTVGetQRCode(ctx)
}

// QuarkTVPollCode 轮询扫码授权，尚未确认时返回 null。
func (StorageService) QuarkTVPollCode(ctx context.Context, queryToken string) (*string, error) {
	return storage.QuarkTVPollCode(ctx, queryToken)
}

// QuarkTVExchange 用授权码换取访问令牌与刷新令牌。
func (StorageService) QuarkTVExchange(ctx context.Context, code string) (storage.NetdiskTokens, error) {
	return storage.QuarkTVExchange(ctx, code)
}

// OplistAuth 打开 api.oplist.org 授权小窗，自动选择 driver 对应的网盘并点击“获取 Token”，
// 用户扫码完成后返回捕获的令牌；用户关闭窗口时返回 null。
func (StorageService) OplistAuth(ctx context.Context, driver string) (*storage.NetdiskTokens, error) {
	return runOplistAuth(ctx, driver)
}

// GamesService 管理已添加的游戏。
type GamesService struct{}

// List 返回游戏列表（会为缺少封面的游戏补上 Steam CDN 地址）。
func (GamesService) List() ([]config.GameConfig, error) { return game.GetGames() }

// Add 手动添加游戏。
func (GamesService) Add(name string, savePaths []string) (config.GameConfig, error) {
	return game.AddGame(name, savePaths)
}

// Remove 删除游戏配置。
func (GamesService) Remove(gameId string) error { return game.RemoveGame(gameId) }

// ScanSaves 扫描游戏的本地存档文件。
func (GamesService) ScanSaves(gameId string) ([]game.SaveFile, error) { return game.ScanSaves(gameId) }

// GetLogo 返回封面的本地缓存路径，必要时按 Steam AppID 下载；取不到时返回 null。
func (GamesService) GetLogo(ctx context.Context, gameId string, steamAppid *uint64) (*string, error) {
	return game.FetchLogo(ctx, gameId, steamAppid)
}

// ExeIconResult 是从 exe 提取的图标。
type ExeIconResult struct {
	// Base64 是 PNG data URI；exe 没有图标时为 null。
	Base64 *string `json:"base64"`
	Path   string  `json:"path"`
}

// SelectAndExtractExeIcon 让用户选择游戏 exe 并提取其图标；用户取消时返回 null。
func (GamesService) SelectAndExtractExeIcon(ctx context.Context) (*ExeIconResult, error) {
	paths, err := mygo.Dialog.Open(mygo.OpenDialogOptions{
		Parent:  mygo.CallerWindow(ctx),
		Title:   "选择游戏可执行文件",
		Filters: []mygo.FileFilter{{Name: "游戏可执行文件", Extensions: []string{"exe"}}},
	})
	if err != nil || len(paths) == 0 {
		return nil, err
	}
	icon, err := game.ExtractIconDataURI(paths[0])
	if err != nil {
		return nil, err
	}
	return &ExeIconResult{Base64: icon, Path: paths[0]}, nil
}

// SaveCustomLogo 保存自定义封面（base64，可带 data URI 前缀）。
func (GamesService) SaveCustomLogo(gameId, logoBase64 string) error {
	return game.SaveCustomLogo(gameId, logoBase64)
}

// GetDbGameLogo 以 data URI 返回封面缓存，没有时返回 null。
func (GamesService) GetDbGameLogo(gameId string) (*string, error) {
	return game.CachedLogoDataURI(gameId)
}

// Launch 通过 Steam 启动游戏。
func (GamesService) Launch(steamAppid *uint64) error { return game.Launch(steamAppid) }

// LaunchWithSync 启动前同步存档，启动游戏并在需要时监控进程以便退出后自动备份。
func (GamesService) LaunchWithSync(ctx context.Context, gameId string) (game.LaunchWithSyncResult, error) {
	return game.LaunchWithSync(ctx, gameId)
}

// UpdateSettings 更新单个游戏的 exe 名与自动备份/确认覆盖设置，null 表示不修改。
func (GamesService) UpdateSettings(gameId string, exeName *string, autoBackupEnabled, confirmBeforeSync *bool) error {
	return game.UpdateGameSettings(gameId, exeName, autoBackupEnabled, confirmBeforeSync)
}

// GameDbService 管理游戏存档路径模板数据库。
type GameDbService struct{}

// Get 返回完整数据库。
func (GameDbService) Get() (game.GameDatabase, error) { return game.LoadDB() }

// Search 按名称、别名或 ID 搜索。
func (GameDbService) Search(query string) ([]game.GameDbEntry, error) {
	db, err := game.LoadDB()
	if err != nil {
		return nil, err
	}
	return game.SearchEntries(&db, query), nil
}

// Upsert 新增或更新条目，存档路径中的本机用户目录会折叠为占位符。
func (GameDbService) Upsert(entry game.GameDbEntry) (game.GameDbEntry, error) {
	return game.UpsertEntry(entry)
}

// Remove 删除条目。
func (GameDbService) Remove(id string) error { return game.RemoveEntry(id) }

// Export 导出数据库 JSON。
func (GameDbService) Export() (string, error) { return game.ExportDB() }

// Import 合并导入数据库 JSON。
func (GameDbService) Import(json string) (bool, error) {
	if err := game.ImportDB(json); err != nil {
		return false, err
	}
	return true, nil
}

// CreateGame 用数据库条目创建本地游戏。
func (GameDbService) CreateGame(ctx context.Context, dbId string) (config.GameConfig, error) {
	return game.CreateFromDB(ctx, dbId)
}

// RefreshSavePaths 从 PCGamingWiki 为数据库条目补充新的存档路径。
func (GameDbService) RefreshSavePaths(ctx context.Context) ([]game.DbPathUpdate, error) {
	return game.RefreshDBSavePaths(ctx)
}

// PcgwService 查询 PCGamingWiki 与 Steam 商店。
type PcgwService struct{}

// Search 按页面名搜索 PCGamingWiki。
func (PcgwService) Search(ctx context.Context, query string) ([]game.PcgwSearchResult, error) {
	return game.SearchPcgw(ctx, query)
}

// FetchSavePaths 解析 PCGamingWiki 页面的 Windows 存档路径。
func (PcgwService) FetchSavePaths(ctx context.Context, pageName string) (game.PcgwGameDetail, error) {
	return game.FetchSavePaths(ctx, pageName)
}

// SearchSteamStore 搜索 Steam 商店（中文结果翻译为英文官方名）。
func (PcgwService) SearchSteamStore(ctx context.Context, query string) ([]game.SteamStoreItem, error) {
	return game.SearchSteamStore(ctx, query)
}

// SearchSteamStoreRobust 多策略搜索 Steam 商店。
func (PcgwService) SearchSteamStoreRobust(ctx context.Context, query string) (game.SteamSearchResult, error) {
	return game.SearchSteamStoreRobust(ctx, query)
}

// SearchBySteamAppid 按 Steam AppID 查询 PCGamingWiki 页面。
func (PcgwService) SearchBySteamAppid(ctx context.Context, appid uint64) ([]game.PcgwSearchResult, error) {
	return game.SearchPcgwBySteamAppID(ctx, appid)
}

// BackupService 执行备份与还原。
type BackupService struct{}

// Full 执行全量备份。
func (BackupService) Full(ctx context.Context, gameId string) (backup.BackupResult, error) {
	return backup.FullBackup(ctx, gameId)
}

// Incremental 执行增量备份。
func (BackupService) Incremental(ctx context.Context, gameId string) (backup.BackupResult, error) {
	return backup.IncrementalBackup(ctx, gameId)
}

// Restore 按本地清单时间戳还原全量备份。
func (BackupService) Restore(ctx context.Context, gameId, backupTimestamp string) (backup.RestoreResult, error) {
	return backup.RestoreByTimestamp(ctx, gameId, backupTimestamp)
}

// History 返回本地备份清单。
func (BackupService) History(gameId string) ([]backup.BackupManifest, error) {
	return backup.LoadManifests(gameId)
}

// ListRemote 列出云端全量备份 ZIP。
func (BackupService) ListRemote(ctx context.Context, gameId string) ([]backup.RemoteBackupEntry, error) {
	return backup.ListRemoteBackups(ctx, gameId)
}

// RestoreRemote 从云端 ZIP 还原。
func (BackupService) RestoreRemote(ctx context.Context, gameId, remoteZipPath string) (backup.RestoreResult, error) {
	return backup.RestoreRemote(ctx, gameId, remoteZipPath)
}

// CheckAllForChanges 返回有未备份变更的游戏。
func (BackupService) CheckAllForChanges() ([]backup.GameChange, error) {
	return backup.CheckAllGamesForChanges()
}

// BackupAllChanged 增量备份所有有变更的游戏。
func (BackupService) BackupAllChanged(ctx context.Context) ([]backup.BatchBackupResult, error) {
	return backup.BackupAllChangedGames(ctx)
}

// ConfigService 读写应用配置。
type ConfigService struct{}

// Load 读取配置。
func (ConfigService) Load() (config.AppConfig, error) { return config.Load() }

// Save 保存配置。前端未回传的设置字段取默认值。
func (ConfigService) Save(cfg config.AppConfig) (bool, error) {
	if err := config.Save(&cfg); err != nil {
		return false, err
	}
	return true, nil
}

// AppService 提供版本与更新检查。
type AppService struct{}

// Version 返回应用版本。
func (AppService) Version() string { return mygo.App.Version() }

// CheckForUpdates 对照 GitHub Releases 检查更新。
func (AppService) CheckForUpdates(ctx context.Context) (update.CheckResult, error) {
	return update.Check(ctx, mygo.App.Version())
}

// OpenReleasePage 在默认浏览器打开本仓库的发布页；其它地址会被替换为 Releases 列表页。
func (AppService) OpenReleasePage(url string) error {
	return mygo.Shell.OpenExternal(update.ReleasePageURL(url))
}
