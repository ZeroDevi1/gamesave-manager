package game

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/ZeroDevi1/gamesave-manager/internal/appdir"
	"github.com/ZeroDevi1/gamesave-manager/internal/config"
)

var logoClient = &http.Client{Timeout: 30 * time.Second}

// logoPath 返回 id 对应的封面缓存文件 {data}/cache/logos/<id>.png。
func logoPath(id string) (string, error) {
	dir, err := appdir.LogosDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, id+".png"), nil
}

// FetchLogo 返回游戏封面的本地缓存路径：已有缓存直接返回；否则按 Steam AppID 下载，
// 依次尝试 SteamGridDB（需配置 API Key）与 Steam 公开 CDN 的竖版海报、横版头图。
// 没有 AppID 或全部来源失败时返回 nil。
func FetchLogo(ctx context.Context, id string, steamAppID *uint64) (*string, error) {
	cached, err := logoPath(id)
	if err != nil {
		return nil, err
	}
	if _, err := os.Stat(cached); err == nil {
		return &cached, nil
	}
	if steamAppID == nil {
		return nil, nil
	}
	appid := *steamAppID

	if cfg, err := config.Load(); err == nil && cfg.Settings.SteamGridDBAPIKey != nil {
		if url := sgdbGridURL(ctx, appid, *cfg.Settings.SteamGridDBAPIKey); url != "" && downloadTo(ctx, url, cached) == nil {
			return &cached, nil
		}
	}
	for _, url := range []string{
		fmt.Sprintf("https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/%d/library_600x900.jpg", appid),
		fmt.Sprintf("https://cdn.akamai.steamstatic.com/steam/apps/%d/header.jpg", appid),
	} {
		if downloadTo(ctx, url, cached) == nil {
			return &cached, nil
		}
	}
	return nil, nil
}

// downloadTo 下载 url 的完整内容后写入 dst；非 2xx 或任何错误时不写文件。
func downloadTo(ctx context.Context, url, dst string) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return err
	}
	resp, err := logoClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("HTTP %d", resp.StatusCode)
	}
	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return err
	}
	return os.WriteFile(dst, data, 0o644)
}

// sgdbGridURL 经 SteamGridDB 查询 AppID 对应游戏的第一张网格图地址；任何失败返回空串。
func sgdbGridURL(ctx context.Context, appid uint64, apiKey string) string {
	get := func(url string, out any) bool {
		req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
		if err != nil {
			return false
		}
		req.Header.Set("Authorization", "Bearer "+apiKey)
		resp, err := logoClient.Do(req)
		if err != nil {
			return false
		}
		defer resp.Body.Close()
		return resp.StatusCode >= 200 && resp.StatusCode < 300 && json.NewDecoder(resp.Body).Decode(out) == nil
	}
	var game struct {
		Data []struct {
			ID uint64 `json:"id"`
		} `json:"data"`
	}
	if !get(fmt.Sprintf("https://www.steamgriddb.com/api/v2/games/id/%d", appid), &game) || len(game.Data) == 0 {
		return ""
	}
	var grids struct {
		Data []struct {
			URL string `json:"url"`
		} `json:"data"`
	}
	if !get(fmt.Sprintf("https://www.steamgriddb.com/api/v2/grids/game/%d?dimensions=460x215,920x430", game.Data[0].ID), &grids) || len(grids.Data) == 0 {
		return ""
	}
	return grids.Data[0].URL
}

// SaveCustomLogo 把 base64 图片（可带 "data:...;base64," 前缀）写入 id 的封面缓存。
func SaveCustomLogo(id, logoBase64 string) error {
	p, err := logoPath(id)
	if err != nil {
		return err
	}
	clean := logoBase64
	if strings.HasPrefix(clean, "data:") {
		if i := strings.Index(clean, "base64,"); i >= 0 {
			clean = clean[i+len("base64,"):]
		}
	}
	data, err := base64.StdEncoding.DecodeString(clean)
	if err != nil {
		return err
	}
	return os.WriteFile(p, data, 0o644)
}

// CachedLogoDataURI 以 data URI 返回 id 的封面缓存；没有缓存时返回 nil。
func CachedLogoDataURI(id string) (*string, error) {
	p, err := logoPath(id)
	if err != nil {
		return nil, err
	}
	data, err := os.ReadFile(p)
	if os.IsNotExist(err) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	uri := "data:image/png;base64," + base64.StdEncoding.EncodeToString(data)
	return &uri, nil
}

// copyLogo 把 src 条目的封面缓存复制为 dst 的缓存，返回新文件路径；src 没有缓存或复制失败时返回 nil。
func copyLogo(srcID, dstID string) *string {
	src, err := logoPath(srcID)
	if err != nil {
		return nil
	}
	dst, err := logoPath(dstID)
	if err != nil {
		return nil
	}
	data, err := os.ReadFile(src)
	if err != nil || os.WriteFile(dst, data, 0o644) != nil {
		return nil
	}
	return &dst
}
