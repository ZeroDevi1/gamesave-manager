// Package alist 是 Alist / OpenList 服务端 HTTP API 的客户端：登录、列目录、上传、下载、建目录与删除。
package alist

import (
	"bytes"
	"context"
	"crypto/md5"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"net/url"
	"os"
	"path"
	"path/filepath"
	"strings"
	"time"

	"github.com/ZeroDevi1/gamesave-manager/internal/netutil"
)

// LoginResult 是登录结果。
type LoginResult struct {
	Token string `json:"token"`
}

// FileEntry 是目录中的一项。
type FileEntry struct {
	Name     string  `json:"name"`
	Path     string  `json:"path"`
	Size     uint64  `json:"size"`
	IsDir    bool    `json:"is_dir"`
	Modified *string `json:"modified,omitempty"`
}

// UnmarshalJSON 在服务端缺少 is_dir 字段时按目录处理（与 Tauri 版的默认值一致）。
func (e *FileEntry) UnmarshalJSON(data []byte) error {
	type plain FileEntry
	v := plain{IsDir: true}
	if err := json.Unmarshal(data, &v); err != nil {
		return err
	}
	*e = FileEntry(v)
	return nil
}

// apiResponse 是 Alist API 的通用响应包装。
type apiResponse struct {
	Code    int             `json:"code"`
	Message string          `json:"message"`
	Data    json.RawMessage `json:"data"`
}

var defaultClient = &http.Client{}

func base(u string) string { return strings.TrimRight(u, "/") }

// postJSON 以 JSON 请求体 POST 到 Alist API，返回 HTTP 状态码与响应文本。
func postJSON(ctx context.Context, client *http.Client, url, token string, body any) (int, string, error) {
	payload, err := json.Marshal(body)
	if err != nil {
		return 0, "", err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(payload))
	if err != nil {
		return 0, "", err
	}
	req.Header.Set("Content-Type", "application/json; charset=utf-8")
	req.Header.Set("Accept", "application/json")
	if token != "" {
		req.Header.Set("Authorization", token)
	}
	resp, err := client.Do(req)
	if err != nil {
		return 0, "", err
	}
	return resp.StatusCode, netutil.ReadBodyString(resp), nil
}

// Login 登录 Alist：先用 MD5 哈希密码请求 /api/auth/login/hash，失败后回退到明文 /api/auth/login
// （兼容部分 OpenList 与旧版 Alist）。两次都失败时返回同时包含两条服务端消息的错误。
func Login(ctx context.Context, baseURL, username, password string) (LoginResult, error) {
	b := base(baseURL)
	sum := md5.Sum([]byte(password))
	hashed := hex.EncodeToString(sum[:])

	hashResp := loginOnce(ctx, b+"/api/auth/login/hash", username, hashed)
	if hashResp.Code == 200 {
		token, err := extractToken(hashResp)
		return LoginResult{Token: token}, err
	}
	slog.Info("[Alist login/hash] 失败，尝试明文登录", "code", hashResp.Code, "msg", hashResp.Message)

	plainResp := loginOnce(ctx, b+"/api/auth/login", username, password)
	if plainResp.Code == 200 {
		token, err := extractToken(plainResp)
		return LoginResult{Token: token}, err
	}
	return LoginResult{}, fmt.Errorf("login/hash: %s | login: %s", hashResp.Message, plainResp.Message)
}

// loginOnce 请求一次登录接口；网络错误或响应无法解析时返回 code=-1，message 为错误或原始响应。
func loginOnce(ctx context.Context, url, username, password string) apiResponse {
	_, text, err := postJSON(ctx, defaultClient, url, "", map[string]string{"username": username, "password": password})
	if err != nil {
		return apiResponse{Code: -1, Message: err.Error()}
	}
	var r apiResponse
	if json.Unmarshal([]byte(text), &r) != nil {
		return apiResponse{Code: -1, Message: text}
	}
	return r
}

