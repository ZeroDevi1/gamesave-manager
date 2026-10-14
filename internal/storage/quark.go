package storage

import (
	"bytes"
	"context"
	"crypto/md5"
	"crypto/sha1"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/ZeroDevi1/gamesave-manager/internal/netutil"
)

// 夸克网盘接口常量，参考 OpenList quark_uc 驱动：
// API 使用 drive.quark.cn（不是 drive-pc.quark.cn），UA 必须带 quark-cloud-drive 标识，否则可能返回 500。
const (
	quarkAPIBase = "https://drive.quark.cn/1/clouddrive"
	quarkUA      = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) quark-cloud-drive/2.5.20 Chrome/100.0.4896.160 Electron/18.3.5.4-b478491100 Safari/537.36 Channel/pckk_other_ch"
	quarkReferer = "https://pan.quark.cn/"
	// quarkOSSUA 必须同时出现在 OSS 签名串与请求头中。
	quarkOSSUA = "aliyun-sdk-js/6.6.1 Chrome 98.0.4758.80 on Windows 10 64-bit"
	// quarkMime 须与 pre 的 format_type、签名串和 OSS PUT 的 Content-Type 一致。
	quarkMime = "application/octet-stream"
)

var (
	quarkClient     = &http.Client{Timeout: 30 * time.Second}
	quarkOSSClient  = &http.Client{Timeout: 60 * time.Second}
	quarkCommitHTTP = &http.Client{}
)

// quarkFile 是夸克目录列表中的一项。
type quarkFile struct {
	Fid      string `json:"fid"`
	FileName string `json:"file_name"`
	Size     int64  `json:"size"`
	// File 为 true 表示文件，false 表示文件夹。
	File bool `json:"file"`
	// UpdatedAt 是毫秒时间戳。
	UpdatedAt int64 `json:"updated_at"`
}

// isFid 判断路径段是否是夸克文件 ID（32 位十六进制）。
func isFid(s string) bool {
	if len(s) != 32 {
		return false
	}
	for _, c := range s {
		if !strings.ContainsRune("0123456789abcdefABCDEF", c) {
			return false
		}
	}
	return true
}

func (b *netdiskBackend) cookie() string {
	b.mu.Lock()
	defer b.mu.Unlock()
	return parseQuarkCookie(b.quarkCookie)
}

// updateCookies 从响应的 Set-Cookie 提取 __puus/__pus 并更新 Cookie 缓存；夸克在每个响应中轮换它们，
// 不更新会话很快就会过期。
func (b *netdiskBackend) updateCookies(resp *http.Response) {
	for _, sc := range resp.Header.Values("Set-Cookie") {
		pair, _, _ := strings.Cut(sc, ";")
		name, value, ok := strings.Cut(strings.TrimSpace(pair), "=")
		name, value = strings.TrimSpace(name), strings.TrimSpace(value)
		if !ok || (name != "__puus" && name != "__pus") {
			continue
		}
		b.mu.Lock()
		// 先规范为 Cookie 头格式，避免把键值追加到 Netscape 文本里
		b.quarkCookie = setCookieValue(parseQuarkCookie(b.quarkCookie), name, value)
		b.mu.Unlock()
		slog.Info("[Quark] 已从响应更新 Cookie", "name", name)
	}
}

