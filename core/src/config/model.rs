// config/model.rs - 配置数据结构
use serde::{Deserialize, Serialize};

/// 应用根配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// 激活的多后端存储配置项（当用户配置了新存储时生效）
    pub storage: Option<StorageConfig>,
    /// 游戏存档配置列表
    pub games: Vec<GameConfig>,
    /// 应用全局通用设置（如主题等）
    pub settings: Settings,
    
    /// 旧版独占的 Alist 配置（专用于向前兼容反序列化，反序列化完后在 load_config 中自动迁移至 storage，不参与新文件的显式序列化输出）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alist: Option<AlistConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            storage: None,
            games: Vec::new(),
            settings: Settings::default(),
            alist: None,
        }
    }
}

/// 统一存储后端多路选择配置枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StorageConfig {
    /// 物理直连网盘模式（基于 api.oplist.org 中转网关免部署一键授权，支持百度网盘、OneDrive 等，免去自建 Alist 的多余复杂性）
    Netdisk(NetdiskConfig),
    /// 用户自行私有化部署的 Alist / OpenList 服务器
    Alist(AlistConfig),
    /// 通用标准 WebDAV 备份挂载协议
    Webdav(WebdavConfig),
    /// AWS S3 兼容标准对象存储备份挂载协议 (如 MinIO, Cloudflare R2, 腾讯云 COS, 阿里云 OSS 等)
    S3(S3Config),
}

/// 免自建 Alist 的直连网盘配置数据结构（基于 api.oplist.org OAuth 中转直连）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetdiskConfig {
    /// 选取的云网盘物理驱动类型（如 "baiduyun_go", "onedrive_go" 等）
    pub driver: String,
    /// 自动授权或手动填入的 Access Token (访问令牌)
    pub token: String,
    /// 自动授权或手动填入的 Refresh Token (刷新令牌，主要用于突破 Access Token 时效限制，非必填以向下兼容旧版)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// 云端物理备份的隔离根路径，例如 "/Download/GameSave"
    pub backup_root: Option<String>,
}

/// Alist 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlistConfig {
    /// 服务端基础 HTTP(S) 地址（例如 "http://127.0.0.1:5244" 或 "https://api.oplist.org"）
    pub base_url: String,
    /// 授权用户名
    pub username: String,
    /// 登录成功或网页 OAuth 扫码拿到的 Bearer Token
    pub token: Option<String>,
    /// 提供商类型标识：支持 "alist" 自建版或 "openlist" 聚合托管版
    pub provider: String, 
    /// 全局云端物理备份根路径，例如 "/Download/GameSave"
    pub backup_root: Option<String>,
    /// 自建端密码，用于全自动免 Token 登录（可选，可替代 Token）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

/// WebDAV 配置数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebdavConfig {
    /// 服务端标准 WebDAV HTTP(S) 服务物理接入点（例如 "https://dav.jianguoyun.com/dav/"）
    pub endpoint: String,
    /// 用于鉴权的 WebDAV 账户名
    pub username: String,
    /// 用于鉴权的 WebDAV 独立第三方应用访问口令/密码（建议对明文进行加密持久化）
    pub password: String,
    /// 云端物理备份存储根文件夹（默认为空，代表挂载根目录，支持多层嵌套如 "/backups/games"）
    pub backup_root: Option<String>,
}

/// S3 对象存储配置数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    /// S3 对象存储端点接入物理地址（例如 "https://s3.us-east-1.amazonaws.com" 或 Cloudflare R2 的 "https://<account>.r2.cloudflarestorage.com"）
    pub endpoint: String,
    /// 目标物理桶/存储空间名称 (Bucket Name)
    pub bucket: String,
    /// 鉴权用的访问密钥 ID (Access Key ID)
    pub access_key_id: String,
    /// 鉴权用的私有访问密钥 (Secret Access Key)
    pub secret_access_key: String,
    /// 区域参数，默认为 "us-east-1"
    pub region: Option<String>,
    /// 桶内的备份物理存储路径前缀前置根目录（例如 "backups" 或 "games/saves"）
    pub backup_root: Option<String>,
}