func extractToken(r apiResponse) (string, error) {
	var d struct {
		Token string `json:"token"`
	}
	if json.Unmarshal(r.Data, &d) != nil || d.Token == "" {
		return "", errors.New("响应中未找到 token")
	}
	return d.Token, nil
}

// ListDir 列出目录内容；空目录（服务端返回 content: null）返回空切片。
// 服务端未返回条目 path 时按 dir 补全。登录态失效时错误信息以 "AUTH_EXPIRED" 开头。
func ListDir(ctx context.Context, baseURL, token, dir string) ([]FileEntry, error) {
	status, text, err := postJSON(ctx, defaultClient, base(baseURL)+"/api/fs/list", token, map[string]any{
		"path": dir, "password": "", "page": 1, "per_page": 0, "refresh": false,
	})
	if err != nil {
		return nil, err
	}
	if status == 401 || status == 403 {
		return nil, fmt.Errorf("AUTH_EXPIRED: 列出目录失败 (HTTP %d): %s", status, netutil.Preview(text, 150))
	}
	if status < 200 || status >= 300 {
		return nil, fmt.Errorf("中转网关响应非预期 (HTTP %d): %s。请检查您的网盘授权是否被限制，或 Token 是否已失效。", status, netutil.Preview(text, 150))
	}
	var r apiResponse
	if err := json.Unmarshal([]byte(text), &r); err != nil {
		return nil, fmt.Errorf("解析中转网关响应失败 (%v): %s", err, netutil.Preview(text, 150))
	}
	if r.Code == 401 || r.Code == 403 {
		return nil, fmt.Errorf("AUTH_EXPIRED: 列出目录失败: %s", r.Message)
	}
	if r.Code != 200 {
		return nil, fmt.Errorf("列出目录失败: %s", r.Message)
	}
	var data struct {
		Content json.RawMessage `json:"content"`
	}
	if json.Unmarshal(r.Data, &data) != nil || len(data.Content) == 0 {
		return nil, errors.New("响应中未找到 content")
	}
	if string(data.Content) == "null" {
		return []FileEntry{}, nil
	}
	var entries []FileEntry
	if err := json.Unmarshal(data.Content, &entries); err != nil {
		return nil, err
	}
	for i := range entries {
		if entries[i].Path == "" {
			sep := "/"
			if strings.HasSuffix(dir, "/") || dir == "/" {
				sep = ""
			}
			entries[i].Path = strings.TrimRight(dir, "/") + sep + entries[i].Name
		}
	}
	return entries, nil
}

// encodeFSPath 对路径逐段做 URL 编码，保留 "/" 分隔符。
func encodeFSPath(p string) string {
	segs := strings.Split(p, "/")
	for i, s := range segs {
		segs[i] = url.PathEscape(s)
	}
	return strings.Join(segs, "/")
}

// checkWriteResponse 解析写操作（上传、建目录）的响应：JSON 业务码非 200 视为失败；
// 响应不是 JSON 时（如代理层返回 413）按 HTTP 状态判断。
func checkWriteResponse(action string, status int, text string) error {
	var r apiResponse
	if json.Unmarshal([]byte(text), &r) == nil {
		if r.Code != 200 {
			return fmt.Errorf("%s失败 (业务代码 %d): %s", action, r.Code, r.Message)
		}
		return nil
	}
	if status < 200 || status >= 300 {
		return fmt.Errorf("%s失败 (HTTP %d): %s", action, status, text)
	}
	return nil
}

