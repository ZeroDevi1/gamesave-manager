// Package config 定义应用配置模型，并负责 config.json 的读写与旧格式迁移。
package config

import (
	"encoding/json"
	"strings"
)

// StorageType 是云端存储后端类型。
type StorageType string

const (
	// StorageNetdisk 直连网盘：百度/夸克走官方接口，其它驱动经 api.oplist.org 的 WebDAV 网关。
	StorageNetdisk StorageType = "netdisk"
	// StorageAlist 用户自建的 Alist / OpenList 服务器。
	StorageAlist StorageType = "alist"
	// StorageWebdav 标准 WebDAV。
	StorageWebdav StorageType = "webdav"
	// StorageS3 S3 兼容对象存储（尚未实现读写）。
	StorageS3 StorageType = "s3"
)

// StorageConfig 是当前启用的云端存储配置。
//
// JSON 形态与 Tauri 版以 type 为标签的枚举一致：各类型只使用自己的字段，
// 其余字段为空并在序列化时省略。空字符串等同于 Rust 版中的 None。
type StorageConfig struct {
	Type StorageType `json:"type"`

	// Driver 是网盘驱动名（netdisk），如 "baiduyun_go"、"quark"、"onedrive_go"。
	Driver string `json:"driver,omitempty"`
	// Token 是访问令牌：netdisk 的 Access Token 或夸克 Cookie；alist 登录得到的 Token。
	Token string `json:"token,omitempty"`
	// RefreshToken 是 netdisk 的刷新令牌，经 api.oplist.org 换取新的 Access Token。
	RefreshToken string `json:"refresh_token,omitempty"`

	// BaseURL 是 alist 服务端地址，如 "http://127.0.0.1:5244"。
	BaseURL string `json:"base_url,omitempty"`
	// Username 用于 alist 登录与 webdav 鉴权。
	Username string `json:"username,omitempty"`
	// Password 是 alist 自建端密码（可选，用于 Token 失效时自动重新登录）或 webdav 密码。
	Password string `json:"password,omitempty"`
	// Provider 是 alist 提供商标识："alist" 或 "openlist"。
	Provider string `json:"provider,omitempty"`

	// Endpoint 是 webdav 或 s3 的服务地址。
	Endpoint        string `json:"endpoint,omitempty"`
	Bucket          string `json:"bucket,omitempty"`
	AccessKeyID     string `json:"access_key_id,omitempty"`
	SecretAccessKey string `json:"secret_access_key,omitempty"`
	Region          string `json:"region,omitempty"`

	// BackupRoot 是云端备份根目录，如 "/Download/GameSave"；为空时使用游戏自身的 remote_path。
	BackupRoot string `json:"backup_root,omitempty"`
}

// AlistConfig 是旧版独立的 Alist 配置，只用于读取老 config.json 并迁移到 StorageConfig。
type AlistConfig struct {
	BaseURL    string `json:"base_url"`
	Username   string `json:"username"`
	Token      string `json:"token,omitempty"`
	Provider   string `json:"provider"`
	BackupRoot string `json:"backup_root,omitempty"`
	Password   string `json:"password,omitempty"`
}

// toStorage 把旧版 Alist 配置转换为统一存储配置。
func (a AlistConfig) toStorage() StorageConfig {
	return StorageConfig{
		Type:       StorageAlist,
		BaseURL:    a.BaseURL,
		Username:   a.Username,
		Token:      a.Token,
		Provider:   a.Provider,
		BackupRoot: a.BackupRoot,
		Password:   a.Password,
	}
}

// GameConfig 是一个受管理游戏的配置。
type GameConfig struct {
	ID string `json:"id"`
	// Name 是显示名，也用于生成默认 RemotePath 与备份文件名。
	Name string `json:"name"`
	// SavePaths 是存档路径，可含环境变量占位符与 glob 通配符。
	SavePaths []string `json:"save_paths"`
	// RemotePath 是默认云端目录 "/GameSaves/<name>"；配置了 BackupRoot 时只取其末段。
	RemotePath string  `json:"remote_path"`
	LastBackup *string `json:"last_backup,omitempty"`
	// LogoPath 是封面：http(s) 远程地址，或 {data}/cache/logos 下的本地文件路径。
	LogoPath   *string `json:"logo_path,omitempty"`
	SteamAppID *uint64 `json:"steam_appid,omitempty"`
	// ExeName 是用于进程监控的主程序文件名（如 "eldenring.exe"），不区分大小写。
	ExeName *string `json:"exe_name,omitempty"`
	// AutoBackupEnabled 为 nil 时跟随 Settings.AutoBackup。
	AutoBackupEnabled *bool `json:"auto_backup_enabled,omitempty"`
	// ConfirmBeforeSync 为 nil 时跟随 Settings.RequireConfirmation。
	ConfirmBeforeSync *bool `json:"confirm_before_sync,omitempty"`
}

