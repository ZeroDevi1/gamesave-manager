// Package storage 抽象云端备份存储，提供直连网盘、Alist/OpenList、WebDAV 与 S3（未实现）四类后端。
//
// 远程路径一律使用正斜杠；各后端自行处理根路径映射与非 ASCII 字符编码。
package storage

import (
	"context"
	"errors"
	"log/slog"

	"github.com/ZeroDevi1/gamesave-manager/internal/config"
)

// RemoteFileEntry 是云端目录中的一项。
type RemoteFileEntry struct {
	Name string `json:"name"`
	// Path 是可直接回传给同一后端 DownloadFile/Delete 的路径。
	Path  string `json:"path"`
	IsDir bool   `json:"is_dir"`
	// Size 单位为字节，目录为 0。
	Size int64 `json:"size"`
	// Modified 是 RFC 3339 或服务端原样返回的修改时间；未知时为 nil。
	Modified *string `json:"modified,omitempty"`
}

// Backend 是一个云端存储后端。实现须可被多个 goroutine 同时调用。
type Backend interface {
	// Mkdir 创建目录及其缺失的上级目录；目录已存在视为成功。
	Mkdir(ctx context.Context, dir string) error
	// UploadFile 上传本地文件到 remotePath。
	UploadFile(ctx context.Context, localPath, remotePath string) error
	// DownloadFile 下载 remotePath 并写入 localPath（自动创建父目录）。
	DownloadFile(ctx context.Context, remotePath, localPath string) error
	// ListDir 列出目录下的直接子项。
	ListDir(ctx context.Context, dir string) ([]RemoteFileEntry, error)
	// Delete 删除文件或目录。
	Delete(ctx context.Context, remotePath string) error
}

// sessionHolder 由会在运行中刷新凭证的后端实现（夸克 Cookie 轮换、Alist 重新登录）。
type sessionHolder interface {
	// applySession 把本次会话中更新过的凭证写入 sc；sc 有变化时返回 true。
	applySession(sc *config.StorageConfig) bool
}

// ErrNotConfigured 表示尚未配置云端存储。
var ErrNotConfigured = errors.New("未检测到有效的云端存储配置，请先进入设置页面配置云端存储后端")

// FromConfig 按应用配置创建当前启用的后端。旧版 Alist 字段已由 config.Load 迁移到 Storage。
func FromConfig(cfg *config.AppConfig) (Backend, error) {
	if cfg.Storage != nil {
		return New(*cfg.Storage), nil
	}
	return nil, ErrNotConfigured
}

// New 按存储配置创建后端，用于连接测试和配置向导中尚未保存的临时配置。
func New(sc config.StorageConfig) Backend {
	switch sc.Type {
	case config.StorageNetdisk:
		return newNetdisk(sc)
	case config.StorageAlist:
		return newAlistBackend(sc)
	case config.StorageWebdav:
		return newWebdav(sc.Endpoint, sc.Username, sc.Password)
	default:
		return s3Backend{}
	}
}

// PersistSession 把后端在本次会话中刷新的凭证（夸克 Cookie、Alist 重新登录得到的 Token）写回 config.json，
// 避免下次启动仍使用过期凭证。读写配置失败只记录日志。
func PersistSession(b Backend) {
	h, ok := b.(sessionHolder)
	if !ok {
		return
	}
	cfg, err := config.Load()
	if err != nil {
		slog.Warn("[storage] 回写会话凭证时读取配置失败", "err", err)
		return
	}
	if cfg.Storage == nil || !h.applySession(cfg.Storage) {
		return
	}
	if err := config.Save(&cfg); err != nil {
		slog.Warn("[storage] 回写会话凭证失败", "err", err)
		return
	}
	slog.Info("[storage] 已持久化更新后的会话凭证")
}
