package storage

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"strings"
	"time"

	"github.com/ZeroDevi1/gamesave-manager/internal/config"
	"github.com/ZeroDevi1/gamesave-manager/internal/netutil"
)

var refreshClient = &http.Client{Timeout: 30 * time.Second}

// RefreshNetdiskToken 经 api.oplist.org 的 GET /<driver>/renewapi 刷新直连网盘令牌，
// ClientSecret 等密钥只存在于网关侧。响应未返回新的刷新令牌时沿用原值。夸克不支持刷新。
func RefreshNetdiskToken(ctx context.Context, driver, refreshToken string) (NetdiskTokens, error) {
	if strings.Contains(driver, "quark") {
		return NetdiskTokens{}, errors.New("夸克网盘不支持自动刷新 Token，Cookie 模式无需刷新，TV 模式请重新扫码")
	}
	clean := cleanDriver(driver)
	req, err := http.NewRequestWithContext(ctx, http.MethodGet,
		fmt.Sprintf("https://api.oplist.org/%s/renewapi?refresh_token=%s", clean, queryEscape(refreshToken)), nil)
	if err != nil {
		return NetdiskTokens{}, err
	}
	resp, err := refreshClient.Do(req)
	if err != nil {
		return NetdiskTokens{}, err
	}
	text := netutil.ReadBodyString(resp)
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return NetdiskTokens{}, fmt.Errorf("中转端刷新 %s Token 失败 (HTTP %d): %s", clean, resp.StatusCode, text)
	}
	var v struct {
		AccessToken  *string `json:"access_token"`
		RefreshToken *string `json:"refresh_token"`
	}
	if err := json.Unmarshal([]byte(text), &v); err != nil {
		return NetdiskTokens{}, err
	}
	if v.AccessToken == nil {
		return NetdiskTokens{}, fmt.Errorf("刷新接口响应中未找到 access_token 字段: %s", text)
	}
	t := NetdiskTokens{AccessToken: *v.AccessToken, RefreshToken: refreshToken}
	if v.RefreshToken != nil {
		t.RefreshToken = *v.RefreshToken
	}
	slog.Info("[Netdisk] Token 刷新成功", "driver", clean)
	return t, nil
}

// RefreshAllTokens 刷新已配置直连网盘的令牌并写回配置；有更新时返回 true。
// 刷新失败不视为错误（只记录日志），读写配置失败才返回错误。
func RefreshAllTokens(ctx context.Context) (bool, error) {
	cfg, err := config.Load()
	if err != nil {
		return false, err
	}
	sc := cfg.Storage
	if sc == nil || sc.Type != config.StorageNetdisk || strings.TrimSpace(sc.RefreshToken) == "" {
		return false, nil
	}
	slog.Info("[Netdisk] 正在静默刷新 Token", "driver", sc.Driver)
	t, err := RefreshNetdiskToken(ctx, sc.Driver, sc.RefreshToken)
	if err != nil {
		slog.Warn("[Netdisk] Token 静默刷新失败（非致命，跳过）", "err", err)
		return false, nil
	}
	sc.Token, sc.RefreshToken = t.AccessToken, t.RefreshToken
	if err := config.Save(&cfg); err != nil {
		return false, err
	}
	return true, nil
}

// TestConnection 用临时配置列出根目录，验证地址与凭证可用。
func TestConnection(ctx context.Context, sc config.StorageConfig) error {
	if _, err := New(sc).ListDir(ctx, "/"); err != nil {
		return fmt.Errorf("云端存储连接测试失败: %v", err)
	}
	return nil
}

// ListDir 列出云端目录。sc 非 nil 时使用该临时配置（配置向导）；为 nil 时使用已保存的配置，
// 并在结束后回写刷新过的会话凭证。
func ListDir(ctx context.Context, dir string, sc *config.StorageConfig) ([]RemoteFileEntry, error) {
	if sc != nil {
		return New(*sc).ListDir(ctx, dir)
	}
	cfg, err := config.Load()
	if err != nil {
		return nil, err
	}
	b, err := FromConfig(&cfg)
	if err != nil {
		return nil, err
	}
	entries, err := b.ListDir(ctx, dir)
	PersistSession(b)
	return entries, err
}
