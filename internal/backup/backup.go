package backup

import (
	"archive/zip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"log/slog"
	"os"
	"path"
	"path/filepath"
	"strings"
	"time"

	"github.com/ZeroDevi1/gamesave-manager/internal/config"
	"github.com/ZeroDevi1/gamesave-manager/internal/pathutil"
	"github.com/ZeroDevi1/gamesave-manager/internal/storage"
)

// BackupResult 是一次备份的结果。
type BackupResult struct {
	Success bool   `json:"success"`
	Message string `json:"message"`
	// FilesBackedUp 是全量备份打包的文件数，或增量备份上传的变更文件数。
	FilesBackedUp int `json:"files_backed_up"`
	// Timestamp 是备份时间戳文本（UTC，20060102_150405）。
	Timestamp string `json:"timestamp"`
}

func loadGame(gameID string) (config.AppConfig, *config.GameConfig, error) {
	cfg, err := config.Load()
	if err != nil {
		return cfg, nil, err
	}
	g := cfg.FindGame(gameID)
	if g == nil {
		return cfg, nil, fmt.Errorf("未找到游戏: %s", gameID)
	}
	return cfg, g, nil
}

// fileSHA256 流式计算文件的 SHA256 十六进制摘要。
func fileSHA256(p string) (string, error) {
	f, err := os.Open(p)
	if err != nil {
		return "", err
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

func entryOf(f pathutil.LocalFile, sum string) FileEntry {
	return FileEntry{RelativePath: f.Rel, Size: uint64(f.Info.Size()), ModifiedTime: f.Info.ModTime().UTC(), SHA256: sum}
}

// sanitizeFilename 把文件名中 Windows 与网盘不接受的字符及空格替换为下划线
// （NTFS 会把冒号解释为备用数据流，百度网盘会返回 errno -7）。
func sanitizeFilename(name string) string {
	return strings.Map(func(r rune) rune {
		switch r {
		case ':', '*', '?', '"', '<', '>', '|', '\\', '/', ' ':
			return '_'
		}
		return r
	}, name)
}

func tempDir(elem ...string) (string, error) {
	dir := filepath.Join(append([]string{os.TempDir(), "gamesave-manager"}, elem...)...)
	return dir, os.MkdirAll(dir, 0o755)
}

// remoteDir 拼接游戏云端目录下的子目录，结果以 "/" 结尾。
func remoteDir(base string, sub ...string) string {
	return strings.TrimRight(base, "/") + "/" + strings.Join(sub, "/") + "/"
}

// FullBackup 把游戏全部存档打包为 ZIP 上传到 <云端目录>/full/，记录清单，
// 并更新云端 manifest.json、按保留数量清理旧备份。远端清单与清理失败只记录日志。
func FullBackup(ctx context.Context, gameID string) (BackupResult, error) {
	cfg, game, err := loadGame(gameID)
	if err != nil {
		return BackupResult{}, err
	}
	now := time.Now().UTC()
	ts := FormatTimestamp(now)
	zipName := fmt.Sprintf("%s_%s.zip", sanitizeFilename(game.Name), ts)

	// 1. 打包到本地临时 ZIP
	tmp, err := tempDir()
	if err != nil {
		return BackupResult{}, err
	}
	zipPath := filepath.Join(tmp, zipName)
	defer os.Remove(zipPath)
	files, err := writeZip(zipPath, game.SavePaths)
	if err != nil {
		return BackupResult{}, err
	}

	// 2. 上传
	dir := remoteDir(cfg.GameRemotePath(game), "full")
	remotePath := dir + zipName
	backend, err := storage.FromConfig(&cfg)
	if err != nil {
		return BackupResult{}, err
	}
	err = backend.Mkdir(ctx, dir)
	if err == nil {
		err = backend.UploadFile(ctx, zipPath, remotePath)
	}
	storage.PersistSession(backend)
	if err != nil {
		return BackupResult{}, err
	}

	// 3. 记录清单、同步云端清单、清理旧备份
	m := BackupManifest{GameID: gameID, BackupType: BackupFull, Timestamp: now, Files: files, TargetPath: remotePath, ZipFile: &remotePath}
	if err := saveManifest(m); err != nil {
		return BackupResult{}, err
	}
	if err := uploadRemoteManifest(ctx, &cfg, game, files, now); err != nil {
		slog.Warn("[全量备份] 远端清单上传失败（不影响备份本身）", "err", err)
	}
	cleanupOldBackups(ctx, &cfg, game)

	return BackupResult{Success: true, Message: "全量备份完成", FilesBackedUp: len(files), Timestamp: ts}, nil
}

// writeZip 把存档文件以相对路径写入 ZIP（Deflate），返回写入文件的清单条目。
func writeZip(zipPath string, savePaths []string) ([]FileEntry, error) {
	out, err := os.Create(zipPath)
	if err != nil {
		return nil, err
	}
	zw := zip.NewWriter(out)
	files := []FileEntry{}
	walkErr := pathutil.WalkSaveFiles(savePaths, true, func(f pathutil.LocalFile) error {
		sum, err := fileSHA256(f.Path)
		if err != nil {
			return err
		}
		hdr, err := zip.FileInfoHeader(f.Info)
		if err != nil {
			return err
		}
		hdr.Name = f.Rel
		hdr.Method = zip.Deflate
		w, err := zw.CreateHeader(hdr)
		if err != nil {
			return err
		}
		src, err := os.Open(f.Path)
		if err != nil {
			return err
		}
		_, err = io.Copy(w, src)
		src.Close()
		if err != nil {
			return err
		}
		files = append(files, entryOf(f, sum))
		return nil
	})
	closeErr := zw.Close()
	fileErr := out.Close()
	for _, e := range []error{walkErr, closeErr, fileErr} {
		if e != nil {
			return nil, e
		}
	}
	return files, nil
}

// IncrementalBackup 只上传自上次备份以来修改时间或内容变化的文件，
// 上传到 <云端目录>/incremental/<时间戳>/<相对路径>。
//
// 无论是否有变化都会追加清单（已删除的文件不再出现在其中）；有变化时才更新云端 manifest.json 并清理旧备份。
func IncrementalBackup(ctx context.Context, gameID string) (BackupResult, error) {
	cfg, game, err := loadGame(gameID)
	if err != nil {
		return BackupResult{}, err
	}
	now := time.Now().UTC()
	ts := FormatTimestamp(now)

	last, err := LatestManifest(gameID)
	if err != nil {
		return BackupResult{}, err
	}
	lastFiles := fileIndex(last)
	base := remoteDir(cfg.GameRemotePath(game), "incremental", ts)

	var backend storage.Backend
	defer func() {
		if backend != nil {
			storage.PersistSession(backend)
		}
	}()

	files := []FileEntry{}
	changedCount := 0
	err = pathutil.WalkSaveFiles(game.SavePaths, false, func(f pathutil.LocalFile) error {
		sum, err := fileSHA256(f.Path)
		if err != nil {
			return err
		}
		entry := entryOf(f, sum)
		if changed(lastFiles, entry) {
			changedCount++
			if backend == nil {
				if backend, err = storage.FromConfig(&cfg); err != nil {
					return err
				}
			}
			remotePath := base + f.Rel
			// 父目录可能已存在，创建失败不中断；真正的问题会在上传时暴露
			_ = backend.Mkdir(ctx, path.Dir(remotePath))
			if err := backend.UploadFile(ctx, f.Path, remotePath); err != nil {
				return err
			}
		}
		files = append(files, entry)
		return nil
	})
	if err != nil {
		return BackupResult{}, err
	}

	m := BackupManifest{GameID: gameID, BackupType: BackupIncremental, Timestamp: now, Files: files, TargetPath: strings.TrimRight(base, "/")}
	if err := saveManifest(m); err != nil {
		return BackupResult{}, err
	}
	if changedCount > 0 {
		if err := uploadRemoteManifest(ctx, &cfg, game, files, now); err != nil {
			slog.Warn("[增量备份] 远端清单上传失败（不影响备份本身）", "err", err)
		}
		cleanupOldBackups(ctx, &cfg, game)
	}
	return BackupResult{
		Success:       true,
		Message:       fmt.Sprintf("增量备份完成，上传 %d 个变更文件", changedCount),
		FilesBackedUp: changedCount,
		Timestamp:     ts,
	}, nil
}
