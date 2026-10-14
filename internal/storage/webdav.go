package storage

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"

	"github.com/ZeroDevi1/gamesave-manager/internal/netutil"
)

// webdavBackend 是不依赖第三方库的 WebDAV 后端，用正则解析 PROPFIND 的 XML 响应。
type webdavBackend struct {
	endpoint string
	username string
	password string
}

func newWebdav(endpoint, username, password string) *webdavBackend {
	return &webdavBackend{endpoint: strings.TrimRight(endpoint, "/"), username: username, password: password}
}

var webdavClient = &http.Client{}

// url 拼接服务地址与远程路径；路径逐段 URL 编码，以支持空格与中文。
func (b *webdavBackend) url(p string) string {
	segs := strings.Split(p, "/")
	for i, s := range segs {
		segs[i] = url.PathEscape(s)
	}
	return b.endpoint + strings.Join(segs, "/")
}

// do 发送带鉴权的请求。密码像 JWT（以 "ey" 开头）或长度超过 30 时按 Bearer Token 发送
// （api.oplist.org 网关的 Access Token），否则使用 Basic Auth。
func (b *webdavBackend) do(ctx context.Context, method, rawURL string, body io.Reader, size int64, header http.Header) (*http.Response, error) {
	req, err := http.NewRequestWithContext(ctx, method, rawURL, body)
	if err != nil {
		return nil, err
	}
	if size >= 0 {
		req.ContentLength = size
	}
	for k, v := range header {
		req.Header[k] = v
	}
	if strings.HasPrefix(b.password, "ey") || len(b.password) > 30 {
		req.Header.Set("Authorization", "Bearer "+b.password)
	} else {
		req.SetBasicAuth(b.username, b.password)
	}
	return webdavClient.Do(req)
}

// Mkdir 逐级 MKCOL 创建目录：MKCOL 不会创建缺失的上级目录（会返回 409）。
// 405 表示目录已存在；其它非 201 状态只记录警告，不中断流程。
func (b *webdavBackend) Mkdir(ctx context.Context, dir string) error {
	clean := strings.Trim(dir, "/")
	if clean == "" {
		return nil
	}
	current := ""
	for _, seg := range strings.Split(clean, "/") {
		if seg == "" {
			continue
		}
		current += "/" + seg
		resp, err := b.do(ctx, "MKCOL", b.url(current), nil, -1, nil)
		if err != nil {
			return err
		}
		text := netutil.ReadBodyString(resp)
		if resp.StatusCode != http.StatusCreated && resp.StatusCode != http.StatusMethodNotAllowed {
			slog.Warn("WebDAV 级联 MKCOL 响应非预期", "status", resp.StatusCode, "body", text)
		}
	}
	return nil
}

// UploadFile 以显式 Content-Length PUT 上传；0 字节文件改为上传 1 个 \0 字节，规避部分网盘拒收空文件。
func (b *webdavBackend) UploadFile(ctx context.Context, localPath, remotePath string) error {
	info, err := os.Stat(localPath)
	if err != nil {
		return err
	}
	var body io.Reader
	size := info.Size()
	if size == 0 {
		body, size = bytes.NewReader([]byte{0}), 1
	} else {
		f, err := os.Open(localPath)
		if err != nil {
			return err
		}
		defer f.Close()
		body = f
	}
	resp, err := b.do(ctx, http.MethodPut, b.url(remotePath), body, size, http.Header{"Content-Type": {"application/octet-stream"}})
	if err != nil {
		return err
	}
	text := netutil.ReadBodyString(resp)
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("WebDAV 物理上传失败 (HTTP %d): %s", resp.StatusCode, text)
	}
	return nil
}

func (b *webdavBackend) DownloadFile(ctx context.Context, remotePath, localPath string) error {
	resp, err := b.do(ctx, http.MethodGet, b.url(remotePath), nil, -1, nil)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		text, _ := io.ReadAll(resp.Body)
		return fmt.Errorf("从 WebDAV 下载备份失败 (HTTP %d): %s", resp.StatusCode, text)
	}
	if err := os.MkdirAll(filepath.Dir(localPath), 0o755); err != nil {
		return err
	}
	f, err := os.Create(localPath)
	if err != nil {
		return err
	}
	if _, err := io.Copy(f, resp.Body); err != nil {
		f.Close()
		return err
	}
	return f.Close()
}

func (b *webdavBackend) Delete(ctx context.Context, remotePath string) error {
	resp, err := b.do(ctx, http.MethodDelete, b.url(remotePath), nil, -1, nil)
	if err != nil {
		return err
	}
	text := netutil.ReadBodyString(resp)
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("WebDAV 删除失败 (HTTP %d): %s", resp.StatusCode, text)
	}
	return nil
}

// 兼容带命名空间前缀（d:、D:、a: 等）与不带前缀的 PROPFIND 响应。
var (
	reDavResponse = regexp.MustCompile(`(?s)<[^>]*:?response[^>]*>(.*?)</[^>]*:?response>`)
	reDavHref     = regexp.MustCompile(`<[^>]*:?href[^>]*>([^<]+)</[^>]*:?href>`)
	reDavLength   = regexp.MustCompile(`<[^>]*:?getcontentlength[^>]*>(\d+)</[^>]*:?getcontentlength>`)
	reDavModified = regexp.MustCompile(`<[^>]*:?getlastmodified[^>]*>([^<]+)</[^>]*:?getlastmodified>`)
	reDavIsDir    = regexp.MustCompile(`<[^>]*:?collection[^>]*>`)
)

// ListDir 以 Depth: 1 的 PROPFIND 列出目录，并滤除目录自身那一条记录。
// 返回条目的 Path 是 URL 解码后的 href。
func (b *webdavBackend) ListDir(ctx context.Context, dir string) ([]RemoteFileEntry, error) {
	resp, err := b.do(ctx, "PROPFIND", b.url(dir), nil, -1, http.Header{
		"Depth":        {"1"},
		"Content-Type": {"application/xml; charset=utf-8"},
	})
	if err != nil {
		return nil, err
	}
	xml := netutil.ReadBodyString(resp)
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return nil, fmt.Errorf("列出 WebDAV 目录失败 (HTTP %d): %s", resp.StatusCode, xml)
	}

	endpointURL, err := url.Parse(b.endpoint)
	if err != nil {
		return nil, err
	}
	basePath := strings.TrimRight(endpointURL.Path, "/")
	selfViaBase := strings.TrimRight(strings.ReplaceAll(basePath+dir, "//", "/"), "/")
	selfDirect := strings.TrimRight(dir, "/")

	entries := []RemoteFileEntry{}
	for _, m := range reDavResponse.FindAllStringSubmatch(xml, -1) {
		body := m[1]
		hm := reDavHref.FindStringSubmatch(body)
		if hm == nil {
			continue
		}
		href := hm[1]
		if decoded, err := url.PathUnescape(href); err == nil {
			href = decoded
		}
		clean := strings.TrimRight(href, "/")
		if clean == selfViaBase || clean == selfDirect || clean == basePath {
			continue
		}
		name := clean[strings.LastIndex(clean, "/")+1:]

		var size int64
		if lm := reDavLength.FindStringSubmatch(body); lm != nil {
			size, _ = strconv.ParseInt(lm[1], 10, 64)
		}
		var modified *string
		if mm := reDavModified.FindStringSubmatch(body); mm != nil {
			v := mm[1]
			modified = &v
		}
		entries = append(entries, RemoteFileEntry{
			Name:     name,
			Path:     href,
			IsDir:    reDavIsDir.MatchString(body),
			Size:     size,
			Modified: modified,
		})
	}
	return entries, nil
}