// UploadFile 通过 PUT /api/fs/put 上传本地文件到 remotePath。
//
// 显式设置 Content-Length 而不用分块传输：百度网盘驱动的预上传接口需要提前知道文件大小。
// 0 字节文件改为上传 1 个 \0 字节，因为多数网盘拒绝空文件。
// File-Path 头逐段 URL 编码，服务端会解码，以支持中文等非 ASCII 文件名。
func UploadFile(ctx context.Context, baseURL, token, localPath, remotePath string) error {
	info, err := os.Stat(localPath)
	if err != nil {
		return err
	}
	var req *http.Request
	size := info.Size()
	endpoint := base(baseURL) + "/api/fs/put"
	if size == 0 {
		req, err = http.NewRequestWithContext(ctx, http.MethodPut, endpoint, bytes.NewReader([]byte{0}))
		size = 1
	} else {
		f, ferr := os.Open(localPath)
		if ferr != nil {
			return ferr
		}
		defer f.Close()
		req, err = http.NewRequestWithContext(ctx, http.MethodPut, endpoint, f)
	}
	if err != nil {
		return err
	}
	req.ContentLength = size
	req.Header.Set("Authorization", token)
	req.Header.Set("File-Path", encodeFSPath(remotePath))
	req.Header.Set("Content-Type", "application/octet-stream")
	req.Header.Set("Content-Disposition", fmt.Sprintf("attachment; filename=%q", filepath.Base(localPath)))

	resp, err := defaultClient.Do(req)
	if err != nil {
		return err
	}
	text := netutil.ReadBodyString(resp)
	return checkWriteResponse("上传", resp.StatusCode, text)
}

// Mkdir 创建目录（服务端负责创建中间目录）。
func Mkdir(ctx context.Context, baseURL, token, dir string) error {
	status, text, err := postJSON(ctx, defaultClient, base(baseURL)+"/api/fs/mkdir", token, map[string]string{"path": dir})
	if err != nil {
		return err
	}
	return checkWriteResponse("创建目录", status, text)
}

// Remove 删除 remotePath 指向的文件或目录。
func Remove(ctx context.Context, baseURL, token, remotePath string) error {
	trimmed := strings.TrimRight(remotePath, "/")
	name := path.Base(trimmed)
	if name == "" || name == "/" || name == "." {
		return fmt.Errorf("无法从路径提取文件名: %s", remotePath)
	}
	dir := path.Dir(trimmed)
	status, text, err := postJSON(ctx, defaultClient, base(baseURL)+"/api/fs/remove", token, map[string]any{
		"dir": dir, "names": []string{name},
	})
	if err != nil {
		return err
	}
	if status < 200 || status >= 300 {
		return fmt.Errorf("删除失败 (HTTP %d): %s", status, text)
	}
	var r apiResponse
	if err := json.Unmarshal([]byte(text), &r); err != nil {
		return fmt.Errorf("解析删除响应失败 (%v): %s", err, text)
	}
	if r.Code != 200 {
		return fmt.Errorf("删除失败: %s", r.Message)
	}
	return nil
}

// IsAuthFailure 判断错误是否表示登录态失效，调用方可重新登录后重试一次。
func IsAuthFailure(err error) bool {
	if err == nil {
		return false
	}
	s := err.Error()
	lower := strings.ToLower(s)
	return strings.Contains(s, "AUTH_EXPIRED") ||
		strings.Contains(lower, "http 401") ||
		strings.Contains(lower, "unauthorized") ||
		strings.Contains(lower, "guest user is disabled") ||
		strings.Contains(lower, "password has been changed")
}

var apiClient = &http.Client{Timeout: 60 * time.Second}