// quarkDo 调用夸克 drive API：附加 Cookie、UA、Referer 与公共查询参数，非 2xx 返回错误。
// 错误信息只包含 Cookie 长度，不包含其内容。
func (b *netdiskBackend) quarkDo(ctx context.Context, method, path string, body any) (*http.Response, error) {
	sep := "?"
	if strings.Contains(path, "?") {
		sep = "&"
	}
	now := time.Now()
	endpoint := fmt.Sprintf("%s%s%spr=ucpro&fr=pc&uc_param_str=&__dt=%d&__t=%d",
		quarkAPIBase, path, sep, now.Nanosecond()%9900+100, now.UnixMilli())

	var reader io.Reader
	if body != nil {
		payload, err := json.Marshal(body)
		if err != nil {
			return nil, err
		}
		reader = bytes.NewReader(payload)
	}
	req, err := http.NewRequestWithContext(ctx, method, endpoint, reader)
	if err != nil {
		return nil, err
	}
	cookie := b.cookie()
	req.Header.Set("Cookie", cookie)
	req.Header.Set("User-Agent", quarkUA)
	req.Header.Set("Referer", quarkReferer)
	req.Header.Set("Accept", "application/json, text/plain, */*")
	req.Header.Set("Origin", "https://pan.quark.cn")
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}

	resp, err := quarkClient.Do(req)
	if err != nil {
		return nil, err
	}
	b.updateCookies(resp)
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		text := netutil.ReadBodyString(resp)
		slog.Warn("[Quark] 请求失败", "status", resp.StatusCode, "body", netutil.Head(text, 300))
		return nil, fmt.Errorf("夸克 HTTP %d (%s %s): 响应=%s | Cookie 解析后长度=%d",
			resp.StatusCode, method, path, netutil.Head(text, 300), len(cookie))
	}
	return resp, nil
}

// quarkCall 发送请求并把响应解码到 out（out 为 nil 时只检查错误）。
// 响应中 status >= 400 或 code != 0 视为业务错误；code 31001 附带 Cookie 失效的排查提示。
func (b *netdiskBackend) quarkCall(ctx context.Context, method, path string, body, out any) error {
	resp, err := b.quarkDo(ctx, method, path, body)
	if err != nil {
		return err
	}
	text := netutil.ReadBodyString(resp)
	var e struct {
		Status  *int    `json:"status"`
		Code    *int    `json:"code"`
		Message *string `json:"message"`
	}
	if json.Unmarshal([]byte(text), &e) == nil {
		status, code := deref(e.Status), deref(e.Code)
		if status >= 400 || code != 0 {
			msg := text
			if e.Message != nil {
				msg = *e.Message
			}
			hint := ""
			if code == 31001 {
				hint = fmt.Sprintf("：Cookie 已过期或无效（已解析 %d 个字符）。请重新从 pan.quark.cn 复制 Cookie", len(b.cookie()))
			}
			return fmt.Errorf("夸克 API 错误 (status=%d, code=%d)%s：%s", status, code, hint, msg)
		}
	}
	if out == nil {
		return nil
	}
	if err := json.Unmarshal([]byte(text), out); err != nil {
		return fmt.Errorf("解析夸克响应失败 (%v): %s", err, text)
	}
	return nil
}

func deref(p *int) int {
	if p == nil {
		return 0
	}
	return *p
}

// quarkList 分页列出目录 fid 下的全部子项。
func (b *netdiskBackend) quarkList(ctx context.Context, pdirFid string) ([]quarkFile, error) {
	const size = 100
	var all []quarkFile
	for page := 1; ; page++ {
		var r struct {
			Data struct {
				List     []quarkFile `json:"list"`
				Metadata *struct {
					Total int `json:"total"`
				} `json:"_metadata"`
			} `json:"data"`
		}
		path := fmt.Sprintf("/file/sort?pdir_fid=%s&_page=%d&_size=%d&_fetch_total=1&_fetch_sub_dirs=1&fetch_all_file=1", pdirFid, page, size)
		if err := b.quarkCall(ctx, http.MethodGet, path, nil, &r); err != nil {
			return nil, err
		}
		all = append(all, r.Data.List...)
		total := 0
		if r.Data.Metadata != nil {
			total = r.Data.Metadata.Total
		}
		if page*size >= total || len(r.Data.List) == 0 {
			return all, nil
		}
	}
}

// quarkResolveDir 把目录路径解析为 fid：根目录为 "0"，路径段可以是名称或 32 位 fid。
func (b *netdiskBackend) quarkResolveDir(ctx context.Context, dir string) (string, error) {
	clean := strings.Trim(dir, "/")
	if clean == "" {
		return "0", nil
	}
	current := "0"
	for _, seg := range strings.Split(clean, "/") {
		if seg == "" {
			continue
		}
		if isFid(seg) {
			current = seg
			continue
		}
		files, err := b.quarkList(ctx, current)
		if err != nil {
			return "", err
		}
		found := ""
		for _, f := range files {
			if f.FileName == seg && !f.File {
				found = f.Fid
				break
			}
		}
		if found == "" {
			return "", fmt.Errorf("夸克网盘路径不存在: '%s' 在目录 %s 中未找到", seg, current)
		}
		current = found
	}
	return current, nil
}

