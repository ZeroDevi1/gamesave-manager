// Package netutil 提供跨网盘复用的 HTTP 辅助：带鉴权头的流式下载与响应片段截断。
package netutil

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// maxRedirects 是手动跟随重定向的上限，与 Tauri 版一致；夸克直链通常只跳 1~2 次。
const maxRedirects = 8

// noRedirectClient 不自动跟随重定向，由 DownloadToFile 逐跳决定是否保留敏感头。
var noRedirectClient = &http.Client{
	Timeout: 600 * time.Second,
	CheckRedirect: func(*http.Request, []*http.Request) error {
		return http.ErrUseLastResponse
	},
}

// Preview 截断 s 到至多 n 个字节（按字符边界），超出时追加 "..."，用于错误信息中的响应片段。
func Preview(s string, n int) string {
	if len(s) <= n {
		return s
	}
	cut := n
	for cut > 0 && !utf8Start(s[cut]) {
		cut--
	}
	return s[:cut] + "..."
}

// Head 截断 s 到至多 n 个字节（按字符边界），不追加省略号。
func Head(s string, n int) string {
	if len(s) <= n {
		return s
	}
	cut := n
	for cut > 0 && !utf8Start(s[cut]) {
		cut--
	}
	return s[:cut]
}

func utf8Start(b byte) bool { return b&0xC0 != 0x80 }

// ParseHeaderMap 把 OpenList `/api/fs/link` 返回的 header 字段解析为请求头。
//
// 兼容三种形态：值为字符串的 JSON 对象、值为字符串数组的 JSON 对象，
// 以及 "Cookie: ...\nUser-Agent: ..." 形式的文本；其它形态返回空头。
func ParseHeaderMap(raw json.RawMessage) http.Header {
	h := http.Header{}
	if len(raw) == 0 {
		return h
	}
	var obj map[string]json.RawMessage
	if json.Unmarshal(raw, &obj) == nil {
		for k, v := range obj {
			var s string
			if json.Unmarshal(v, &s) == nil {
				h.Add(k, s)
				continue
			}
			var arr []any
			if json.Unmarshal(v, &arr) == nil {
				for _, item := range arr {
					if str, ok := item.(string); ok {
						h.Add(k, str)
					}
				}
			}
		}
		return h
	}
	var text string
	if json.Unmarshal(raw, &text) == nil && strings.TrimSpace(text) != "" {
		for _, line := range strings.Split(text, "\n") {
			k, v, ok := strings.Cut(line, ":")
			if ok && strings.TrimSpace(k) != "" {
				h.Add(strings.TrimSpace(k), strings.TrimSpace(v))
			}
		}
	}
	return h
}

func isQuarkFamily(host string) bool {
	return host == "quark.cn" || strings.HasSuffix(host, ".quark.cn")
}

// shouldForwardSensitive 判断重定向时是否继续携带 Cookie/Authorization。
// 夸克直链会从 drive.quark.cn 302 到 dl-*.pds.quark.cn，若剥掉 Cookie，
// PDS 回调会返回 `require login [auth miss]`；因此同源或同属夸克域时保留。
func shouldForwardSensitive(from, to *url.URL) bool {
	a, b := strings.ToLower(from.Hostname()), strings.ToLower(to.Hostname())
	if a == "" || b == "" {
		return false
	}
	return a == b || (isQuarkFamily(a) && isQuarkFamily(b))
}

// DownloadToFile 下载 rawURL 并流式写入 localPath（自动创建父目录）。
//
// 手动跟随至多 8 次 http(s) 重定向：跨域跳转时剥离 Cookie 与 Authorization，
// 同源或夸克下载域之间保留。非 2xx 响应返回含 HTTP 状态与响应片段的错误。
func DownloadToFile(ctx context.Context, rawURL string, headers http.Header, localPath string) error {
	current, err := url.Parse(rawURL)
	if err != nil {
		return fmt.Errorf("无效下载地址: %s", rawURL)
	}
	headers = headers.Clone()
	if headers == nil {
		headers = http.Header{}
	}

	for hops := 1; ; hops++ {
		if hops > maxRedirects {
			return fmt.Errorf("下载重定向次数过多: %s", current)
		}
		req, err := http.NewRequestWithContext(ctx, http.MethodGet, current.String(), nil)
		if err != nil {
			return err
		}
		req.Header = headers.Clone()
		resp, err := noRedirectClient.Do(req)
		if err != nil {
			return fmt.Errorf("请求下载地址失败: %s: %w", current, err)
		}

		if resp.StatusCode >= 300 && resp.StatusCode < 400 {
			resp.Body.Close()
			location := resp.Header.Get("Location")
			if location == "" {
				return fmt.Errorf("下载重定向缺少 Location: HTTP %d", resp.StatusCode)
			}
			next, err := current.Parse(location)
			if err != nil {
				return fmt.Errorf("无法解析重定向: %s", location)
			}
			if next.Scheme != "http" && next.Scheme != "https" {
				return fmt.Errorf("拒绝跟随非 HTTP 重定向: %s", next)
			}
			if !shouldForwardSensitive(current, next) {
				headers.Del("Cookie")
				headers.Del("Authorization")
			}
			slog.Debug("[download] 重定向", "from", current.String(), "to", next.String(), "status", resp.StatusCode)
			current = next
			continue
		}

		if resp.StatusCode < 200 || resp.StatusCode >= 300 {
			body, _ := io.ReadAll(resp.Body)
			resp.Body.Close()
			return fmt.Errorf("下载失败 (HTTP %d): %s", resp.StatusCode, Preview(string(body), 800))
		}

		err = writeBody(resp.Body, localPath)
		resp.Body.Close()
		if err != nil {
			return fmt.Errorf("读取下载流失败: %s: %w", current, err)
		}
		return nil
	}
}

func writeBody(body io.Reader, localPath string) error {
	if err := os.MkdirAll(filepath.Dir(localPath), 0o755); err != nil {
		return err
	}
	f, err := os.Create(localPath)
	if err != nil {
		return err
	}
	if _, err := io.Copy(f, body); err != nil {
		f.Close()
		return err
	}
	return f.Close()
}

// ReadBodyString 读取并关闭响应体，读取失败时返回空串（与 Rust 版 `text().unwrap_or_default()` 一致）。
func ReadBodyString(resp *http.Response) string {
	defer resp.Body.Close()
	b, _ := io.ReadAll(resp.Body)
	return string(b)
}