// resolveDownloadSource 解析真实下载地址与所需请求头。
//
// `/d/` 直链走签名中间件、不认 JWT，开启 sign_all 的实例直接 GET 会返回 401，
// 因此依次尝试：/api/fs/link（直链 + 上游 Cookie）→ /api/fs/get 的 raw_url → 带 sign 的 /p/ 代理地址。
func resolveDownloadSource(ctx context.Context, b, token, remotePath string) (string, http.Header, error) {
	// 1. /api/fs/link：网络错误或 404 时静默回退到 /api/fs/get
	if status, text, err := postJSON(ctx, apiClient, b+"/api/fs/link", token, map[string]string{"path": remotePath}); err == nil {
		if status == 401 || status == 403 {
			return "", nil, fmt.Errorf("AUTH_EXPIRED: HTTP %d", status)
		}
		if status != 404 {
			var r apiResponse
			if json.Unmarshal([]byte(text), &r) == nil {
				if r.Code == 401 || r.Code == 403 {
					return "", nil, fmt.Errorf("AUTH_EXPIRED: %s", r.Message)
				}
				if r.Code == 200 {
					var d struct {
						URL    string          `json:"url"`
						Header json.RawMessage `json:"header"`
					}
					if json.Unmarshal(r.Data, &d) == nil && strings.TrimSpace(d.URL) != "" {
						slog.Info("[Alist] 使用 /api/fs/link 下载", "url", d.URL)
						return d.URL, netutil.ParseHeaderMap(d.Header), nil
					}
				} else {
					slog.Warn("[Alist] /api/fs/link 未返回可用直链", "code", r.Code, "msg", r.Message)
				}
			} else {
				slog.Warn("[Alist] /api/fs/link 响应无法解析", "status", status, "body", netutil.Head(text, 200))
			}
		}
	}

	// 2. /api/fs/get：优先 raw_url，其次签名代理地址
	status, text, err := postJSON(ctx, apiClient, b+"/api/fs/get", token, map[string]string{"path": remotePath, "password": ""})
	if err != nil {
		return "", nil, err
	}
	if status == 401 || status == 403 {
		return "", nil, fmt.Errorf("AUTH_EXPIRED: HTTP %d", status)
	}
	var r apiResponse
	if err := json.Unmarshal([]byte(text), &r); err != nil {
		return "", nil, fmt.Errorf("解析 /api/fs/get 失败 (%v): %s", err, netutil.Head(text, 200))
	}
	if r.Code == 401 || r.Code == 403 {
		return "", nil, fmt.Errorf("AUTH_EXPIRED: %s", r.Message)
	}
	if r.Code != 200 {
		return "", nil, fmt.Errorf("获取下载地址失败: %s", r.Message)
	}
	var d struct {
		RawURL string          `json:"raw_url"`
		Header json.RawMessage `json:"header"`
		Sign   string          `json:"sign"`
	}
	_ = json.Unmarshal(r.Data, &d)
	if d.RawURL != "" {
		slog.Info("[Alist] 使用 /api/fs/get raw_url 下载", "url", d.RawURL)
		return d.RawURL, netutil.ParseHeaderMap(d.Header), nil
	}
	if d.Sign != "" {
		abs := remotePath
		if !strings.HasPrefix(abs, "/") {
			abs = "/" + abs
		}
		fallback := fmt.Sprintf("%s/p%s?sign=%s", b, encodeFSPath(abs), d.Sign)
		slog.Info("[Alist] 使用签名代理地址下载", "url", fallback)
		return fallback, http.Header{}, nil
	}
	return "", nil, errors.New("OpenList 未返回 raw_url 或 sign，无法下载。请确认账号对该路径有读取权限。")
}

// DownloadFile 下载 remotePath 到 localPath。下载地址属于该 Alist 实例自身时附带 JWT；
// 上游 CDN 不会收到它（跨域重定向会剥离 Authorization）。
func DownloadFile(ctx context.Context, baseURL, token, remotePath, localPath string) error {
	b := base(baseURL)
	downloadURL, headers, err := resolveDownloadSource(ctx, b, token, remotePath)
	if err != nil {
		return err
	}
	if strings.HasPrefix(downloadURL, b) {
		headers.Set("Authorization", token)
	}
	if err := netutil.DownloadToFile(ctx, downloadURL, headers, localPath); err != nil {
		msg := err.Error()
		if strings.Contains(msg, "401") || strings.Contains(strings.ToLower(msg), "unauthorized") {
			return fmt.Errorf("AUTH_EXPIRED: %s", msg)
		}
		return fmt.Errorf("下载备份文件失败: %s", msg)
	}
	return nil
}