// quarkFind 在目录 fid 下按名称查找条目；wantFile 为 nil 时文件与文件夹都匹配。
func (b *netdiskBackend) quarkFind(ctx context.Context, parentFid, name string, wantFile *bool) (*quarkFile, error) {
	files, err := b.quarkList(ctx, parentFid)
	if err != nil {
		return nil, err
	}
	for i := range files {
		if files[i].FileName == name && (wantFile == nil || files[i].File == *wantFile) {
			return &files[i], nil
		}
	}
	return nil, nil
}

// quarkMkdir 在 parentFid 下创建目录并返回其 fid；同名目录已存在时返回现有目录的 fid。
func (b *netdiskBackend) quarkMkdir(ctx context.Context, parentFid, name string) (string, error) {
	resp, err := b.quarkDo(ctx, http.MethodPost, "/file", map[string]any{
		"dir_init_lock": false, "dir_path": "", "file_name": name, "pdir_fid": parentFid,
	})
	if err != nil {
		return "", err
	}
	text := netutil.ReadBodyString(resp)
	var r struct {
		Code *int `json:"code"`
		Data struct {
			Fid string `json:"fid"`
		} `json:"data"`
		Message string `json:"message"`
	}
	if json.Unmarshal([]byte(text), &r) == nil {
		if r.Code != nil && *r.Code == 0 && r.Data.Fid != "" {
			return r.Data.Fid, nil
		}
		if (r.Code != nil && *r.Code == 23008) || strings.Contains(r.Message, "同名冲突") || strings.Contains(r.Message, "already exists") {
			slog.Info("[Quark] 目录已存在，从列表查找 fid", "name", name)
		}
	}
	isFile := false
	f, err := b.quarkFind(ctx, parentFid, name, &isFile)
	if err != nil {
		return "", err
	}
	if f == nil {
		return "", fmt.Errorf("夸克网盘创建目录后未找到: %s（响应: %s）", name, netutil.Head(text, 200))
	}
	return f.Fid, nil
}

// quarkEnsureDir 逐级创建目录并返回最末一级的 fid；路径段为 fid 时直接作为父目录。
func (b *netdiskBackend) quarkEnsureDir(ctx context.Context, dir string) (string, error) {
	parent := "0"
	for _, seg := range strings.Split(strings.Trim(dir, "/"), "/") {
		if seg == "" {
			continue
		}
		if isFid(seg) {
			parent = seg
			continue
		}
		fid, err := b.quarkMkdir(ctx, parent, seg)
		if err != nil {
			return "", err
		}
		parent = fid
	}
	return parent, nil
}

// splitParent 把 "a/b/c" 拆成 ("a/b", "c")；没有 "/" 时父路径为空。
func splitParent(p string) (string, string) {
	if i := strings.LastIndex(p, "/"); i >= 0 {
		return p[:i], p[i+1:]
	}
	return "", p
}

func (b *netdiskBackend) quarkListDir(ctx context.Context, dir string) ([]RemoteFileEntry, error) {
	fid, err := b.quarkResolveDir(ctx, dir)
	if err != nil {
		return nil, err
	}
	files, err := b.quarkList(ctx, fid)
	if err != nil {
		return nil, err
	}
	parent := strings.TrimRight(dir, "/")
	out := make([]RemoteFileEntry, 0, len(files))
	for _, f := range files {
		// 返回“父路径/文件名”，DownloadFile 可直接使用
		full := f.FileName
		if parent != "" && parent != "." {
			full = parent + "/" + f.FileName
		}
		mod := time.UnixMilli(f.UpdatedAt).UTC().Format(time.RFC3339)
		out = append(out, RemoteFileEntry{Name: f.FileName, Path: full, IsDir: !f.File, Size: f.Size, Modified: &mod})
	}
	return out, nil
}

