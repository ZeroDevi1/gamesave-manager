package storage

import (
	"context"
	"errors"
)

// s3Backend 是 S3 兼容存储的占位实现：除 Mkdir 外的操作都返回“尚未支持”。
// 对象存储没有真实目录，因此 Mkdir 直接成功。
type s3Backend struct{}

var errS3Unsupported = errors.New("S3 对象存储支持目前正处于实验性开发阶段，敬请期待！推荐优先选用稳定性极佳的 WebDAV 或 Alist/OpenList 后端。")

func (s3Backend) Mkdir(context.Context, string) error                { return nil }
func (s3Backend) UploadFile(context.Context, string, string) error   { return errS3Unsupported }
func (s3Backend) DownloadFile(context.Context, string, string) error { return errS3Unsupported }
func (s3Backend) Delete(context.Context, string) error               { return errS3Unsupported }
func (s3Backend) ListDir(context.Context, string) ([]RemoteFileEntry, error) {
	return nil, errS3Unsupported
}
