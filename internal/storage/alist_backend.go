package storage

import (
	"context"
	"errors"
	"log/slog"
	"strings"
	"sync"

	"github.com/ZeroDevi1/gamesave-manager/internal/alist"
	"github.com/ZeroDevi1/gamesave-manager/internal/config"
)

// alistBackend 包装 Alist/OpenList 客户端：Token 缺失时用保存的密码登录，
// 请求遇到登录态失效时用密码重新登录并重试一次。
type alistBackend struct {
	cfg config.StorageConfig

	mu sync.Mutex
	// liveToken 是本次会话重新登录得到的 Token，优先于 cfg.Token；由 PersistSession 写回配置。
	liveToken string
}

func newAlistBackend(cfg config.StorageConfig) *alistBackend { return &alistBackend{cfg: cfg} }

func (b *alistBackend) setLive(token string) {
	b.mu.Lock()
	b.liveToken = token
	b.mu.Unlock()
}

func (b *alistBackend) live() string {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.liveToken
}

func (b *alistBackend) login(ctx context.Context) (string, error) {
	res, err := alist.Login(ctx, b.cfg.BaseURL, b.cfg.Username, b.cfg.Password)
	if err != nil {
		return "", err
	}
	b.setLive(res.Token)
	return res.Token, nil
}

// token 依次取会话 Token、配置中的 Token；两者皆空但有密码时自动登录。
func (b *alistBackend) token(ctx context.Context) (string, error) {
	if t := b.live(); strings.TrimSpace(t) != "" {
		return t, nil
	}
	if strings.TrimSpace(b.cfg.Token) != "" {
		return b.cfg.Token, nil
	}
	if strings.TrimSpace(b.cfg.Password) != "" {
		slog.Info("[Alist] 授权 Token 为空，正在通过用户名/密码自动认证换取 Token...")
		return b.login(ctx)
	}
	return "", errors.New("Alist 认证失败: 授权 Token 令牌与密码均为空，请进入设置界面补全云端同步参数")
}

// relogin 在登录态失效后用保存的密码重新登录；未保存密码时返回提示用户重新登录的错误。
func (b *alistBackend) relogin(ctx context.Context) (string, error) {
	if strings.TrimSpace(b.cfg.Password) == "" {
		return "", errors.New("Alist/OpenList 登录已失效，且未保存密码，请到设置页重新登录")
	}
	slog.Info("[Alist] Token 失效，正在使用已保存密码重新登录...")
	return b.login(ctx)
}

// withRetry 用当前 Token 执行 op；遇到登录态失效时重新登录并重试一次。
func (b *alistBackend) withRetry(ctx context.Context, op func(token string) error) error {
	token, err := b.token(ctx)
	if err != nil {
		return err
	}
	err = op(token)
	if !alist.IsAuthFailure(err) {
		return err
	}
	token, err = b.relogin(ctx)
	if err != nil {
		return err
	}
	return op(token)
}

func (b *alistBackend) Mkdir(ctx context.Context, dir string) error {
	return b.withRetry(ctx, func(t string) error { return alist.Mkdir(ctx, b.cfg.BaseURL, t, dir) })
}

func (b *alistBackend) UploadFile(ctx context.Context, localPath, remotePath string) error {
	return b.withRetry(ctx, func(t string) error {
		return alist.UploadFile(ctx, b.cfg.BaseURL, t, localPath, remotePath)
	})
}

func (b *alistBackend) DownloadFile(ctx context.Context, remotePath, localPath string) error {
	return b.withRetry(ctx, func(t string) error {
		return alist.DownloadFile(ctx, b.cfg.BaseURL, t, remotePath, localPath)
	})
}

func (b *alistBackend) ListDir(ctx context.Context, dir string) ([]RemoteFileEntry, error) {
	var raw []alist.FileEntry
	err := b.withRetry(ctx, func(t string) error {
		var err error
		raw, err = alist.ListDir(ctx, b.cfg.BaseURL, t, dir)
		return err
	})
	if err != nil {
		return nil, err
	}
	out := make([]RemoteFileEntry, 0, len(raw))
	for _, e := range raw {
		out = append(out, RemoteFileEntry{Name: e.Name, Path: e.Path, IsDir: e.IsDir, Size: int64(e.Size), Modified: e.Modified})
	}
	return out, nil
}

func (b *alistBackend) Delete(ctx context.Context, remotePath string) error {
	return b.withRetry(ctx, func(t string) error { return alist.Remove(ctx, b.cfg.BaseURL, t, remotePath) })
}

func (b *alistBackend) applySession(sc *config.StorageConfig) bool {
	t := b.live()
	if strings.TrimSpace(t) == "" || sc.Type != config.StorageAlist || sc.Token == t {
		return false
	}
	sc.Token = t
	return true
}