// quarkDownload 下载文件。remotePath 可以是“父路径/文件名”，也可以是裸文件 fid。
func (b *netdiskBackend) quarkDownload(ctx context.Context, remotePath, localPath string) error {
	remote := strings.Trim(remotePath, "/")
	fileFid := remote
	if !isFid(remote) {
		parentPath, name := splitParent(remote)
		parentFid, err := b.quarkResolveDir(ctx, parentPath)
		if err != nil {
			return err
		}
		isFile := true
		f, err := b.quarkFind(ctx, parentFid, name, &isFile)
		if err != nil {
			return err
		}
		if f == nil {
			return fmt.Errorf("夸克网盘文件不存在: %s", remotePath)
		}
		fileFid = f.Fid
	}

	var r struct {
		Data []struct {
			DownloadURL string `json:"download_url"`
		} `json:"data"`
	}
	if err := b.quarkCall(ctx, http.MethodPost, "/file/download", map[string]any{"fids": []string{fileFid}}, &r); err != nil {
		return err
	}
	if len(r.Data) == 0 {
		return errors.New("夸克网盘下载链接为空")
	}

	headers := http.Header{}
	headers.Set("Cookie", strings.NewReplacer("\r", " ", "\n", " ").Replace(b.cookie()))
	headers.Set("User-Agent", quarkUA)
	headers.Set("Referer", quarkReferer)
	if err := netutil.DownloadToFile(ctx, r.Data[0].DownloadURL, headers, localPath); err != nil {
		msg := err.Error()
		if strings.Contains(msg, "auth miss") || strings.Contains(msg, "RequestDeniedByCallback") || strings.Contains(msg, "HTTP 403") {
			return fmt.Errorf("夸克网盘下载失败: 直链节点拒绝了未携带登录态的请求。请重试；若仍失败，请重新从 pan.quark.cn 复制完整 Cookie。原始错误: %s", msg)
		}
		return fmt.Errorf("夸克网盘下载失败: %s", msg)
	}
	return nil
}

// quarkDelete 删除文件或目录：先解析父目录，再按名称（文件或文件夹）定位 fid。
func (b *netdiskBackend) quarkDelete(ctx context.Context, remotePath string) error {
	remote := strings.Trim(remotePath, "/")
	fid := remote
	if !isFid(remote) {
		parentPath, name := splitParent(remote)
		parentFid, err := b.quarkResolveDir(ctx, parentPath)
		if err != nil {
			return err
		}
		f, err := b.quarkFind(ctx, parentFid, name, nil)
		if err != nil {
			return err
		}
		if f == nil {
			return fmt.Errorf("夸克网盘路径不存在: %s", remotePath)
		}
		fid = f.Fid
	}
	return b.quarkCall(ctx, http.MethodPost, "/file/delete", map[string]any{"fids": []string{fid}, "to_pdir_fid": ""}, nil)
}

// quarkUpload 先确保父目录存在，再走 OSS 直传流程上传。
func (b *netdiskBackend) quarkUpload(ctx context.Context, localPath, remotePath string) error {
	parentPath, name := splitParent(strings.Trim(remotePath, "/"))
	parentFid, err := b.quarkEnsureDir(ctx, parentPath)
	if err != nil {
		return err
	}
	return b.quarkUploadTo(ctx, localPath, parentFid, name)
}

func ossDate() string { return time.Now().UTC().Format("Mon, 02 Jan 2006 15:04:05 GMT") }

