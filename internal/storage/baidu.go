package storage

import (
	"bytes"
	"context"
	"crypto/md5"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"mime/multipart"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/ZeroDevi1/gamesave-manager/internal/netutil"
)

// baiduSliceSize 是百度普通用户的标准分片大小（4 MiB），也是 block_list MD5 的切分单位。
const baiduSliceSize = 4 * 1024 * 1024

const baiduUA = "pan.baidu.com"

var baiduClient = &http.Client{}

// baiduPost 以表单 POST 到百度接口，返回 HTTP 状态与响应文本。
func baiduPost(ctx context.Context, endpoint string, form url.Values) (int, string, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, strings.NewReader(form.Encode()))
	if err != nil {
		return 0, "", err
	}
	req.Header.Set("User-Agent", baiduUA)
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	resp, err := baiduClient.Do(req)
	if err != nil {
		return 0, "", err
	}
	return resp.StatusCode, netutil.ReadBodyString(resp), nil
}

func baiduGet(ctx context.Context, endpoint string) (*http.Response, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", baiduUA)
	return baiduClient.Do(req)
}

func (b *netdiskBackend) xpanURL(method string) string {
	return "https://pan.baidu.com/rest/2.0/xpan/file?method=" + method + "&access_token=" + b.cfg.Token
}

// baiduMkdir 创建目录；目录已存在（HTTP 失败且 error_code 31061）视为成功。
func (b *netdiskBackend) baiduMkdir(ctx context.Context, dir string) error {
	status, body, err := baiduPost(ctx, b.xpanURL("mkdir"), url.Values{"path": {"/" + strings.TrimLeft(dir, "/")}})
	if err != nil {
		return err
	}
	if status < 200 || status >= 300 {
		var v struct {
			ErrorCode int64 `json:"error_code"`
		}
		if json.Unmarshal([]byte(body), &v) == nil && v.ErrorCode == 31061 {
			slog.Info("[BaiduNetdisk] 目录已存在，跳过创建", "path", dir)
			return nil
		}
		return fmt.Errorf("百度网盘直连创建目录失败 (HTTP %d): %s", status, body)
	}
	var r struct {
		Errno int `json:"errno"`
	}
	if err := json.Unmarshal([]byte(body), &r); err != nil {
		return fmt.Errorf("解析百度网盘响应失败 (%v): %s", err, body)
	}
	if r.Errno != 0 {
		return fmt.Errorf("百度网盘创建目录接口报错 (errno %d): 请尝试重新一键授权刷新 Token。", r.Errno)
	}
	return nil
}

