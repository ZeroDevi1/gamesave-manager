package backup

import (
	"archive/zip"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"path"
	"path/filepath"
	"strings"
	"time"

	"github.com/ZeroDevi1/gamesave-manager/internal/config"
	"github.com/ZeroDevi1/gamesave-manager/internal/pathutil"
	"github.com/ZeroDevi1/gamesave-manager/internal/storage"
)

// RestoreResult 是一次还原的结果。
type RestoreResult struct {
	Success bool   `json:"success"`
	Message string `json:"message"`
}

// RemoteBackupEntry 是云端的一个全量备份 ZIP。
type RemoteBackupEntry struct {
	Name     string  `json:"name"`
	Path     string  `json:"path"`
	Size     uint64  `json:"size"`
	Modified *string `json:"modified,omitempty"`
}

// ListRemoteBackups 列出 <云端目录>/full/ 下的 ZIP；目录不存在（从未备份）时返回空列表。
func ListRemoteBackups(ctx context.Context, gameID string) ([]RemoteBackupEntry, error) {
	cfg, game, err := loadGame(gameID)
	if err != nil {
		return nil, err
	}
	backend, err := storage.FromConfig(&cfg)
	if err != nil {
		return nil, err
	}
	entries, err := backend.ListDir(ctx, remoteDir(cfg.GameRemotePath(game), "full"))
	storage.PersistSession(backend)
	if err != nil {
		if isNotFound(err) {
			return []RemoteBackupEntry{}, nil
		}
		return nil, err
	}
	out := []RemoteBackupEntry{}
	for _, e := range entries {
		if !e.IsDir && strings.HasSuffix(e.Name, ".zip") {
			out = append(out, RemoteBackupEntry{Name: e.Name, Path: e.Path, Size: uint64(max(e.Size, 0)), Modified: e.Modified})
		}
	}
	return out, nil
}

// RestoreByTimestamp 按本地清单的时间戳（20060102_150405）找到对应的全量备份并还原；增量备份不支持还原。
func RestoreByTimestamp(ctx context.Context, gameID, ts string) (RestoreResult, error) {
	cfg, game, err := loadGame(gameID)
	if err != nil {
		return RestoreResult{}, err
	}
	list, err := LoadManifests(gameID)
	if err != nil {
		return RestoreResult{}, err
	}
	for _, m := range list {
		if FormatTimestamp(m.Timestamp) != ts {
			continue
		}
		if m.ZipFile == nil {
			return RestoreResult{}, errors.New("该备份不支持恢复")
		}
		return restoreZip(ctx, &cfg, game, *m.ZipFile)
	}
	return RestoreResult{}, fmt.Errorf("未找到备份: %s", ts)
}

// RestoreRemote 从云端指定的 ZIP 还原，不依赖本地清单。
func RestoreRemote(ctx context.Context, gameID, remoteZipPath string) (RestoreResult, error) {
	cfg, game, err := loadGame(gameID)
	if err != nil {
		return RestoreResult{}, err
	}
	return restoreZip(ctx, &cfg, game, remoteZipPath)
}

// restoreZip 下载 ZIP，把现有存档复制为 "<路径>.bak_<时间戳>"，再把 ZIP 解压到第一个存档路径。
func restoreZip(ctx context.Context, cfg *config.AppConfig, g *config.GameConfig, remoteZipPath string) (RestoreResult, error) {
	if len(g.SavePaths) == 0 {
		return RestoreResult{}, errors.New("无存档路径")
	}

	// 1. 下载到临时目录
	tmp, err := tempDir("restore")
	if err != nil {
		return RestoreResult{}, err
	}
	localZip := filepath.Join(tmp, path.Base(strings.TrimRight(remoteZipPath, "/")))
	defer os.Remove(localZip)
	backend, err := storage.FromConfig(cfg)
	if err != nil {
		return RestoreResult{}, err
	}
	err = backend.DownloadFile(ctx, remoteZipPath, localZip)
	storage.PersistSession(backend)
	if err != nil {
		return RestoreResult{}, err
	}

	zr, err := zip.OpenReader(localZip)
	if err != nil {
		return RestoreResult{}, err
	}
	defer zr.Close()
	target, err := extractTarget(g.SavePaths[0], zr.File)
	if err != nil {
		return RestoreResult{}, err
	}

	// 2. 覆盖前备份现有存档
	ts := FormatTimestamp(time.Now())
	for _, tpl := range g.SavePaths {
		for _, p := range pathutil.ResolveSavePaths(tpl) {
			if err := copyAside(p, p+".bak_"+ts); err != nil {
				return RestoreResult{}, err
			}
		}
	}

	// 3. 解压覆盖
	if err := extractAll(zr.File, target); err != nil {
		return RestoreResult{}, err
	}
	return RestoreResult{Success: true, Message: "恢复完成"}, nil
}

// extractTarget 决定解压目录。ZIP 条目是相对存档路径本身的路径（见 pathutil.LocalFile.Rel），因此：
// 存档路径是目录时解压到该目录；是文件时解压到其所在目录。
// 路径当前不存在时，若 ZIP 只有一个与路径同名的条目则按文件处理，否则按目录创建。
func extractTarget(tpl string, entries []*zip.File) (string, error) {
	if resolved := pathutil.ResolveSavePaths(tpl); len(resolved) > 0 {
		p := resolved[0]
		if info, err := os.Stat(p); err == nil && !info.IsDir() {
			return filepath.Dir(p), nil
		}
		return p, nil
	}
	expanded := pathutil.ExpandEnv(tpl)
	if strings.ContainsAny(expanded, "*?[") {
		return "", fmt.Errorf("存档路径含通配符且当前没有匹配项，无法确定还原位置: %s", tpl)
	}
	if len(entries) == 1 && entries[0].Name == filepath.Base(expanded) {
		return filepath.Dir(expanded), nil
	}
	return expanded, nil
}

func extractAll(entries []*zip.File, target string) error {
	for _, f := range entries {
		if strings.HasSuffix(f.Name, "/") {
			continue
		}
		// 拒绝绝对路径与 ".." 条目，防止写出目标目录之外
		rel := filepath.FromSlash(f.Name)
		if filepath.IsAbs(rel) || !filepath.IsLocal(rel) {
			return fmt.Errorf("备份包含非法路径: %s", f.Name)
		}
		out := filepath.Join(target, rel)
		if err := os.MkdirAll(filepath.Dir(out), 0o755); err != nil {
			return err
		}
		if err := extractOne(f, out); err != nil {
			return err
		}
	}
	return nil
}

func extractOne(f *zip.File, out string) error {
	src, err := f.Open()
	if err != nil {
		return err
	}
	defer src.Close()
	dst, err := os.Create(out)
	if err != nil {
		return err
	}
	if _, err := io.Copy(dst, src); err != nil {
		dst.Close()
		return err
	}
	return dst.Close()
}

// copyAside 把文件或目录复制到 dst，先删除已存在的 dst。
func copyAside(src, dst string) error {
	info, err := os.Stat(src)
	if err != nil {
		return err
	}
	_ = os.RemoveAll(dst)
	if !info.IsDir() {
		return copyFile(src, dst)
	}
	return filepath.WalkDir(src, func(p string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, p)
		if err != nil {
			return err
		}
		target := filepath.Join(dst, rel)
		if d.IsDir() {
			return os.MkdirAll(target, 0o755)
		}
		return copyFile(p, target)
	})
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		return err
	}
	return out.Close()
}