// quarkUploadTo 按 OpenList quark_uc 的 Put 流程上传：
// pre（取 OSS 凭证）→ update/hash（MD5+SHA1 秒传检测）→ 逐分片 auth + PUT → commit（合并分片）→ finish。
func (b *netdiskBackend) quarkUploadTo(ctx context.Context, localPath, parentFid, fileName string) error {
	data, err := os.ReadFile(localPath)
	if err != nil {
		return err
	}
	if len(data) == 0 {
		return errors.New("夸克网盘不允许上传空文件")
	}
	nowMs := time.Now().UnixMilli()

	// 1. pre：预上传，取 OSS 上传参数
	var pre struct {
		Data struct {
			TaskID    string          `json:"task_id"`
			Bucket    string          `json:"bucket"`
			ObjKey    string          `json:"obj_key"`
			UploadID  string          `json:"upload_id"`
			AuthInfo  string          `json:"auth_info"`
			Callback  json.RawMessage `json:"callback"`
			UploadURL string          `json:"upload_url"`
		} `json:"data"`
		Metadata struct {
			PartSize int64 `json:"part_size"`
		} `json:"metadata"`
	}
	if err := b.quarkCall(ctx, http.MethodPost, "/file/upload/pre", map[string]any{
		"ccp_hash_update": true, "dir_name": "", "file_name": fileName, "format_type": quarkMime,
		"l_created_at": nowMs, "l_updated_at": nowMs, "pdir_fid": parentFid, "size": len(data),
	}, &pre); err != nil {
		return err
	}
	d := pre.Data
	partSize := pre.Metadata.PartSize
	if partSize <= 0 {
		return fmt.Errorf("夸克预上传返回了无效的分片大小: %d", partSize)
	}
	uploadDomain := strings.TrimRight(d.UploadURL, "/")
	uploadDomain = strings.TrimPrefix(strings.TrimPrefix(uploadDomain, "https://"), "http://")

	// 2. 秒传检测
	md5Sum := md5.Sum(data)
	sha1Sum := sha1.Sum(data)
	var hash struct {
		Data struct {
			Finish bool `json:"finish"`
		} `json:"data"`
	}
	if err := b.quarkCall(ctx, http.MethodPost, "/file/update/hash", map[string]any{
		"md5": hex.EncodeToString(md5Sum[:]), "sha1": hex.EncodeToString(sha1Sum[:]), "task_id": d.TaskID,
	}, &hash); err != nil {
		return err
	}
	if hash.Data.Finish {
		slog.Info("[Quark] 秒传命中，跳过上传", "file", fileName)
		return nil
	}

	// 3. 逐分片签名并 PUT 到 OSS
	numParts := int((int64(len(data)) + partSize - 1) / partSize)
	slog.Info("[Quark] 上传开始", "file", fileName, "size", len(data), "parts", numParts, "partSize", partSize)
	etags := make([]string, 0, numParts)
	for i := 0; i < numParts; i++ {
		start := int64(i) * partSize
		end := min(start+partSize, int64(len(data)))
		etag, err := b.quarkPutPart(ctx, d.AuthInfo, d.TaskID, d.Bucket, uploadDomain, d.ObjKey, d.UploadID, i+1, numParts, data[start:end])
		if err != nil {
			return err
		}
		etags = append(etags, etag)
	}

	// 4. commit：CompleteMultipartUpload
	var xml strings.Builder
	xml.WriteString("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CompleteMultipartUpload>\n")
	for i, etag := range etags {
		fmt.Fprintf(&xml, "<Part>\n<PartNumber>%d</PartNumber>\n<ETag>%s</ETag>\n</Part>\n", i+1, etag)
	}
	xml.WriteString("</CompleteMultipartUpload>")
	xmlBody := xml.String()
	xmlMD5 := md5.Sum([]byte(xmlBody))
	contentMD5 := base64.StdEncoding.EncodeToString(xmlMD5[:])

	var callback bytes.Buffer
	if err := json.Compact(&callback, d.Callback); err != nil {
		return fmt.Errorf("夸克预上传返回的 callback 无法解析: %w", err)
	}
	callbackB64 := base64.StdEncoding.EncodeToString(callback.Bytes())
	date := ossDate()
	commitMeta := fmt.Sprintf("POST\n%s\napplication/xml\n%s\nx-oss-callback:%s\nx-oss-date:%s\nx-oss-user-agent:%s\n/%s/%s?uploadId=%s",
		contentMD5, date, callbackB64, date, quarkOSSUA, d.Bucket, d.ObjKey, d.UploadID)
	authKey, err := b.quarkUploadAuth(ctx, d.AuthInfo, commitMeta, d.TaskID)
	if err != nil {
		return err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost,
		fmt.Sprintf("https://%s.%s/%s?uploadId=%s", d.Bucket, uploadDomain, d.ObjKey, d.UploadID), strings.NewReader(xmlBody))
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", authKey)
	req.Header.Set("Content-Type", "application/xml")
	req.Header.Set("Content-MD5", contentMD5)
	req.Header.Set("Referer", quarkReferer)
	req.Header.Set("x-oss-callback", callbackB64)
	req.Header.Set("x-oss-date", date)
	req.Header.Set("x-oss-user-agent", quarkOSSUA)
	resp, err := quarkCommitHTTP.Do(req)
	if err != nil {
		return err
	}
	commitText := netutil.ReadBodyString(resp)
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("夸克 OSS 合并分片失败 (HTTP %d): %s", resp.StatusCode, commitText)
	}

	// 5. finish：通知夸克上传完成
	if err := b.quarkCall(ctx, http.MethodPost, "/file/upload/finish", map[string]any{"obj_key": d.ObjKey, "task_id": d.TaskID}, nil); err != nil {
		return err
	}
	slog.Info("[Quark] 上传完成", "file", fileName)
	return nil
}

