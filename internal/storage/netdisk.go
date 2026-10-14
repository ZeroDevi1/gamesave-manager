package storage

import (
	"context"
	"fmt"
	"net/url"
	"strings"
	"sync"

	"github.com/ZeroDevi1/gamesave-manager/internal/config"
)

// netdiskBackend 是免部署的直连网盘后端，按驱动名分流：
// 含 "baidu" 走百度 xpan 官方接口，含 "quark" 走夸克 drive.quark.cn 的 Cookie 接口，
// 其它驱动（阿里云盘、OneDrive 等）经 api.oplist.org 的 WebDAV 网关存取。
type netdiskBackend struct {
	cfg config.StorageConfig

	mu sync.Mutex
	// quarkCookie 是夸克 Cookie 的当前值。夸克在响应的 Set-Cookie 中轮换 __puus/__pus，
	// 每次请求后在此更新，最终由 PersistSession 写回配置。
	quarkCookie string
}

func newNetdisk(cfg config.StorageConfig) *netdiskBackend {
	return &netdiskBackend{cfg: cfg, quarkCookie: cfg.Token}
}

func (b *netdiskBackend) isBaidu() bool { return strings.Contains(b.cfg.Driver, "baidu") }
func (b *netdiskBackend) isQuark() bool { return strings.Contains(b.cfg.Driver, "quark") }

// cleanDriver 去掉驱动名的 _go/_qr/_fn 后缀，对齐 api.oplist.org 的挂载点与刷新端点命名。
func cleanDriver(driver string) string {
	d := strings.TrimSuffix(driver, "_go")
	d = strings.TrimSuffix(d, "_qr")
	return strings.TrimSuffix(d, "_fn")
}

// gatewayPath 把路径映射到网关上该驱动的挂载点 "/<clean_driver>/..."；已带前缀的路径原样返回。
// 网关不允许直接访问根目录，因此 "/" 映射为挂载点本身。
func (b *netdiskBackend) gatewayPath(p string) string {
	prefix := "/" + cleanDriver(b.cfg.Driver)
	if strings.HasPrefix(p, prefix) {
		return p
	}
	clean := strings.TrimLeft(p, "/")
	if clean == "" {
		return prefix
	}
	return prefix + "/" + clean
}

func (b *netdiskBackend) gateway() *webdavBackend {
	// 网关以 Bearer Token 鉴权，用户名只是占位
	return newWebdav("https://api.oplist.org/dav", "admin", b.cfg.Token)
}

// gatewayErr 为网关操作的错误补充请求地址、脱敏 Token 与排查建议。
func (b *netdiskBackend) gatewayErr(realPath string, err error) error {
	if err == nil {
		return nil
	}
	tokenPreview := b.cfg.Token
	if len(tokenPreview) > 8 {
		tokenPreview = tokenPreview[:8] + "..."
	}
	return fmt.Errorf("网盘 %q WebDAV 操作失败。\n  实际请求 URL: https://api.oplist.org/dav%s\n  Access Token (前8位): %s\n  排查建议: "+
		"1) 检查 Token 是否过期，尝试重新授权刷新；2) 确认 api.oplist.org 网关服务是否可正常访问；"+
		"3) 若持续失败，可能是驱动 %s 在网关侧挂载异常。\n  原始错误: %v",
		b.cfg.Driver, realPath, tokenPreview, b.cfg.Driver, err)
}

func (b *netdiskBackend) Mkdir(ctx context.Context, dir string) error {
	switch {
	case b.isBaidu():
		return b.baiduMkdir(ctx, dir)
	case b.isQuark():
		_, err := b.quarkEnsureDir(ctx, dir)
		return err
	}
	real := b.gatewayPath(dir)
	return b.gatewayErr(real, b.gateway().Mkdir(ctx, real))
}

func (b *netdiskBackend) UploadFile(ctx context.Context, localPath, remotePath string) error {
	switch {
	case b.isBaidu():
		return b.baiduUpload(ctx, localPath, remotePath)
	case b.isQuark():
		return b.quarkUpload(ctx, localPath, remotePath)
	}
	real := b.gatewayPath(remotePath)
	return b.gatewayErr(real, b.gateway().UploadFile(ctx, localPath, real))
}

func (b *netdiskBackend) DownloadFile(ctx context.Context, remotePath, localPath string) error {
	switch {
	case b.isBaidu():
		return b.baiduDownload(ctx, remotePath, localPath)
	case b.isQuark():
		return b.quarkDownload(ctx, remotePath, localPath)
	}
	real := b.gatewayPath(remotePath)
	return b.gatewayErr(real, b.gateway().DownloadFile(ctx, real, localPath))
}

func (b *netdiskBackend) ListDir(ctx context.Context, dir string) ([]RemoteFileEntry, error) {
	switch {
	case b.isQuark():
		return b.quarkListDir(ctx, dir)
	case b.isBaidu():
		return b.baiduListDir(ctx, dir)
	}
	real := b.gatewayPath(dir)
	entries, err := b.gateway().ListDir(ctx, real)
	return entries, b.gatewayErr(real, err)
}

func (b *netdiskBackend) Delete(ctx context.Context, remotePath string) error {
	switch {
	case b.isQuark():
		return b.quarkDelete(ctx, remotePath)
	case b.isBaidu():
		return b.baiduDelete(ctx, remotePath)
	}
	real := b.gatewayPath(remotePath)
	return b.gatewayErr(real, b.gateway().Delete(ctx, real))
}

func (b *netdiskBackend) applySession(sc *config.StorageConfig) bool {
	if !b.isQuark() {
		return false
	}
	b.mu.Lock()
	cookie := b.quarkCookie
	b.mu.Unlock()
	if strings.TrimSpace(cookie) == "" || sc.Type != config.StorageNetdisk ||
		!strings.Contains(sc.Driver, "quark") || sc.Token == cookie {
		return false
	}
	sc.Token = cookie
	return true
}

// absPath 把远程路径规范为以 "/" 开头的形式（百度接口要求）；空路径与 "/" 返回 "/"。
func absPath(p string) string {
	if p == "" || p == "/" {
		return "/"
	}
	return "/" + strings.TrimLeft(p, "/")
}

// queryEscape 对查询参数值编码（与 Rust urlencoding::encode 一致，空格编码为 %20）。
func queryEscape(s string) string { return strings.ReplaceAll(url.QueryEscape(s), "+", "%20") }
