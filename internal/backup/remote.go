package backup

import (
	"context"
	"encoding/json"
	"log/slog"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/ZeroDevi1/gamesave-manager/internal/config"
	"github.com/ZeroDevi1/gamesave-manager/internal/storage"
)

// RemoteManifest 是云端 <游戏目录>/manifest.json 的内容：最近一次备份的文件清单，
// 供启动游戏前与本地存档比对，而无需下载整个存档。
type RemoteManifest struct {
	GameID    string      `json:"game_id"`
	Timestamp time.Time   `json:"timestamp"`
	Files     []FileEntry `json:"files"`
}

func remoteManifestPath(cfg *config.AppConfig, g *config.GameConfig) string {
	return strings.TrimRight(cfg.GameRemotePath(g), "/") + "/manifest.json"
}

// uploadRemoteManifest 经临时文件上传云端 manifest.json。
// 先删除旧文件：百度网盘等上传同名文件时会生成带编号的副本而不是覆盖。
func uploadRemoteManifest(ctx context.Context, cfg *config.AppConfig, g *config.GameConfig, files []FileEntry, ts time.Time) error {
	data, err := json.MarshalIndent(RemoteManifest{GameID: g.ID, Timestamp: ts, Files: files}, "", "  ")
	if err != nil {
		return err
	}
	tmp := filepath.Join(os.TempDir(), "gamesave_manifest_"+g.ID+".json")
	if err := os.WriteFile(tmp, data, 0o644); err != nil {
		return err
	}
	defer os.Remove(tmp)

	remotePath := remoteManifestPath(cfg, g)
	backend, err := storage.FromConfig(cfg)
	if err != nil {
		return err
	}
	defer storage.PersistSession(backend)
	_ = backend.Mkdir(ctx, path.Dir(remotePath))
	if err := backend.Delete(ctx, remotePath); err != nil {
		slog.Debug("[远端清单] 删除旧 manifest 失败（可能不存在）", "err", err)
	}
	if err := backend.UploadFile(ctx, tmp, remotePath); err != nil {
		return err
	}
	slog.Info("[远端清单] 已上传", "game", g.ID, "path", remotePath)
	return nil
}

// isNotFound 判断各后端的“文件不存在”错误。
func isNotFound(err error) bool {
	msg := strings.ToLower(err.Error())
	for _, s := range []string{"not found", "object not found", "路径不存在", "404", "no such file", "文件不存在"} {
		if strings.Contains(msg, s) {
			return true
		}
	}
	return false
}

// downloadRemoteManifest 下载并解析云端 manifest.json；云端不存在时返回 nil。
func downloadRemoteManifest(ctx context.Context, cfg *config.AppConfig, g *config.GameConfig) (*RemoteManifest, error) {
	backend, err := storage.FromConfig(cfg)
	if err != nil {
		return nil, err
	}
	defer storage.PersistSession(backend)
	tmp := filepath.Join(os.TempDir(), "gamesave_manifest_remote_"+g.ID+".json")
	defer os.Remove(tmp)
	if err := backend.DownloadFile(ctx, remoteManifestPath(cfg, g), tmp); err != nil {
		if isNotFound(err) {
			return nil, nil
		}
		return nil, err
	}
	data, err := os.ReadFile(tmp)
	if err != nil {
		return nil, err
	}
	var m RemoteManifest
	if err := json.Unmarshal(data, &m); err != nil {
		return nil, err
	}
	return &m, nil
}

// cleanupOldBackups 按 Settings.BackupRetentionLimit 删除云端多余的全量 ZIP（按修改时间）与增量目录（按目录名即时间戳），
// 并把本地清单裁剪到同样数量。未设置上限时不做任何事；单项删除失败只记录日志。
func cleanupOldBackups(ctx context.Context, cfg *config.AppConfig, g *config.GameConfig) {
	limitPtr := cfg.Settings.BackupRetentionLimit
	if limitPtr == nil || *limitPtr == 0 {
		return
	}
	limit := int(*limitPtr)
	backend, err := storage.FromConfig(cfg)
	if err != nil {
		return
	}
	defer storage.PersistSession(backend)
	base := cfg.GameRemotePath(g)

	// 1. 全量备份 ZIP：修改时间新的在前，未知时间排在最后
	fullDir := remoteDir(base, "full")
	if entries, err := backend.ListDir(ctx, fullDir); err == nil {
		var zips []storage.RemoteFileEntry
		for _, e := range entries {
			if !e.IsDir && strings.HasSuffix(e.Name, ".zip") {
				zips = append(zips, e)
			}
		}
		sort.SliceStable(zips, func(i, j int) bool { return modKey(zips[i]) > modKey(zips[j]) })
		for _, old := range tail(zips, limit) {
			deleteLogged(ctx, backend, fullDir+old.Name, "全量备份")
		}
	}

	// 2. 增量目录：目录名是时间戳，按名称降序即按时间降序
	incDir := remoteDir(base, "incremental")
	if entries, err := backend.ListDir(ctx, incDir); err == nil {
		var dirs []storage.RemoteFileEntry
		for _, e := range entries {
			if e.IsDir {
				dirs = append(dirs, e)
			}
		}
		sort.SliceStable(dirs, func(i, j int) bool { return dirs[i].Name > dirs[j].Name })
		for _, old := range tail(dirs, limit) {
			deleteLogged(ctx, backend, incDir+old.Name+"/", "增量目录")
		}
	}

	// 3. 本地清单只保留最新的若干条
	if err := pruneManifests(g.ID, max(limit, 1)); err != nil {
		slog.Warn("[清理旧备份] 裁剪本地清单失败", "err", err)
	}
}

func modKey(e storage.RemoteFileEntry) string {
	if e.Modified == nil {
		return ""
	}
	return *e.Modified
}

func tail[T any](s []T, keep int) []T {
	if len(s) <= keep {
		return nil
	}
	return s[keep:]
}

func deleteLogged(ctx context.Context, b storage.Backend, p, kind string) {
	if err := b.Delete(ctx, p); err != nil {
		slog.Warn("[清理旧备份] 删除失败", "kind", kind, "path", p, "err", err)
		return
	}
	slog.Info("[清理旧备份] 已删除", "kind", kind, "path", p)
}