func (b *netdiskBackend) quarkUploadAuth(ctx context.Context, authInfo, authMeta, taskID string) (string, error) {
	var r struct {
		Data struct {
			AuthKey string `json:"auth_key"`
		} `json:"data"`
	}
	err := b.quarkCall(ctx, http.MethodPost, "/file/upload/auth", map[string]any{
		"auth_info": authInfo, "auth_meta": authMeta, "task_id": taskID,
	}, &r)
	return r.Data.AuthKey, err
}

// quarkPutPart 签名并上传一个分片，返回 OSS 的 ETag。
// 连接失败或 5xx 时最多重试 3 次（间隔 1s、2s），4xx 不重试。
func (b *netdiskBackend) quarkPutPart(ctx context.Context, authInfo, taskID, bucket, domain, objKey, uploadID string, partNumber, numParts int, slice []byte) (string, error) {
	date := ossDate()
	meta := fmt.Sprintf("PUT\n\n%s\n%s\nx-oss-date:%s\nx-oss-user-agent:%s\n/%s/%s?partNumber=%d&uploadId=%s",
		quarkMime, date, date, quarkOSSUA, bucket, objKey, partNumber, uploadID)
	authKey, err := b.quarkUploadAuth(ctx, authInfo, meta, taskID)
	if err != nil {
		return "", err
	}
	ossURL := fmt.Sprintf("https://%s.%s/%s?partNumber=%d&uploadId=%s", bucket, domain, objKey, partNumber, uploadID)

	var lastErr string
	for attempt := 0; attempt < 3; attempt++ {
		if attempt > 0 {
			select {
			case <-time.After(time.Duration(attempt) * time.Second):
			case <-ctx.Done():
				return "", ctx.Err()
			}
		}
		req, err := http.NewRequestWithContext(ctx, http.MethodPut, ossURL, bytes.NewReader(slice))
		if err != nil {
			return "", err
		}
		req.Header.Set("Authorization", authKey)
		req.Header.Set("Content-Type", quarkMime)
		req.Header.Set("Referer", quarkReferer)
		req.Header.Set("x-oss-date", date)
		req.Header.Set("x-oss-user-agent", quarkOSSUA)
		resp, err := quarkOSSClient.Do(req)
		if err != nil {
			lastErr = fmt.Sprintf("连接失败 (url=%s): %v", ossURL, err)
			continue
		}
		text := netutil.ReadBodyString(resp)
		if resp.StatusCode == http.StatusOK {
			return resp.Header.Get("ETag"), nil
		}
		lastErr = fmt.Sprintf("HTTP %d: %s", resp.StatusCode, netutil.Head(text, 200))
		if resp.StatusCode < 500 {
			break
		}
	}
	return "", fmt.Errorf("夸克 OSS 分片上传失败 (分片 %d/%d): %s", partNumber, numParts, lastErr)
}