// Settings 是全局设置。
type Settings struct {
	// Theme 取 "system"、"light" 或 "dark"。
	Theme             string  `json:"theme"`
	SteamGridDBAPIKey *string `json:"steamgriddb_api_key,omitempty"`
	// AutoBackup 是自动备份总开关：关闭后游戏进程退出时不再自动备份。
	AutoBackup bool `json:"auto_backup"`
	// RequireConfirmation 为 true 时自动同步/备份前需要用户确认。
	RequireConfirmation bool `json:"require_confirmation"`
	// BackupRetentionLimit 是云端全量备份与增量目录各自的保留数量；nil 或 0 表示不限制。
	BackupRetentionLimit *uint32 `json:"backup_retention_limit,omitempty"`
}

// DefaultSettings 返回缺省设置。
func DefaultSettings() Settings {
	return Settings{Theme: "system", AutoBackup: true, RequireConfirmation: true}
}

// UnmarshalJSON 先填入缺省值再解码，使缺失字段保持默认（对应 Rust 版的 #[serde(default)]）。
// 前端保存配置时只回传它知道的设置字段，因此这一步决定了 auto_backup 等字段不会被清零。
func (s *Settings) UnmarshalJSON(data []byte) error {
	type plain Settings
	v := plain(DefaultSettings())
	if err := json.Unmarshal(data, &v); err != nil {
		return err
	}
	*s = Settings(v)
	return nil
}

// AppConfig 是 config.json 的根结构。
type AppConfig struct {
	// Storage 是当前启用的云端存储；nil 表示未配置。
	Storage  *StorageConfig `json:"storage,omitempty"`
	Games    []GameConfig   `json:"games"`
	Settings Settings       `json:"settings"`
	// Alist 是旧版配置字段，Load 时迁移到 Storage 后清空，不再写回。
	Alist *AlistConfig `json:"alist,omitempty"`
}

// UnmarshalJSON 在 settings 缺失时同样使用缺省设置。
func (c *AppConfig) UnmarshalJSON(data []byte) error {
	type plain AppConfig
	v := plain{Settings: DefaultSettings()}
	if err := json.Unmarshal(data, &v); err != nil {
		return err
	}
	*c = AppConfig(v)
	return nil
}

// Default 返回空配置。
func Default() AppConfig {
	return AppConfig{Games: []GameConfig{}, Settings: DefaultSettings()}
}

// FindGame 返回 id 对应的游戏；不存在时返回 nil。返回的指针指向 c.Games 中的元素。
func (c *AppConfig) FindGame(id string) *GameConfig {
	for i := range c.Games {
		if c.Games[i].ID == id {
			return &c.Games[i]
		}
	}
	return nil
}

// sanitizeRemotePath 把除 "/" 外网盘不接受的字符（\ : * ? " < > |）替换为下划线。
// 百度网盘遇到这些字符会返回 errno -7，例如带冒号的《Kingdom Come: Deliverance II》。
func sanitizeRemotePath(p string) string {
	return strings.Map(func(r rune) rune {
		switch r {
		case ':', '*', '?', '"', '<', '>', '|', '\\':
			return '_'
		}
		return r
	}, p)
}

// GameRemotePath 返回游戏在云端的备份目录（已清洗非法字符）。
//
// 配置了 BackupRoot 时为 "<BackupRoot>/<RemotePath 去掉 /GameSaves/ 前缀>"，
// 否则直接使用 RemotePath。未配置 Storage 时回退读取旧版 Alist.BackupRoot。
func (c *AppConfig) GameRemotePath(g *GameConfig) string {
	var root string
	switch {
	case c.Storage != nil:
		root = c.Storage.BackupRoot
	case c.Alist != nil:
		root = c.Alist.BackupRoot
	}

	raw := g.RemotePath
	if root != "" {
		var sub string
		if strings.HasPrefix(g.RemotePath, "/GameSaves/") {
			sub = strings.TrimPrefix(g.RemotePath, "/GameSaves/")
		} else {
			sub = strings.TrimLeft(g.RemotePath, "/")
		}
		raw = strings.TrimRight(root, "/") + "/" + sub
	}
	return sanitizeRemotePath(raw)
}