/// 辅助函数：对网盘远程路径进行非法字符“清洗”（Sanitize）
///
/// # 核心设计与规避
/// 百度网盘（以及 OneDrive 等主流云盘）的云端文件系统对文件名及文件夹名有着严苛的安全限制，
/// 严禁使用以下非法特殊字符：`\ : * ? " < > |`。
/// 特别地，如《Kingdom Come: Deliverance II》等带副标题的游戏，其名称自带英文冒号 `:`。若直接发送给
/// 百度网盘 API 创建文件夹，会立刻触发 `errno: -7` (非法字符限制) 异常。
/// 本函数将路径中除层级分隔符 `/` 以外的所有非法特殊字符统一洗消替换为下划线 `_`，确保网盘云端顺利建档。
fn sanitize_remote_path(path: &str) -> String {
    path.chars().map(|c| {
        match c {
            ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\\' => '_',
            _ => c
        }
    }).collect()
}

impl AppConfig {
    /// 动态获取并洗消游戏的云端物理备份路径。
    ///
    /// # 参数说明
    /// * `game` - 目标游戏配置
    ///
    /// # 核心设计
    /// 1. 动态迁移与多后端前缀提取：首先从激活的 `storage` 多后端配置中尝试获取 `backup_root` 参数，
    ///    如果 `storage` 未配置，则退避（Fallback）到旧版的 `alist.backup_root`。
    /// 2. 路径拼装逻辑：如果配置了自定义的备份根目录 `backup_root`，则会将默认的游戏备份子目录
    ///    拼接在其下。例如，如果 `backup_root` 是 `/CloudSave`，原始游戏路径是 `/GameSaves/KingdomCome`，
    ///    则会被拼接净化为 `/CloudSave/KingdomCome`。
    /// 3. 非法路径净化：计算完成后，调用 `sanitize_remote_path` 对最终的远程物理路径进行全局高危非法字符
    ///    （如冒号 `:`，问号 `?` 等）洗消替换为下划线 `_`，彻底杜绝百度网盘等物理文件系统因特殊文件名抛出 `-errno: -7` 崩溃。
    pub fn get_game_remote_path(&self, game: &GameConfig) -> String {
        // 首先尝试从 storage 多后端配置中提取对应的 backup_root 选项
        let backup_root = match &self.storage {
            Some(StorageConfig::Netdisk(ref netdisk)) => netdisk.backup_root.clone(),
            Some(StorageConfig::Alist(ref alist)) => alist.backup_root.clone(),
            Some(StorageConfig::Webdav(ref webdav)) => webdav.backup_root.clone(),
            Some(StorageConfig::S3(ref s3)) => s3.backup_root.clone(),
            None => self.alist.as_ref().and_then(|a| a.backup_root.clone()),
        };

        let raw_path = if let Some(ref root) = backup_root {
            if !root.is_empty() {
                // 剥除默认的前缀以避免重复拼接路径层级
                let game_sub = if game.remote_path.starts_with("/GameSaves/") {
                    game.remote_path.trim_start_matches("/GameSaves/")
                } else {
                    game.remote_path.trim_start_matches('/')
                };
                format!("{}/{}", root.trim_end_matches('/'), game_sub)
            } else {
                game.remote_path.clone()
            }
        } else {
            game.remote_path.clone()
        };

        // 统一对计算完成的路径进行非法字符净化
        sanitize_remote_path(&raw_path)
    }
}


/// 游戏配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
    pub id: String,
    pub name: String,
    pub save_paths: Vec<String>,
    pub remote_path: String,
    pub last_backup: Option<String>,
    pub logo_path: Option<String>,
    pub steam_appid: Option<u64>,
}

/// 全局设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String, // "system" | "light" | "dark"
    pub steamgriddb_api_key: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            steamgriddb_api_key: None,
        }
    }
}