// baiduUpload 按官方 xpan 流程上传：precreate → locateupload（取上传服务器）→ 逐分片上传 → create。
// 不使用已被降级的旧版 PCS 简易上传端点。百度拒绝空文件。
func (b *netdiskBackend) baiduUpload(ctx context.Context, localPath, remotePath string) error {
	cleanPath := "/" + strings.TrimLeft(remotePath, "/")
	token := b.cfg.Token

	// 1. 读取文件并按 4 MiB 切片计算 MD5
	data, err := os.ReadFile(localPath)
	if err != nil {
		return err
	}
	if len(data) == 0 {
		return errors.New("百度网盘不允许上传空文件")
	}
	var md5s []string
	for start := 0; start < len(data); start += baiduSliceSize {
		end := min(start+baiduSliceSize, len(data))
		sum := md5.Sum(data[start:end])
		md5s = append(md5s, hex.EncodeToString(sum[:]))
	}
	blockListJSON, _ := json.Marshal(md5s)
	sizeStr := strconv.Itoa(len(data))

	// 2. precreate：注册上传任务；return_type=2 表示秒传命中
	_, preBody, err := baiduPost(ctx, b.xpanURL("precreate"), url.Values{
		"path": {cleanPath}, "size": {sizeStr}, "isdir": {"0"}, "autoinit": {"1"},
		"block_list": {string(blockListJSON)}, "rtype": {"3"},
	})
	if err != nil {
		return err
	}
	var pre struct {
		Errno      int    `json:"errno"`
		UploadID   string `json:"uploadid"`
		BlockList  []int  `json:"block_list"`
		ReturnType int    `json:"return_type"`
	}
	if err := json.Unmarshal([]byte(preBody), &pre); err != nil {
		return fmt.Errorf("解析百度 precreate 响应失败 (%v): %s", err, preBody)
	}
	if pre.Errno != 0 {
		return fmt.Errorf("百度网盘预创建失败 (errno %d): %s", pre.Errno, preBody)
	}
	if pre.ReturnType == 2 {
		slog.Info("[BaiduNetdisk] 文件秒传命中，跳过实际上传", "path", remotePath)
		return nil
	}
	if pre.UploadID == "" {
		return fmt.Errorf("precreate 响应中缺少 uploadid: %s", preBody)
	}
	if pre.BlockList == nil {
		return errors.New("precreate 响应中缺少 block_list")
	}

	// 3. locateupload：取动态上传服务器，失败时回退到 d.pcs.baidu.com
	locateURL := fmt.Sprintf("https://d.pcs.baidu.com/rest/2.0/pcs/file?method=locateupload&access_token=%s&appid=250528&uploadid=%s&path=%s&upload_version=2.0",
		token, queryEscape(pre.UploadID), queryEscape(cleanPath))
	resp, err := baiduGet(ctx, locateURL)
	if err != nil {
		return err
	}
	locateBody := netutil.ReadBodyString(resp)
	var locate struct {
		Servers []struct {
			Server string `json:"server"`
		} `json:"servers"`
	}
	if err := json.Unmarshal([]byte(locateBody), &locate); err != nil {
		return fmt.Errorf("解析百度 locateupload 响应失败 (%v): %s", err, locateBody)
	}
	server := "https://d.pcs.baidu.com"
	if len(locate.Servers) > 0 {
		server = locate.Servers[0].Server
	} else {
		slog.Warn("[BaiduNetdisk] locateupload 未返回服务器列表，使用默认上传域名 d.pcs.baidu.com")
	}
	slog.Info("[BaiduNetdisk] 开始上传", "server", server, "slices", len(pre.BlockList), "size", len(data))

	// 4. 逐分片以 multipart/form-data 上传（文件内容放在 file 字段）
	for _, seq := range pre.BlockList {
		start := seq * baiduSliceSize
		if start < 0 || start >= len(data) {
			return fmt.Errorf("百度网盘返回的分片序号越界: %d", seq)
		}
		end := min(start+baiduSliceSize, len(data))
		if err := b.baiduUploadSlice(ctx, server, cleanPath, pre.UploadID, seq, len(pre.BlockList), data[start:end]); err != nil {
			return err
		}
	}

	// 5. create：合并分片并持久化文件
	_, createBody, err := baiduPost(ctx, b.xpanURL("create"), url.Values{
		"path": {cleanPath}, "size": {sizeStr}, "isdir": {"0"}, "uploadid": {pre.UploadID},
		"block_list": {string(blockListJSON)}, "rtype": {"3"},
	})
	if err != nil {
		return err
	}
	var created struct {
		Errno int64 `json:"errno"`
	}
	if err := json.Unmarshal([]byte(createBody), &created); err != nil {
		return fmt.Errorf("解析百度 create 响应失败 (%v): %s", err, createBody)
	}
	if created.Errno != 0 {
		return fmt.Errorf("百度网盘创建文件失败: %s", createBody)
	}
	slog.Info("[BaiduNetdisk] 上传完成", "path", remotePath)
	return nil
}

func (b *netdiskBackend) baiduUploadSlice(ctx context.Context, server, cleanPath, uploadID string, seq, total int, slice []byte) error {
	var body bytes.Buffer
	mw := multipart.NewWriter(&body)
	part, err := mw.CreateFormFile("file", fmt.Sprintf("part_%d", seq))
	if err != nil {
		return err
	}
	if _, err := part.Write(slice); err != nil {
		return err
	}
	if err := mw.Close(); err != nil {
		return err
	}
	endpoint := fmt.Sprintf("%s/rest/2.0/pcs/superfile2?method=upload&access_token=%s&type=tmpfile&path=%s&uploadid=%s&partseq=%d",
		server, b.cfg.Token, queryEscape(cleanPath), queryEscape(uploadID), seq)
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, &body)
	if err != nil {
		return err
	}
	req.Header.Set("User-Agent", baiduUA)
	req.Header.Set("Content-Type", mw.FormDataContentType())
	resp, err := baiduClient.Do(req)
	if err != nil {
		return err
	}
	text := netutil.ReadBodyString(resp)
	var r struct {
		ErrorCode int     `json:"error_code"`
		ErrorMsg  *string `json:"error_msg"`
	}
	if err := json.Unmarshal([]byte(text), &r); err != nil {
		return fmt.Errorf("解析百度分片上传响应失败 (分片%d): %v", seq, err)
	}
	if r.ErrorCode != 0 {
		msg := text
		if r.ErrorMsg != nil {
			msg = *r.ErrorMsg
		}
		return fmt.Errorf("百度网盘分片上传失败 (分片 %d/%d, error_code %d): %s", seq+1, total, r.ErrorCode, msg)
	}
	return nil
}

// baiduDownload 经内容下载域名 c.pcs.baidu.com 下载文件。
func (b *netdiskBackend) baiduDownload(ctx context.Context, remotePath, localPath string) error {
	endpoint := fmt.Sprintf("https://c.pcs.baidu.com/rest/2.0/pcs/file?method=download&access_token=%s&path=%s",
		b.cfg.Token, queryEscape("/"+strings.TrimLeft(remotePath, "/")))
	resp, err := baiduGet(ctx, endpoint)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		raw, _ := io.ReadAll(resp.Body)
		hint := ""
		var v struct {
			ErrorCode int64 `json:"error_code"`
		}
		if json.Unmarshal(raw, &v) == nil && v.ErrorCode == 31212 {
			hint = "：百度 PCS 下载服务暂时不可用，请稍后重试"
		}
		return fmt.Errorf("百度网盘直连下载失败 (HTTP %d)%s：%s", resp.StatusCode, hint, raw)
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

// baiduListDir 列出目录；errno -9（目录不存在）返回空列表。
func (b *netdiskBackend) baiduListDir(ctx context.Context, dir string) ([]RemoteFileEntry, error) {
	resp, err := baiduGet(ctx, b.xpanURL("list")+"&dir="+queryEscape(absPath(dir)))
	if err != nil {
		return nil, err
	}
	text := netutil.ReadBodyString(resp)
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return nil, fmt.Errorf("百度网盘直连列目录失败 (HTTP %d): %s", resp.StatusCode, text)
	}
	var r struct {
		Errno int `json:"errno"`
		List  []struct {
			ServerFilename string `json:"server_filename"`
			Path           string `json:"path"`
			IsDir          int    `json:"isdir"`
			Size           int64  `json:"size"`
			ServerMtime    int64  `json:"server_mtime"`
		} `json:"list"`
	}
	if err := json.Unmarshal([]byte(text), &r); err != nil {
		return nil, fmt.Errorf("解析百度网盘响应失败 (%v): %s", err, text)
	}
	if r.Errno == -9 {
		return []RemoteFileEntry{}, nil
	}
	if r.Errno != 0 {
		return nil, fmt.Errorf("百度网盘接口报错 (errno %d): 请尝试重新一键授权刷新 Token。", r.Errno)
	}
	out := make([]RemoteFileEntry, 0, len(r.List))
	for _, e := range r.List {
		mod := time.Unix(e.ServerMtime, 0).UTC().Format(time.RFC3339)
		out = append(out, RemoteFileEntry{Name: e.ServerFilename, Path: e.Path, IsDir: e.IsDir == 1, Size: e.Size, Modified: &mod})
	}
	return out, nil
}

func (b *netdiskBackend) baiduDelete(ctx context.Context, remotePath string) error {
	fileList, _ := json.Marshal([]map[string]string{{"path": absPath(remotePath)}})
	status, text, err := baiduPost(ctx,
		"https://pan.baidu.com/rest/2.0/xpan/file?method=filemanager&opera=delete&access_token="+b.cfg.Token,
		url.Values{"async": {"0"}, "filelist": {string(fileList)}, "ondup": {"1"}})
	if err != nil {
		return err
	}
	if status < 200 || status >= 300 {
		return fmt.Errorf("百度网盘删除失败 (HTTP %d): %s", status, text)
	}
	var r struct {
		Errno *int64 `json:"errno"`
	}
	if err := json.Unmarshal([]byte(text), &r); err != nil {
		return fmt.Errorf("解析百度网盘删除响应失败 (%v): %s", err, text)
	}
	if r.Errno == nil || *r.Errno != 0 {
		errno := int64(-1)
		if r.Errno != nil {
			errno = *r.Errno
		}
		return fmt.Errorf("百度网盘删除失败 (errno %d): %s", errno, text)
	}
	return nil
}
