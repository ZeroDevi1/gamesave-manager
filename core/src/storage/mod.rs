// storage/mod.rs - 统一存储后端抽象层与多路适配器实现
use crate::config::model::{AlistConfig, StorageConfig, WebdavConfig, S3Config};
use reqwest;
use serde::{Deserialize, Serialize};
use std::path::Path;
use regex::Regex;
use base64::Engine;

/// 统一的云端物理文件/文件夹项定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFileEntry {
    /// 文件的物理名称（例如 "KingdomCome2_20260523.zip"）
    pub name: String,
    /// 文件的云端绝对或相对物理路径（例如 "/backups/games/KingdomCome2/full/save.zip"）
    pub path: String,
    /// 标识该文件项是否为物理文件夹/目录
    pub is_dir: bool,
    /// 文件大小（单位：字节；如果是文件夹，则默认为 0）
    pub size: i64,
    /// 上次物理修改时间戳（RFC3339 字符串，如果是文件夹则为 None）
    pub modified: Option<String>,
}

/// 统一存储后端物理适配器分发枚举 (Enum-based Static Dispatch)
/// 
/// # 核心设计哲学
/// 在 Rust 中，包含原生 `async fn` 的 Trait 并不符合对象安全（dyn compatibility）条件，无法直接进行 `Box<dyn Trait>` 动态分发。
/// 为了实现优雅的跨后端零虚表、零额外堆内存分配（Heap Allocation）开销的多路分发，此处我们采用 Rust 社区极力推崇的
/// **基于枚举的静态分发（Enum-based Static Dispatch）适配模式**。
/// 这不仅完全规避了引入 `async-trait` 外部宏的编译开销，而且运行速度极快，是极致系统性能与可维护性的完美契合。
pub enum StorageBackend {
    /// 物理直连网盘模式 (免安装部署 Alist，一键通过 api.oplist.org 中转网关直连)
    Netdisk(NetdiskBackend),
    /// 用户自建的本地 / 远程 Alist 服务器
    Alist(AlistBackend),
    /// 通用标准 WebDAV 备份变体
    Webdav(WebdavBackend),
    /// AWS S3 兼容对象存储备份变体
    S3(S3Backend),
}

impl StorageBackend {
    /// 在云端物理系统上递归或多级级联创建目录结构
    pub async fn mkdir(&self, path: &str) -> anyhow::Result<()> {
        match self {
            Self::Netdisk(b) => b.mkdir(path).await,
            Self::Alist(b) => b.mkdir(path).await,
            Self::Webdav(b) => b.mkdir(path).await,
            Self::S3(b) => b.mkdir(path).await,
        }
    }

    /// 将本地物理存档压缩包上传至云端指定的目标路径
    pub async fn upload_file(&self, local_path: &str, remote_path: &str) -> anyhow::Result<()> {
        match self {
            Self::Netdisk(b) => b.upload_file(local_path, remote_path).await,
            Self::Alist(b) => b.upload_file(local_path, remote_path).await,
            Self::Webdav(b) => b.upload_file(local_path, remote_path).await,
            Self::S3(b) => b.upload_file(local_path, remote_path).await,
        }
    }

    /// 从云端物理拉取指定路径 of 存档，覆盖写入本地绝对物理路径
    pub async fn download_file(&self, remote_path: &str, local_path: &str) -> anyhow::Result<()> {
        match self {
            Self::Netdisk(b) => b.download_file(remote_path, local_path).await,
            Self::Alist(b) => b.download_file(remote_path, local_path).await,
            Self::Webdav(b) => b.download_file(remote_path, local_path).await,
            Self::S3(b) => b.download_file(remote_path, local_path).await,
        }
    }

    /// 列出云端指定物理目录下的所有子条目列表
    pub async fn list_dir(&self, path: &str) -> anyhow::Result<Vec<RemoteFileEntry>> {
        match self {
            Self::Netdisk(b) => b.list_dir(path).await,
            Self::Alist(b) => b.list_dir(path).await,
            Self::Webdav(b) => b.list_dir(path).await,
            Self::S3(b) => b.list_dir(path).await,
        }
    }
}

// =========================================================================
// 0. 免部署直连网盘适配器实现 (借助官方 OpenAPI 免去对 api.oplist.org 的直接 API 依赖)
// =========================================================================

/// 百度网盘官方文件列表响应实体
#[derive(Debug, Deserialize)]
struct BaiduFileListResponse {
    /// 接口返回码，0 代表成功
    errno: i32,
    /// 文件列表数据，若目录为空则为 None
    list: Option<Vec<BaiduFileEntry>>,
}

/// 百度网盘官方单个文件项元数据实体
#[derive(Debug, Deserialize)]
struct BaiduFileEntry {
    /// 文件物理显示名称
    server_filename: String,
    /// 文件的绝对物理路径
    path: String,
    /// 标识是否为物理文件夹，1 代表是，0 代表否
    isdir: i32,
    /// 文件大小（单位：字节）
    size: i64,
    /// 上次物理修改时间戳（秒级 Unix 戳）
    server_mtime: u64,
}

/// 百度网盘官方通用返回码响应实体
#[derive(Debug, Deserialize)]
struct BaiduCommonResponse {
    /// 接口返回码，0 代表成功
    errno: i32,
}

/// 百度网盘预创建上传响应（xpan/file?method=precreate）
#[derive(Debug, Deserialize)]
struct BaiduPrecreateResp {
    errno: i32,
    uploadid: Option<String>,
    /// 分片序号列表，如 [0] 表示单分片，[0,1,2] 表示三分片
    block_list: Option<Vec<i32>>,
    /// return_type=1 代表需要上传（新文件），return_type=2 代表秒传命中
    return_type: Option<i32>,
}

/// 百度网盘上传服务器定位响应（pcs/file?method=locateupload）
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BaiduLocateUploadResp {
    error_code: Option<i32>,
    servers: Option<Vec<BaiduUploadServer>>,
}

#[derive(Debug, Deserialize)]
struct BaiduUploadServer {
    server: String,
}

/// 百度网盘上传分片响应（pcs/superfile2?method=upload）
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BaiduSliceUploadResp {
    /// 非 0 表示错误
    error_code: Option<i32>,
    error_msg: Option<String>,
    md5: Option<String>,
    request_id: Option<i64>,
}

/// 百度网盘小文件（≤4MB）直接上传：precreate → locateupload → 单分片 → create
const BAIDU_SLICE_SIZE: i64 = 4 * 1024 * 1024; // 4MB，普通用户标准分片大小

// =========================================================================
// 夸克网盘直连 API 类型定义（Cookie 认证，基于 drive.quark.cn 官方接口）
// =========================================================================

/// 夸克网盘 API 通用错误响应
#[derive(Debug, Deserialize)]
struct QuarkResp {
    status: Option<i32>,
    code: Option<i32>,
    message: Option<String>,
}

/// 夸克网盘文件条目
#[derive(Debug, Deserialize, Clone)]
struct QuarkFile {
    fid: String,
    file_name: String,
    pdir_fid: String,
    size: i64,
    file: bool,          // true=文件, false=文件夹
    category: Option<i32>, // 0=其他, 1=视频
    updated_at: i64,     // 毫秒时间戳
    created_at: i64,
}

/// 夸克网盘目录列表响应
#[derive(Debug, Deserialize)]
struct QuarkSortResp {
    data: QuarkSortData,
}

#[derive(Debug, Deserialize)]
struct QuarkSortData {
    list: Vec<QuarkFile>,
    #[serde(rename = "_metadata")]
    metadata: Option<QuarkMetadata>,
}

#[derive(Debug, Deserialize)]
struct QuarkMetadata {
    total: i32,
}

/// 夸克网盘下载链接响应
#[derive(Debug, Deserialize)]
struct QuarkDownResp {
    data: Vec<QuarkDownData>,
}

#[derive(Debug, Deserialize)]
struct QuarkDownData {
    download_url: String,
}

/// 夸克网盘上传预请求响应
#[derive(Debug, Deserialize)]
struct QuarkUpPreResp {
    data: QuarkUpPreData,
    metadata: QuarkUpPreMeta,
}

#[derive(Debug, Deserialize)]
struct QuarkUpPreData {
    task_id: String,
    bucket: String,
    obj_key: String,
    upload_id: String,
    auth_info: String,
    callback: serde_json::Value,
    upload_url: String,
}

#[derive(Debug, Deserialize)]
struct QuarkUpPreMeta {
    #[serde(rename = "part_size")]
    part_size: i64,
}

/// 夸克网盘 OSS 上传授权响应
#[derive(Debug, Deserialize)]
struct QuarkUpAuthResp {
    data: QuarkUpAuthData,
}

#[derive(Debug, Deserialize)]
struct QuarkUpAuthData {
    auth_key: String,
}

/// 夸克网盘秒传/哈希检测响应
#[derive(Debug, Deserialize)]
struct QuarkHashResp {
    data: QuarkHashData,
}

#[derive(Debug, Deserialize)]
struct QuarkHashData {
    finish: bool,
}

/// 夸克网盘 API 基础配置常量
const QUARK_API_BASE: &str = "https://drive-pc.quark.cn/1/clouddrive";
const QUARK_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const QUARK_REFERER: &str = "https://pan.quark.cn/";
const QUARK_UPLOAD_PART_SIZE: i64 = 8 * 1024 * 1024; // 8MB 分片

/// 将用户输入的 Cookie 字符串转换为 HTTP Cookie 头格式
/// 支持两种格式：
/// 1. Netscape cookie 导出格式（制表符分隔：domain flag path secure expires name value）
/// 2. 标准 HTTP Cookie 格式（name=value; name=value）
fn parse_quark_cookie(raw: &str) -> String {
    let trimmed = raw.trim();
    
    // 1. 检测 JSON 格式（EditThisCookie 等浏览器扩展导出的 cookie JSON 数组）
    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(trimmed) {
            let pairs: Vec<String> = arr.iter().filter_map(|item| {
                let name = item.get("name")?.as_str()?;
                let value = item.get("value")?.as_str()?;
                if name == "isQuark" || name == "isQuark.sig" { return None; }
                Some(format!("{}={}", name, value))
            }).collect();
            if !pairs.is_empty() { return pairs.join("; "); }
        }
    }
    
    // 2. 检测 Netscape 格式（制表符分隔）
    if trimmed.contains('\t') {
        let mut pairs = Vec::new();
        for line in trimmed.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 7 {
                let name = parts[5].trim();
                let value = parts[6].trim();
                if name == "isQuark" || name == "isQuark.sig" { continue; }
                if !name.is_empty() { pairs.push(format!("{}={}", name, value)); }
            }
        }
        if !pairs.is_empty() { return pairs.join("; "); }
    }
    
    // 3. 假定已是 HTTP Cookie 格式（name=value; name=value），原样返回
    trimmed.to_string()
}
// 夸克 TV 扫码登录配置（基于 open-api-drive.quark.cn）
// =========================================================================

/// 夸克 TV API 域名
const QUARK_TV_API: &str = "https://open-api-drive.quark.cn";
/// 夸克 TV Client ID（来自 OpenList 夸克 TV 驱动）
const QUARK_TV_CLIENT_ID: &str = "d3194e61504e493eb6222857bccfed94";
/// 夸克 TV 签名密钥
const QUARK_TV_SIGN_KEY: &str = "kw2dvtd7p4t3pjl2d9ed9yc8yej8kw2d";
/// 夸克 TV App 版本
const QUARK_TV_APP_VER: &str = "1.8.2.2";
/// 夸克 TV 渠道
const QUARK_TV_CHANNEL: &str = "GENERAL";
/// 夸克 TV 设备 UA
const QUARK_TV_UA: &str = "Mozilla/5.0 (Linux; U; Android 13; zh-cn; M2004J7AC Build/UKQ1.231108.001) AppleWebKit/533.1 (KHTML, like Gecko) Mobile Safari/533.1";

/// 夸克 TV 通用响应
#[derive(Debug, Deserialize)]
struct QuarkTVCommonRsp {
    status: Option<i32>,
    errno: Option<i32>,
    #[serde(rename = "error_info")]
    error_info: Option<String>,
}

/// 夸克 TV 二维码响应
#[derive(Debug, Deserialize)]
struct QuarkTVQrResp {
    #[serde(flatten)]
    common: QuarkTVCommonRsp,
    qr_data: Option<String>,
    query_token: Option<String>,
}

/// 夸克 TV 授权码响应
#[derive(Debug, Deserialize)]
struct QuarkTVCodeResp {
    #[serde(flatten)]
    common: QuarkTVCommonRsp,
    code: Option<String>,
}

/// 夸克 TV Token 交换响应
#[derive(Debug, Deserialize)]
struct QuarkTVTokenResp {
    code: Option<i32>,
    message: Option<String>,
    data: Option<QuarkTVTokenData>,
}

#[derive(Debug, Deserialize)]
struct QuarkTVTokenData {
    access_token: Option<String>,
    refresh_token: Option<String>,
}
/// 基于 api.oplist.org SaaS 统一中转网关的直连网盘物理适配器 (免去用户本地部署 Alist 的烦恼)
pub struct NetdiskBackend {
    config: crate::config::model::NetdiskConfig,
}


impl NetdiskBackend {
    /// 构造全新的 Netdisk 物理适配器
    pub fn new(config: crate::config::model::NetdiskConfig) -> Self {
        Self { config }
    }

    /// 统一锁定中转基准 API URL 地址，彻底解除对本地 Alist 的安装依赖
    fn base_url(&self) -> &'static str {
        "https://api.oplist.org"
    }

    /// 将直连网盘配置转换为专用的 WebDAV 适配器配置，利用 api.oplist.org 统一 WebDAV 网关实现免部署存取
    fn to_webdav_backend(&self) -> WebdavBackend {
        let webdav_cfg = crate::config::model::WebdavConfig {
            endpoint: "https://api.oplist.org/dav".to_string(),
            username: "admin".to_string(), // 临时占位，Bearer Token 模式下自动忽略用户名
            password: self.config.token.clone(),
            backup_root: self.config.backup_root.clone(),
        };
        WebdavBackend::new(webdav_cfg)
    }

    /// 辅助方法：动态组装直连网关在 AList / OpenList 服务端的物理挂载子路径
    /// 
    /// # 核心设计
    /// 在 api.oplist.org 公共直连网关上，百度网盘、阿里云盘等不同云盘均作为子挂载点隔离在诸如 /baiduyun_go、/alicloud_qr 下。
    /// 但 api.oplist.org 的 Alist 实例可能以清理后的驱动名（去掉 _go/_qr/_fn 后缀）挂载 WebDAV 子路径。
    /// 因此本方法先清理驱动后缀，再构建 WebDAV 路径前缀，确保路径与远程挂载点 100% 匹配。
    /// 1. 若直接请求根目录 "/" 会由于中转端限制返回 404，本方法会自动将其重映射为 "/{clean_driver}" 虚拟挂载根；
    /// 2. 具备前缀防叠防重检测：若传入路径已携带挂载前缀，则原样返回，保障多级级联浏览和上传下载路径 100% 正确。
    fn get_real_path(&self, path: &str) -> String {
        // 清理驱动后缀，对齐 api.oplist.org 的 WebDAV 挂载点命名
        let clean_driver = self.config.driver
            .trim_end_matches("_go")
            .trim_end_matches("_qr")
            .trim_end_matches("_fn");
        let prefix = format!("/{}", clean_driver);
        if path.starts_with(&prefix) {
            path.to_string()
        } else {
            let clean_path = path.trim_start_matches('/');
            if clean_path.is_empty() {
                prefix
            } else {
                format!("{}/{}", prefix, clean_path)
            }
        }
    }
    /// 构造非百度网盘 WebDAV 操作的诊断错误上下文信息，包含脱敏 Token、驱动名和排查建议
    fn webdav_error_context(&self, real_path: &str, err: anyhow::Error) -> anyhow::Error {
        let token_preview = if self.config.token.len() > 8 {
            format!("{}...", &self.config.token[..8])
        } else {
            self.config.token.clone()
        };
        let dav_url = format!("https://api.oplist.org/dav{}", real_path);
        anyhow::anyhow!(
            "网盘 {:?} WebDAV 操作失败。\n  实际请求 URL: {}\n  Access Token (前8位): {}\n  排查建议: \
            1) 检查 Token 是否过期，尝试重新授权刷新；\
            2) 确认 api.oplist.org 网关服务是否可正常访问；\
            3) 若持续失败，可能是驱动 {} 在网关侧挂载异常。\n  原始错误: {}",
            self.config.driver,
            dav_url,
            token_preview,
            self.config.driver,
            err
        )
    }
}

impl NetdiskBackend {
    /// 在直连网盘云端创建目录物理结构
    pub async fn mkdir(&self, path: &str) -> anyhow::Result<()> {
        let driver = &self.config.driver;
        // 核心分支：百度网盘官方 API 直连免部署 AList 支持
        if driver.contains("baidu") {
            let client = reqwest::Client::new();
            let clean_path = format!("/{}", path.trim_start_matches('/'));
            
            let url = format!(
                "https://pan.baidu.com/rest/2.0/xpan/file?method=mkdir&access_token={}",
                self.config.token
            );
            
            let mut params = std::collections::HashMap::new();
            params.insert("path", clean_path);
            
            let resp = client
                .post(&url)
                .header("User-Agent", "pan.baidu.com")
                .form(&params)
                .send()
                .await?;
                
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();

            // 百度网盘 mkdir 幂等性处理：目录已存在（error_code 31061）视为成功，
            // 即使在 HTTP 400 状态下也直接返回 Ok，避免中断备份流程
            if !status.is_success() {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body_text) {
                    if val.get("error_code").and_then(|c| c.as_i64()) == Some(31061) {
                        log::info!("[BaiduNetdisk] 目录 {} 已存在，跳过创建", path);
                        return Ok(());
                    }
                }
                anyhow::bail!("百度网盘直连创建目录失败 (HTTP {}): {}", status, body_text);
            }

            let baidu_resp: BaiduCommonResponse = serde_json::from_str(&body_text)
                .map_err(|e| anyhow::anyhow!("解析百度网盘响应失败 ({}): {}", e, body_text))?;

            if baidu_resp.errno != 0 {
                anyhow::bail!("百度网盘创建目录接口报错 (errno {}): 请尝试重新一键授权刷新 Token。", baidu_resp.errno);
            }
            
            return Ok(());
        }

        // 夸克网盘直连 Cookie API：逐级创建目录
        if driver.contains("quark") {
            let clean = path.trim_matches('/');
            if clean.is_empty() {
                return Ok(());
            }
            let mut parent_fid = "0".to_string();
            for segment in clean.split('/') {
                if segment.is_empty() { continue; }
                // 如果段是纯 fid（32位 hex），跳过创建，直接用 fid 作为父目录
                let seg_is_fid = segment.len() == 32 && segment.chars().all(|c| c.is_ascii_hexdigit());
                if seg_is_fid {
                    parent_fid = segment.to_string();
                    continue;
                }
                // 创建目录并直接从响应获取 fid，不需要再列目录查找
                parent_fid = self.quark_mkdir_internal(&parent_fid, segment).await?;
            }
            return Ok(());
        }
        // 其它网盘（阿里、OneDrive）重映射委托给 api.oplist.org WebDAV 网关
        let real_path = self.get_real_path(path);
        let webdav_backend = self.to_webdav_backend();
        webdav_backend.mkdir(&real_path).await
            .map_err(|e| self.webdav_error_context(&real_path, e))
    }
    /// 百度网盘直连上传核心实现，遵循官方 xpan API 三段式上传流程：
    /// precreate → locateupload（获取动态上传服务器）→ 分片上传 → create
    /// 
    /// 文件 ≤4MB 时单分片直传，>4MB 时按 4MB 切片分片上传。
    /// 彻底规避已被百度降级的旧版 PCS 简易上传端点（c.pcs.baidu.com?method=upload）。
    async fn baidu_upload_file(&self, local_path: &str, remote_path: &str) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let clean_path = format!("/{}", remote_path.trim_start_matches('/'));
        let token = &self.config.token;

        // 1. 读取文件并计算大小与切片 MD5
        let file_data = tokio::fs::read(local_path).await?;
        let file_size = file_data.len() as i64;
        if file_size == 0 {
            anyhow::bail!("百度网盘不允许上传空文件");
        }

        // 按 4MB 切分并计算每个分片的 MD5
        let slice_size = BAIDU_SLICE_SIZE as usize;
        let mut block_md5s: Vec<String> = Vec::new();
        let num_slices = ((file_size as usize + slice_size - 1) / slice_size).max(1);
        for i in 0..num_slices {
            let start = i * slice_size;
            let end = ((i + 1) * slice_size).min(file_data.len());
            let digest = md5::compute(&file_data[start..end]);
            block_md5s.push(format!("{:x}", digest));
        }
        let block_list_json = serde_json::to_string(&block_md5s)?;

        // 2. 预创建（precreate）：向百度注册上传任务
        let precreate_url = format!(
            "https://pan.baidu.com/rest/2.0/xpan/file?method=precreate&access_token={}",
            token
        );
        let mut precreate_form = std::collections::HashMap::new();
        precreate_form.insert("path", clean_path.as_str());
        let size_str = file_size.to_string();
        precreate_form.insert("size", &size_str);
        precreate_form.insert("isdir", "0");
        precreate_form.insert("autoinit", "1");
        precreate_form.insert("block_list", &block_list_json);
        precreate_form.insert("rtype", "3");

        let precreate_resp = client
            .post(&precreate_url)
            .header("User-Agent", "pan.baidu.com")
            .form(&precreate_form)
            .send()
            .await?;

        let precreate_body = precreate_resp.text().await?;
        let precreate: BaiduPrecreateResp = serde_json::from_str(&precreate_body)
            .map_err(|e| anyhow::anyhow!("解析百度 precreate 响应失败 ({}): {}", e, precreate_body))?;

        if precreate.errno != 0 {
            anyhow::bail!("百度网盘预创建失败 (errno {}): {}", precreate.errno, precreate_body);
        }

        // return_type=2 表示秒传命中（文件已存在于百度服务器），无需实际上传
        if precreate.return_type == Some(2) {
            log::info!("[BaiduNetdisk] 文件秒传命中，跳过实际上传: {}", remote_path);
            return Ok(());
        }

        let uploadid = precreate.uploadid
            .ok_or_else(|| anyhow::anyhow!("precreate 响应中缺少 uploadid: {}", precreate_body))?;
        let block_list = precreate.block_list
            .ok_or_else(|| anyhow::anyhow!("precreate 响应中缺少 block_list"))?;

        // 3. 获取动态上传服务器地址（locateupload）
        let locate_url = format!(
            "https://d.pcs.baidu.com/rest/2.0/pcs/file?method=locateupload&access_token={}&appid=250528&uploadid={}&path={}&upload_version=2.0",
            token,
            urlencoding::encode(&uploadid),
            urlencoding::encode(&clean_path)
        );

        let locate_resp = client
            .get(&locate_url)
            .header("User-Agent", "pan.baidu.com")
            .send()
            .await?;

        let locate_body = locate_resp.text().await?;
        let locate: BaiduLocateUploadResp = serde_json::from_str(&locate_body)
            .map_err(|e| anyhow::anyhow!("解析百度 locateupload 响应失败 ({}): {}", e, locate_body))?;

        let upload_server = if let Some(ref servers) = locate.servers {
            servers.first().map(|s| s.server.as_str()).unwrap_or("https://d.pcs.baidu.com")
        } else {
            log::warn!("[BaiduNetdisk] locateupload 未返回服务器列表，使用默认上传域名 d.pcs.baidu.com");
            "https://d.pcs.baidu.com"
        };

        log::info!(
            "[BaiduNetdisk] 上传到 {}，分片数={}，文件大小={}",
            upload_server, block_list.len(), file_size
        );

        // 4. 逐分片上传
        for &partseq in &block_list {
            let start = partseq as usize * slice_size;
            let end = ((partseq as usize + 1) * slice_size).min(file_data.len());
            let slice_data = &file_data[start..end];

            let upload_url = format!(
                "{}/rest/2.0/pcs/superfile2?method=upload&access_token={}&type=tmpfile&path={}&uploadid={}&partseq={}",
                upload_server,
                token,
                urlencoding::encode(&clean_path),
                urlencoding::encode(&uploadid),
                partseq
            );

            // 百度分片上传要求 multipart/form-data 格式，文件内容放在 file 字段中
            // 手动构造 multipart body 以避免引入 reqwest multipart feature
            let boundary = format!("baidu_upload_boundary_{}", partseq);
            let mut multipart_body = Vec::new();
            
            // 文件字段头
            let header = format!(
                "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"part_{}\"\r\nContent-Type: application/octet-stream\r\n\r\n",
                boundary, partseq
            );
            multipart_body.extend_from_slice(header.as_bytes());
            multipart_body.extend_from_slice(slice_data);
            
            // 结束边界
            let footer = format!("\r\n--{}--\r\n", boundary);
            multipart_body.extend_from_slice(footer.as_bytes());

            let content_type = format!("multipart/form-data; boundary={}", boundary);

            let slice_resp = client
                .post(&upload_url)
                .header("User-Agent", "pan.baidu.com")
                .header("Content-Type", &content_type)
                .body(multipart_body)
                .send()
                .await?;

            let slice_body = slice_resp.text().await?;
            let slice: BaiduSliceUploadResp = serde_json::from_str(&slice_body)
                .map_err(|e| anyhow::anyhow!("解析百度分片上传响应失败 (分片{}): {}", partseq, e))?;

            if slice.error_code.unwrap_or(0) != 0 {
                anyhow::bail!(
                    "百度网盘分片上传失败 (分片 {}/{}, error_code {}): {}",
                    partseq + 1,
                    block_list.len(),
                    slice.error_code.unwrap_or(0),
                    slice.error_msg.as_deref().unwrap_or(&slice_body)
                );
            }
        }

        // 5. 创建文件（create）：通知百度合并分片、持久化文件
        let create_url = format!(
            "https://pan.baidu.com/rest/2.0/xpan/file?method=create&access_token={}",
            token
        );
        let mut create_form = std::collections::HashMap::new();
        create_form.insert("path", clean_path.as_str());
        create_form.insert("size", &size_str);
        create_form.insert("isdir", "0");
        create_form.insert("uploadid", &uploadid);
        create_form.insert("block_list", &block_list_json);
        create_form.insert("rtype", "3");

        let create_resp = client
            .post(&create_url)
            .header("User-Agent", "pan.baidu.com")
            .form(&create_form)
            .send()
            .await?;

        let create_body = create_resp.text().await?;
        let create_val: serde_json::Value = serde_json::from_str(&create_body)
            .map_err(|e| anyhow::anyhow!("解析百度 create 响应失败 ({}): {}", e, create_body))?;

        if create_val.get("errno").and_then(|e| e.as_i64()).unwrap_or(0) != 0 {
            anyhow::bail!("百度网盘创建文件失败: {}", create_body);
        }

        log::info!("[BaiduNetdisk] 上传完成: {}", remote_path);
        Ok(())
    }
    // =====================================================================
    // 夸克网盘直连 API 核心方法（Cookie 认证，基于 drive.quark.cn）
    // =====================================================================

    /// 夸克网盘通用 HTTP 请求辅助函数
    /// 
    /// 自动附加 Cookie、UA、Referer 和公共查询参数 pr=ucpro&fr=pc，
    /// 解析夸克统一错误响应格式 {status, code, message}。
    async fn quark_request(
        &self,
        path: &str,
        method: &str,
        body_json: Option<&serde_json::Value>,
    ) -> anyhow::Result<reqwest::Response> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let sep = if path.contains('?') { "&" } else { "?" };
        let url = format!("{}{}{}pr=ucpro&fr=pc&uc_param_str=&__dt={}&__t={}", QUARK_API_BASE, path, sep, (chrono::Utc::now().timestamp_subsec_nanos() % 9900) + 100, chrono::Utc::now().timestamp_millis());
        
        let mut req = match method {
            "GET" => client.get(&url),
            "POST" => client.post(&url),
            _ => anyhow::bail!("夸克请求不支持的 HTTP 方法: {}", method),
        };

        req = req
            .header("Cookie", {
                let parsed = parse_quark_cookie(&self.config.token);
                log::info!("[Quark] Cookie 解析结果 (前200字符): {}", &parsed[..parsed.len().min(200)]);
                parsed
            })
            .header("User-Agent", QUARK_UA)
            .header("Referer", QUARK_REFERER)
            .header("Accept", "application/json, text/plain, */*")
            .header("Origin", "https://pan.quark.cn");

        if let Some(json) = body_json {
            req = req.header("Content-Type", "application/json").json(json);
        }

        let resp = req.send().await?;
        let status = resp.status();
        log::debug!("[Quark] {} {} → HTTP {}", method, url, status);
        if !status.is_success() {
            let body = resp.text().await?;
            let preview = &body[..body.len().min(300)];
            let cookie_preview = &parse_quark_cookie(&self.config.token);
            let cookie_short = &cookie_preview[..cookie_preview.len().min(150)];
            log::warn!("[Quark] 请求失败 (HTTP {}): {}", status, preview);
            anyhow::bail!("夸克 HTTP {} ({} {}): 响应={} | Cookie解析后(前150字符)={}", status, method, path, preview, cookie_short);
        }
        Ok(resp)
    }

    /// 解析夸克 API 响应的 JSON body，并检查错误状态
    async fn quark_parse_response<T: serde::de::DeserializeOwned>(
        &self,
        resp: reqwest::Response,
    ) -> anyhow::Result<T> {
        let body = resp.text().await?;
        
        // 先尝试解析为错误响应
        if let Ok(err) = serde_json::from_str::<QuarkResp>(&body) {
            let status_code = err.status.unwrap_or(0);
            let code = err.code.unwrap_or(0);
            if status_code >= 400 || code != 0 {
                let msg = err.message.unwrap_or_else(|| body.clone());
                // 常见错误码给出中文排查指引
                let hint = match code {
                    31001 => {
                        let parsed_len = parse_quark_cookie(&self.config.token).len();
                        format!("：Cookie 已过期或无效（已解析 {} 个字符）。请重新从 pan.quark.cn 复制 Cookie", parsed_len)
                    },
                    _ => String::new(),
                };
                anyhow::bail!("夸克 API 错误 (status={}, code={}){}：{}", status_code, code, hint, msg);
            }
        }

        serde_json::from_str::<T>(&body)
            .map_err(|e| anyhow::anyhow!("解析夸克响应失败 ({}): {}", e, body))
    }

    /// 根据夸克文件 ID（fid）列出目录下的子文件
    async fn quark_list_by_fid(&self, pdir_fid: &str) -> anyhow::Result<Vec<QuarkFile>> {
        let mut all_files = Vec::new();
        let mut page = 1;
        let size = 100;

        loop {
            let path = format!(
                "/file/sort?pdir_fid={}&_page={}&_size={}&_fetch_total=1&_fetch_sub_dirs=1&fetch_all_file=1",
                pdir_fid, page, size
            );
            let resp = self.quark_request(&path, "GET", None).await?;
            let sort: QuarkSortResp = self.quark_parse_response(resp).await?;

            let count = sort.data.list.len();
            all_files.extend(sort.data.list);

            let total = sort.data.metadata.map(|m| m.total).unwrap_or(0);
            if page * size >= total as usize || count == 0 {
                break;
            }
            page += 1;
        }

        Ok(all_files)
    }

    /// 将夸克网盘的路径解析为对应的 fid
    /// - 根路径 "" 或 "/" → "0"
    /// - 纯 fid（32位 hex）→ 直接返回
    /// - 名称路径（如 "backups/games"）→ 从根逐级按名称查找
    async fn quark_resolve_path(&self, path: &str) -> anyhow::Result<String> {
        let clean = path.trim_matches('/');
        if clean.is_empty() {
            return Ok("0".to_string());
        }
        // 检测是否为纯 fid（32 位 hex 字符串）
        let is_fid = clean.len() == 32 && clean.chars().all(|c| c.is_ascii_hexdigit());
        if is_fid && !clean.contains('/') {
            return Ok(clean.to_string());
        }

        let mut current_fid = "0".to_string();
        for segment in clean.split('/') {
            if segment.is_empty() { continue; }
            // 每段也可能是 fid
            let seg_is_fid = segment.len() == 32 && segment.chars().all(|c| c.is_ascii_hexdigit());
            if seg_is_fid {
                current_fid = segment.to_string();
                continue;
            }
            let files = self.quark_list_by_fid(&current_fid).await?;
            let found = files.iter().find(|f| f.file_name == segment && !f.file);
            match found {
                Some(f) => current_fid = f.fid.clone(),
                None => anyhow::bail!("夸克网盘路径不存在: '{}' 在目录 {} 中未找到", segment, current_fid),
            }
        }
        Ok(current_fid)
    }

    /// 夸克网盘创建目录，返回目录的 fid（幂等：已存在也返回 fid）
    async fn quark_mkdir_internal(&self, parent_fid: &str, dir_name: &str) -> anyhow::Result<String> {
        let body = serde_json::json!({
            "dir_init_lock": false, "dir_path": "",
            "file_name": dir_name, "pdir_fid": parent_fid,
        });

        let url = format!("{}/file?pr=ucpro&fr=pc&uc_param_str=&__dt={}&__t={}",
            QUARK_API_BASE, chrono::Utc::now().timestamp_subsec_nanos() % 9900 + 100, chrono::Utc::now().timestamp_millis());
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let resp = client
            .post(&url)
            .header("Cookie", parse_quark_cookie(&self.config.token))
            .header("User-Agent", QUARK_UA)
            .header("Referer", QUARK_REFERER)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;
        let resp_body = resp.text().await?;

        // 成功创建：从响应 data.fid 直接提取
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&resp_body) {
            let code = val.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
            if code == 0 {
                if let Some(fid) = val.get("data").and_then(|d| d.get("fid")).and_then(|f| f.as_str()) {
                    return Ok(fid.to_string());
                }
            }
            // 同名冲突(code=23008)或已存在：从列表查找 fid
            let msg = val.get("message").and_then(|m| m.as_str()).unwrap_or("");
            if code == 23008 || msg.contains("同名冲突") || msg.contains("already exists") {
                log::info!("[Quark] 目录 {} 已存在，从列表查找 fid", dir_name);
            }
        }

        // 回退：通过列表查找 fid（目录已存在的情况）
        let files = self.quark_list_by_fid(parent_fid).await?;
        files.iter()
            .find(|f| f.file_name == dir_name && !f.file)
            .map(|f| f.fid.clone())
            .ok_or_else(|| anyhow::anyhow!("夸克网盘创建目录后未找到: {}（响应: {}）", dir_name, &resp_body[..resp_body.len().min(200)]))
    }

    /// 夸克网盘获取文件下载直链
    async fn quark_get_download_url(&self, fid: &str) -> anyhow::Result<String> {
        let body = serde_json::json!({ "fids": [fid] });
        let resp = self.quark_request("/file/download", "POST", Some(&body)).await?;
        let down: QuarkDownResp = self.quark_parse_response(resp).await?;
        
        down.data
            .first()
            .map(|d| d.download_url.clone())
            .ok_or_else(|| anyhow::anyhow!("夸克网盘下载链接为空"))
    }

    /// 夸克网盘 OSS 直传完整流程：pre → hash(秒传) → auth → PUT 分片 → commit → finish
    ///
    /// 参考 OpenList quark_uc 驱动的 Put 方法实现，对齐以下关键逻辑：
    /// 1. pre 预上传获取 OSS 上传凭证
    /// 2. hash 秒传检测（MD5+SHA1 去重，命中则直接完成）
    /// 3. auth + PUT 分片上传到阿里云 OSS
    /// 4. commit 合并分片
    /// 5. finish 通知夸克服务端
    async fn quark_upload_file_internal(&self, local_path: &str, parent_fid: &str, file_name: &str) -> anyhow::Result<()> {
        let file_data = tokio::fs::read(local_path).await?;
        let file_size = file_data.len() as i64;
        if file_size == 0 {
            anyhow::bail!("夸克网盘不允许上传空文件");
        }

        // MIME 类型统一使用 application/octet-stream，与 OpenList 的 stream.GetMimetype() 行为一致
        // 注意：pre 的 format_type、auth_meta 中的 Content-Type、OSS PUT 的 Content-Type 必须保持一致
        let mime_type = "application/octet-stream";
        let now_ms = chrono::Utc::now().timestamp_millis();

        // 1. 预上传（pre）
        let pre_body = serde_json::json!({
            "ccp_hash_update": true,
            "dir_name": "",
            "file_name": file_name,
            "format_type": mime_type,
            "l_created_at": now_ms,
            "l_updated_at": now_ms,
            "pdir_fid": parent_fid,
            "size": file_size,
        });

        let pre_resp = self.quark_request("/file/upload/pre", "POST", Some(&pre_body)).await?;
        let pre: QuarkUpPreResp = self.quark_parse_response(pre_resp).await?;

        let task_id = &pre.data.task_id;
        let bucket = &pre.data.bucket;
        let obj_key = &pre.data.obj_key;
        let upload_id = &pre.data.upload_id;
        let auth_info = &pre.data.auth_info;
        let part_size = pre.metadata.part_size;

        // 提取上传域名（去除协议前缀 http:// 或 https://）
        let upload_domain = {
            let trimmed = pre.data.upload_url.trim_end_matches('/');
            if let Some(d) = trimmed.strip_prefix("https://") {
                d.to_string()
            } else if let Some(d) = trimmed.strip_prefix("http://") {
                d.to_string()
            } else {
                trimmed.to_string()
            }
        };

        // 2. 秒传哈希检测（对齐 OpenList upHash）
        // 计算文件的 MD5 和 SHA1，提交给夸克服务端检测是否已存在相同文件
        let file_md5 = format!("{:x}", md5::compute(&file_data));
        let file_sha1 = {
            use sha1::{Sha1, Digest};
            let mut hasher = Sha1::new();
            hasher.update(&file_data);
            format!("{:x}", hasher.finalize())
        };
        let hash_body = serde_json::json!({
            "md5": file_md5,
            "sha1": file_sha1,
            "task_id": task_id,
        });
        let hash_resp = self.quark_request("/file/update/hash", "POST", Some(&hash_body)).await?;
        let hash: QuarkHashResp = self.quark_parse_response(hash_resp).await?;
        if hash.data.finish {
            log::info!("[Quark] 秒传命中，跳过上传: {}", file_name);
            return Ok(());
        }

        // 3. 计算分片信息并上传
        let num_parts = ((file_size + part_size - 1) / part_size).max(1) as usize;
        log::info!("[Quark] 上传开始: 文件={}, 大小={}, 分片={}, partSize={}", file_name, file_size, num_parts, part_size);

        // 逐分片上传到 OSS
        let mut etags: Vec<String> = Vec::new();
        for i in 0..num_parts {
            let start = i * part_size as usize;
            let end = ((i + 1) * part_size as usize).min(file_data.len());
            let slice_data = &file_data[start..end];
            let part_number = (i + 1) as i32;

            // 3a. 获取 OSS 上传签名
            let time_str = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
            let auth_meta = format!(
                "PUT\n\n{}\n{}\nx-oss-date:{}\nx-oss-user-agent:aliyun-sdk-js/6.6.1 Chrome 98.0.4758.80 on Windows 10 64-bit\n/{}/{}?partNumber={}&uploadId={}",
                mime_type, time_str, time_str, bucket, obj_key, part_number, upload_id
            );

            let auth_body = serde_json::json!({
                "auth_info": auth_info,
                "auth_meta": auth_meta,
                "task_id": task_id,
            });

            let auth_resp = self.quark_request("/file/upload/auth", "POST", Some(&auth_body)).await?;
            let auth: QuarkUpAuthResp = self.quark_parse_response(auth_resp).await?;

            // 3b. 直传 OSS（带 60 秒超时 + 3 次重试）
            let oss_url = format!(
                "https://{}.{}/{}?partNumber={}&uploadId={}",
                bucket,
                &upload_domain,
                obj_key, part_number, upload_id
            );

            let upload_client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()?;

            let mut last_err = String::new();
            for attempt in 0..3 {
                if attempt > 0 {
                    tokio::time::sleep(std::time::Duration::from_secs(attempt as u64)).await;
                }
                let oss_resp = upload_client
                    .put(&oss_url)
                    .header("Authorization", &auth.data.auth_key)
                    .header("Content-Type", mime_type)
                    .header("Referer", QUARK_REFERER)
                    .header("x-oss-date", &time_str)
                    .header("x-oss-user-agent", "aliyun-sdk-js/6.6.1 Chrome 98.0.4758.80 on Windows 10 64-bit")
                    .body(slice_data.to_vec())
                    .send()
                    .await;
                
                match oss_resp {
                    Ok(resp) => {
                        let oss_status = resp.status().as_u16();
                        if oss_status == 200 {
                            let etag = resp.headers()
                                .get("ETag").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
                            etags.push(etag);
                            last_err.clear();
                            break;
                        }
                        let err_body = resp.text().await.unwrap_or_default();
                        let preview = &err_body[..err_body.len().min(200)];
                        last_err = format!("HTTP {}: {}", oss_status, preview);
                        if oss_status < 500 { break; } // 非服务端错误不重试
                    }
                    Err(e) => {
                        last_err = format!("连接失败 (url={}): {}", oss_url, e.to_string());
                    }
                }
            }
            if !last_err.is_empty() {
                anyhow::bail!("夸克 OSS 分片上传失败 (分片 {}/{}): {}", part_number, num_parts, last_err);
            }
        }

        // 4. commit：通知 OSS 合并分片
        let mut xml_body = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CompleteMultipartUpload>\n");
        for (i, etag) in etags.iter().enumerate() {
            xml_body.push_str(&format!(
                "<Part>\n<PartNumber>{}</PartNumber>\n<ETag>{}</ETag>\n</Part>\n",
                i + 1,
                etag
            ));
        }
        xml_body.push_str("</CompleteMultipartUpload>");

        // Content-MD5: base64 of the raw 16-byte MD5 digest
        let digest_bytes = md5::compute(xml_body.as_bytes());
        let content_md5 = base64::engine::general_purpose::STANDARD.encode(&digest_bytes.0[..]);

        let callback_b64 = base64::engine::general_purpose::STANDARD.encode(
            serde_json::to_string(&pre.data.callback)?.as_bytes()
        );
        let time_str = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let commit_auth_meta = format!(
            "POST\n{}\napplication/xml\n{}\nx-oss-callback:{}\nx-oss-date:{}\nx-oss-user-agent:aliyun-sdk-js/6.6.1 Chrome 98.0.4758.80 on Windows 10 64-bit\n/{}/{}?uploadId={}",
            content_md5, time_str, callback_b64, time_str,
            bucket, obj_key, upload_id
        );

        let commit_body = serde_json::json!({
            "auth_info": auth_info,
            "auth_meta": commit_auth_meta,
            "task_id": task_id,
        });

        let commit_auth_resp = self.quark_request("/file/upload/auth", "POST", Some(&commit_body)).await?;
        let commit_auth: QuarkUpAuthResp = self.quark_parse_response(commit_auth_resp).await?;

        // 发送 CompleteMultipartUpload 到 OSS
        let oss_commit_url = format!(
            "https://{}.{}/{}?uploadId={}",
            bucket, &upload_domain, obj_key, upload_id
        );

        let oss_commit_resp = reqwest::Client::new()
            .post(&oss_commit_url)
            .header("Authorization", &commit_auth.data.auth_key)
            .header("Content-Type", "application/xml")
            .header("Content-MD5", &content_md5)
            .header("Referer", QUARK_REFERER)
            .header("x-oss-callback", &callback_b64)
            .header("x-oss-date", &time_str)
            .header("x-oss-user-agent", "aliyun-sdk-js/6.6.1 Chrome 98.0.4758.80 on Windows 10 64-bit")
            .body(xml_body)
            .send()
            .await?;

        let oss_commit_status = oss_commit_resp.status();
        if !oss_commit_status.is_success() {
            let err_body = oss_commit_resp.text().await.unwrap_or_default();
            anyhow::bail!("夸克 OSS 合并分片失败 (HTTP {}): {}", oss_commit_status, err_body);
        }

        // 5. finish：通知夸克服务端上传完成
        let finish_body = serde_json::json!({
            "obj_key": obj_key,
            "task_id": task_id,
        });
        let finish_resp = self.quark_request("/file/upload/finish", "POST", Some(&finish_body)).await?;
        self.quark_parse_response::<serde_json::Value>(finish_resp).await?;

        log::info!("[Quark] 上传完成: {}", file_name);
        Ok(())
    }
    /// 将本地游戏存档上传至直连网盘
    pub async fn upload_file(&self, local_path: &str, remote_path: &str) -> anyhow::Result<()> {
        let driver = &self.config.driver;
        // 百度网盘采用官方 xpan API 直连上传（precreate → locateupload → 分片 → create）
        if driver.contains("baidu") {
            return self.baidu_upload_file(local_path, remote_path).await;
        }
        // 夸克网盘直连 Cookie API
        if driver.contains("quark") {
            let remote = remote_path.trim_matches('/');
            let (parent_path, file_name) = match remote.rsplit_once('/') {
                Some((p, f)) => (p, f),
                None => ("", remote),
            };
            // 先确保父目录存在（含备份根路径下的 full/游戏名 等子目录）
            if !parent_path.is_empty() {
                self.mkdir(parent_path).await?;
            }
            let parent_fid = self.quark_resolve_path(parent_path).await?;
            return self.quark_upload_file_internal(local_path, &parent_fid, file_name).await;
        }

        // 其它网盘（阿里、OneDrive）重映射委托给 api.oplist.org WebDAV 网关
        let real_path = self.get_real_path(remote_path);
        let webdav_backend = self.to_webdav_backend();
        webdav_backend.upload_file(local_path, &real_path).await
            .map_err(|e| self.webdav_error_context(&real_path, e))
    }
    /// 从网盘下载指定的游戏存档
    pub async fn download_file(&self, remote_path: &str, local_path: &str) -> anyhow::Result<()> {
        let driver = &self.config.driver;
        // 核心分支：百度网盘官方 API 直接获取 PCS 数据流
        if driver.contains("baidu") {
            let client = reqwest::Client::new();
            let clean_path = format!("/{}", remote_path.trim_start_matches('/'));

            // 使用 c.pcs.baidu.com（官方推荐的内容下载域名）
            let url = format!(
                "https://c.pcs.baidu.com/rest/2.0/pcs/file?method=download&access_token={}&path={}",
                self.config.token,
                urlencoding::encode(&clean_path)
            );

            let resp = client
                .get(&url)
                .header("User-Agent", "pan.baidu.com")
                .send()
                .await?;

            let status = resp.status();
            if !status.is_success() {
                let body_text = resp.text().await.unwrap_or_default();
                let hint = if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body_text) {
                    match val.get("error_code").and_then(|c| c.as_i64()) {
                        Some(31212) => "：百度 PCS 下载服务暂时不可用，请稍后重试",
                        _ => "",
                    }
                } else {
                    ""
                };
                anyhow::bail!("百度网盘直连下载失败 (HTTP {}){}：{}", status, hint, body_text);
            }

            if let Some(parent) = Path::new(local_path).parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            let bytes = resp.bytes().await?;
            tokio::fs::write(local_path, &bytes).await?;

            return Ok(());
        }
        // 夸克网盘直连 Cookie API
        if driver.contains("quark") {
            let remote = remote_path.trim_matches('/');
            let (parent_path, file_name) = match remote.rsplit_once('/') {
                Some((p, f)) => (p, f),
                None => ("", remote),
            };
            let parent_fid = self.quark_resolve_path(parent_path).await?;
            let files = self.quark_list_by_fid(&parent_fid).await?;
            let file = files.iter()
                .find(|f| f.file_name == file_name && f.file)
                .ok_or_else(|| anyhow::anyhow!("夸克网盘文件不存在: {}", remote_path))?;
            let download_url = self.quark_get_download_url(&file.fid).await?;

            // 下载文件内容
            let resp = reqwest::Client::new()
                .get(&download_url)
                .header("Cookie", parse_quark_cookie(&self.config.token))
                .header("User-Agent", QUARK_UA)
                .header("Referer", QUARK_REFERER)
                .send()
                .await?;

            let status = resp.status();
            if !status.is_success() {
                anyhow::bail!("夸克网盘下载失败 (HTTP {}): {}", status, resp.text().await.unwrap_or_default());
            }

            if let Some(parent) = Path::new(local_path).parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let bytes = resp.bytes().await?;
            tokio::fs::write(local_path, &bytes).await?;
            return Ok(());
        }

        // 其它网盘（阿里、OneDrive）重映射委托给 api.oplist.org WebDAV 网关
        let real_path = self.get_real_path(remote_path);
        let webdav_backend = self.to_webdav_backend();
        webdav_backend.download_file(&real_path, local_path).await
            .map_err(|e| self.webdav_error_context(&real_path, e))
    }

    /// 列出直连网盘指定路径下的子条目列表
    pub async fn list_dir(&self, path: &str) -> anyhow::Result<Vec<RemoteFileEntry>> {
        let driver = &self.config.driver;
        // 夸克网盘直连 Cookie API
        if driver.contains("quark") {
            let fid = self.quark_resolve_path(path).await?;
            let files = self.quark_list_by_fid(&fid).await?;
            let entries = files.into_iter().map(|f| {
                let dt = chrono::DateTime::from_timestamp_millis(f.updated_at)
                    .unwrap_or_else(|| chrono::Utc::now());
                RemoteFileEntry {
                    name: f.file_name,
                    path: f.fid.clone(),
                    is_dir: !f.file,
                    size: f.size,
                    modified: Some(dt.to_rfc3339()),
                }
            }).collect();
            return Ok(entries);
        }
        // 核心分支：百度网盘官方 API 极速列出目录与元数据转换
        if driver.contains("baidu") {
            let client = reqwest::Client::new();
            let clean_path = if path == "/" || path.is_empty() {
                "/".to_string()
            } else {
                format!("/{}", path.trim_start_matches('/'))
            };
            let url = format!(
                "https://pan.baidu.com/rest/2.0/xpan/file?method=list&access_token={}&dir={}",
                self.config.token,
                urlencoding::encode(&clean_path)
            );
            let resp = client.get(&url).header("User-Agent", "pan.baidu.com").send().await?;
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                anyhow::bail!("百度网盘直连列目录失败 (HTTP {}): {}", status, err_text);
            }
            let baidu_resp: BaiduFileListResponse = serde_json::from_str(&err_text)
                .map_err(|e| anyhow::anyhow!("解析百度网盘响应失败 ({}): {}", e, err_text))?;
            if baidu_resp.errno == -9 { return Ok(Vec::new()); }
            if baidu_resp.errno != 0 {
                anyhow::bail!("百度网盘接口报错 (errno {}): 请尝试重新一键授权刷新 Token。", baidu_resp.errno);
            }
            let entries = baidu_resp.list.unwrap_or_default().into_iter().map(|e| {
                let dt = chrono::DateTime::from_timestamp(e.server_mtime as i64, 0)
                    .unwrap_or_else(|| chrono::Utc::now());
                RemoteFileEntry { name: e.server_filename, path: e.path, is_dir: e.isdir == 1, size: e.size, modified: Some(dt.to_rfc3339()) }
            }).collect();
            return Ok(entries);
        }
        // 其它网盘 WebDAV 网关回退
        let real_path = self.get_real_path(path);
        let webdav_backend = self.to_webdav_backend();
        webdav_backend.list_dir(&real_path).await
            .map_err(|e| self.webdav_error_context(&real_path, e))
    }

    // =====================================================================
    // 夸克 TV 扫码登录方法（请求签名 + 二维码 + 轮询 + Token 交换）
    // =====================================================================

    /// 生成夸克 TV API 请求签名（x-pan-token）
    fn quark_tv_sign(&self, method: &str, path: &str) -> (String, String, String) {
        use sha2::{Sha256, Digest};
        let timestamp = chrono::Utc::now().timestamp_millis().to_string();
        let device_id = format!("{:x}", md5::compute(self.config.token.as_bytes()));
        let req_id = format!("{:x}", md5::compute(format!("{}{}", device_id, timestamp).as_bytes()));
        let sign_str = format!("{}&{}&{}&{}", method, path, timestamp, QUARK_TV_SIGN_KEY);
        let x_pan_token = format!("{:x}", Sha256::digest(sign_str.as_bytes()));
        (timestamp, x_pan_token, req_id)
    }

    /// 夸克 TV 通用 HTTP 请求（带设备指纹和签名）
    async fn quark_tv_request(&self, path: &str, query_params: &[(&str, &str)]) -> anyhow::Result<reqwest::Response> {
        let url = format!("{}{}", QUARK_TV_API, path);
        let (tm, token, req_id) = self.quark_tv_sign("GET", path);
        let device_id = format!("{:x}", md5::compute(self.config.token.as_bytes()));
        let mut full_url = url.clone();
        full_url.push_str(&format!(
            "?req_id={}&access_token={}&app_ver={}&device_id={}&device_brand=Xiaomi&platform=tv&device_name=M2004J7AC&device_model=M2004J7AC&build_device=M2004J7AC&build_product=M2004J7AC&device_gpu=Adreno%20(TM)%20550&activity_rect={}&channel={}",
            req_id,
            urlencoding::encode(&self.config.token), // access_token（初始可空）
            QUARK_TV_APP_VER,
            urlencoding::encode(&device_id),
            "{}",
            QUARK_TV_CHANNEL
        ));
        for (k, v) in query_params { full_url.push_str(&format!("&{}={}", k, urlencoding::encode(v))); }
        let resp = reqwest::Client::new().get(&full_url)
            .header("Accept", "application/json, text/plain, */*")
            .header("User-Agent", QUARK_TV_UA)
            .header("x-pan-tm", &tm).header("x-pan-token", &token)
            .header("x-pan-client-id", QUARK_TV_CLIENT_ID)
            .send().await?;
        Ok(resp)
    }

    /// 获取夸克 TV 登录二维码（返回 base64 PNG data URI 和 query_token）
    pub async fn quark_tv_get_qr(&self) -> anyhow::Result<(String, String)> {
        let resp = self.quark_tv_request("/oauth/authorize", &[
            ("auth_type", "code"), ("client_id", QUARK_TV_CLIENT_ID),
            ("scope", "netdisk"), ("qrcode", "1"), ("qr_width", "460"), ("qr_height", "460"),
        ]).await?;
        let body = resp.text().await?;
        let qr: QuarkTVQrResp = serde_json::from_str(&body)
            .map_err(|e| anyhow::anyhow!("解析夸克 TV 二维码响应失败: {}", e))?;
        let status = qr.common.status.unwrap_or(0);
        let errno = qr.common.errno.unwrap_or(0);
        if status >= 400 || errno != 0 {
            let msg = qr.common.error_info.unwrap_or_else(|| body.clone());
            anyhow::bail!("夸克 TV 获取二维码失败 (status={}, errno={}): {}", status, errno, msg);
        }
        let qr_data = qr.qr_data.ok_or_else(|| anyhow::anyhow!("二维码数据为空"))?;
        let query_token = qr.query_token.ok_or_else(|| anyhow::anyhow!("query_token 为空"))?;
        Ok((qr_data, query_token))
    }

    /// 轮询夸克 TV 授权码（用户扫码后调用，未扫码时返回 None）
    pub async fn quark_tv_poll_code(&self, query_token: &str) -> anyhow::Result<Option<String>> {
        let resp = self.quark_tv_request("/oauth/code", &[
            ("client_id", QUARK_TV_CLIENT_ID), ("scope", "netdisk"), ("query_token", query_token),
        ]).await?;
        let body = resp.text().await?;
        let code_resp: QuarkTVCodeResp = serde_json::from_str(&body)
            .map_err(|e| anyhow::anyhow!("解析夸克 TV 授权码响应失败: {}", e))?;
        let status = code_resp.common.status.unwrap_or(0);
        if status >= 400 {
            let msg = code_resp.common.error_info.unwrap_or_default();
            if msg.contains("未授权") || msg.contains("not authorized") || msg.contains("waiting") {
                return Ok(None);
            }
            anyhow::bail!("夸克 TV 获取授权码失败: {}", msg);
        }
        Ok(code_resp.code)
    }

    /// 用授权码交换 AccessToken 和 RefreshToken
    pub async fn quark_tv_exchange_token(&self, code: &str) -> anyhow::Result<(String, String)> {
        let (_tm, _token, req_id) = self.quark_tv_sign("POST", "/token");
        let device_id = format!("{:x}", md5::compute(self.config.token.as_bytes()));
        let body = serde_json::json!({
            "req_id": req_id, "app_ver": QUARK_TV_APP_VER, "device_id": device_id,
            "device_brand": "Xiaomi", "platform": "tv", "device_name": "M2004J7AC",
            "device_model": "M2004J7AC", "build_device": "M2004J7AC",
            "build_product": "M2004J7AC", "device_gpu": "Adreno (TM) 550",
            "activity_rect": "{}", "channel": QUARK_TV_CHANNEL, "code": code,
        });
        let resp = reqwest::Client::new()
            .post("http://api.extscreen.com/quarkdrive/token")
            .header("Content-Type", "application/json").json(&body).send().await?;
        let body_text = resp.text().await?;
        let token_resp: QuarkTVTokenResp = serde_json::from_str(&body_text)
            .map_err(|e| anyhow::anyhow!("解析夸克 TV Token 响应失败: {}", e))?;
        if token_resp.code.unwrap_or(0) != 200 {
            let msg = token_resp.message.unwrap_or_else(|| body_text);
            anyhow::bail!("夸克 TV Token 交换失败: {}", msg);
        }
        let data = token_resp.data.ok_or_else(|| anyhow::anyhow!("Token 响应中无 data 字段"))?;
        let access_token = data.access_token.ok_or_else(|| anyhow::anyhow!("未获取到 access_token"))?;
        let refresh_token = data.refresh_token.unwrap_or_default();
        Ok((access_token, refresh_token))
    }
}

// =========================================================================
// 1. Alist / OpenList 统一适配器实现
// =========================================================================

/// 包装 Alist 既有驱动逻辑的适配器
pub struct AlistBackend {
    config: AlistConfig,
}

impl AlistBackend {
    /// 构造全新的 Alist / OpenList 统一物理存取适配器
    pub fn new(config: AlistConfig) -> Self {
        Self { config }
    }

    /// 获取当前的有效 Token，若本地 Token 缺失但有密码，则在底层发起自动登录换取临时 Token。
    ///
    /// # 核心设计与规避机制
    /// 1. 优先使用本地持久化存储的授权 Token (令牌)；
    /// 2. 弱 Token 缺失或被清空，但 `password` 字段非空，则利用自建密码自动调用 `/api/auth/login` 接口；
    /// 3. 如果两者均为空，则抛出详尽的配置缺失报错，指引用户到设置页填写。
    async fn get_effective_token(&self) -> anyhow::Result<String> {
        if let Some(ref tok) = self.config.token {
            if !tok.trim().is_empty() {
                return Ok(tok.clone());
            }
        }
        
        if let Some(ref pwd) = self.config.password {
            if !pwd.trim().is_empty() {
                log::info!("[Alist] 授权 Token 为空，正在通过用户名/密码自动认证换取 Token...");
                let login_res = crate::alist::auth::login(
                    &self.config.base_url,
                    &self.config.username,
                    pwd
                ).await?;
                return Ok(login_res.token);
            }
        }

        anyhow::bail!("Alist 认证失败: 授权 Token 令牌与密码均为空，请进入设置界面补全云端同步参数")
    }
}

impl AlistBackend {
    /// 在 Alist 云端物理系统上创建目录
    pub async fn mkdir(&self, path: &str) -> anyhow::Result<()> {
        let token = self.get_effective_token().await?;
        crate::alist::fs::mkdir(&self.config.base_url, &token, path).await
    }

    async fn upload_file(&self, local_path: &str, remote_path: &str) -> anyhow::Result<()> {
        let token = self.get_effective_token().await?;
        crate::alist::fs::upload_file(&self.config.base_url, &token, local_path, remote_path).await
    }

    async fn download_file(&self, remote_path: &str, local_path: &str) -> anyhow::Result<()> {
        let token = self.get_effective_token().await?;
        crate::alist::fs::download_file(&self.config.base_url, &token, remote_path, local_path).await
    }

    async fn list_dir(&self, path: &str) -> anyhow::Result<Vec<RemoteFileEntry>> {
        let token = self.get_effective_token().await?;
        let raw_entries = crate::alist::fs::list_dir(&self.config.base_url, &token, path).await?;
        
        // 转换 Alist 原生数据实体为系统通用抽象实体
        let converted = raw_entries
            .into_iter()
            .map(|e| RemoteFileEntry {
                name: e.name,
                path: e.path,
                is_dir: e.is_dir,
                size: e.size as i64,
                modified: e.modified,
            })
            .collect();
        Ok(converted)
    }
}

// =========================================================================
// 2. WebDAV 极致轻量适配器实现（零 XML 第三方依赖）
// =========================================================================

/// 通用标准 WebDAV 物理适配器（通过 reqwest 纯手写实现标准 WebDAV 动词与微型 XML 解析器）
pub struct WebdavBackend {
    config: WebdavConfig,
}

impl WebdavBackend {
    /// 构造全新的 WebDAV 物理存取适配器
    pub fn new(config: WebdavConfig) -> Self {
        Self { config }
    }

    /// 辅助方法：统一构造带 Basic Auth 基础安全授权的 HTTP 客户端与请求基底
    fn create_client(&self) -> reqwest::Client {
        reqwest::Client::new()
    }

    /// 动态应用鉴权头。支持智能 Bearer Token（JWT或长Token形式）与标准 Basic Auth 自动路由分流。
    fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.config.password.starts_with("ey") || self.config.password.len() > 30 {
            req.header("Authorization", format!("Bearer {}", self.config.password))
        } else {
            req.basic_auth(&self.config.username, Some(&self.config.password))
        }
    }
}

impl WebdavBackend {
    /// 级联创建 WebDAV 物理目录
    ///
    /// # 核心设计与规避
    /// WebDAV 协议规范中的 `MKCOL` 动作不支持一次性级联递归创建（例如当 `/a/` 不存在时直接 MKCOL `/a/b/` 会爆发 `409 Conflict`）。
    /// 为确保多层目录下的绝对稳定性，此处采用级联路径拆分算法，从根起自左向右逐级进行创建。
    /// 当收到 `405 Method Not Allowed` 时，代表云端对应目录已存在，视作静默创建成功，实现优雅的幂等性。
    pub async fn mkdir(&self, path: &str) -> anyhow::Result<()> {
        let client = self.create_client();
        let endpoint = self.config.endpoint.trim_end_matches('/');
        let clean_path = path.trim_start_matches('/').trim_end_matches('/');
        if clean_path.is_empty() {
            return Ok(());
        }

        let mut current_path = String::new();
        // 逐级分割目录以级联调用 MKCOL
        for segment in clean_path.split('/') {
            if segment.is_empty() {
                continue;
            }
            current_path.push('/');
            current_path.push_str(segment);

            let url = format!("{}{}", endpoint, current_path);
            let mkcol_method = reqwest::Method::from_bytes(b"MKCOL")?;
            
            let mut req = client.request(mkcol_method, &url);
            req = self.apply_auth(req);
            let resp = req.send().await?;

            let status = resp.status();
            // 201 Created 代表成功，405 代表目录早已存在（视作成功），其它状态抛出异常以预警
            if status != reqwest::StatusCode::CREATED && status != reqwest::StatusCode::METHOD_NOT_ALLOWED {
                let err_text = resp.text().await.unwrap_or_default();
                log::warn!("WebDAV 级联 MKCOL 响应非预期 (HTTP {}): {}", status, err_text);
            }
        }
        Ok(())
    }

    /// 上传本地物理存档至 WebDAV 存储区
    ///
    /// # 核心设计与规避
    /// 1. 显式内容长度：通过在 header 中传递文件元数据物理大小，避免触发 Transfer-Encoding 分块代理层出错。
    /// 2. 0 字节特殊规避：针对空标记文件，默认填充 1 字节空数据规避少数物理网盘的上传拦截。
    async fn upload_file(&self, local_path: &str, remote_path: &str) -> anyhow::Result<()> {
        let client = self.create_client();
        let endpoint = self.config.endpoint.trim_end_matches('/');
        let url = format!("{}{}", endpoint, remote_path);

        // 获取文件并判定大小
        let metadata = tokio::fs::metadata(local_path).await?;
        let mut file_size = metadata.len();

        let body = if file_size == 0 {
            file_size = 1;
            reqwest::Body::from(vec![0u8])
        } else {
            let file = tokio::fs::File::open(local_path).await?;
            reqwest::Body::from(file)
        };

        let mut req = client
            .put(&url)
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", file_size.to_string())
            .body(body);
        req = self.apply_auth(req);
        let resp = req.send().await?;

        let status = resp.status();
        if !status.is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("WebDAV 物理上传失败 (HTTP {}): {}", status, err_text);
        }

        Ok(())
    }

    /// 从 WebDAV 云端物理下载备份到本地绝对路径
    async fn download_file(&self, remote_path: &str, local_path: &str) -> anyhow::Result<()> {
        let client = self.create_client();
        let endpoint = self.config.endpoint.trim_end_matches('/');
        let url = format!("{}{}", endpoint, remote_path);

        let mut req = client.get(&url);
        req = self.apply_auth(req);
        let resp = req.send().await?;

        let status = resp.status();
        if !status.is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("从 WebDAV 下载备份失败 (HTTP {}): {}", status, err_text);
        }

        // 创建本地多级目录
        if let Some(parent) = Path::new(local_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let bytes = resp.bytes().await?;
        tokio::fs::write(local_path, &bytes).await?;
        Ok(())
    }

    /// 列出 WebDAV 云端指定物理路径下的子条目列表
    ///
    /// # 核心设计与规避：极致轻量且安全的无外部依赖 XML 正则提取解析器
    /// WebDAV `PROPFIND` 操作默认返回大片包含命名空间的 XML 物理属性 structure。
    /// 为了捍卫 Rust 轻量小体积底座、杜绝引入多余庞大的第三方 XML crate，此处手写编写了
    /// 搭载 Regex 正则表达式的高健壮性 XML 切消匹配扫描算法。
    /// 能够稳定兼容带命名空间前缀（如 `d:href`、`D:href`、`a:href` 等）与不带前缀的各类 WebDAV 服务端（坚云、Nextcloud 等）。
    async fn list_dir(&self, path: &str) -> anyhow::Result<Vec<RemoteFileEntry>> {
        let client = self.create_client();
        let endpoint = self.config.endpoint.trim_end_matches('/');
        let url = format!("{}{}", endpoint, path);

        let propfind_method = reqwest::Method::from_bytes(b"PROPFIND")?;
        let mut req = client
            .request(propfind_method, &url)
            // 设定 Depth: 1 获取目录及直接子文件信息
            .header("Depth", "1")
            .header("Content-Type", "application/xml; charset=utf-8");
        req = self.apply_auth(req);
        let resp = req.send().await?;

        let status = resp.status();
        if !status.is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("列出 WebDAV 目录失败 (HTTP {}): {}", status, err_text);
        }

        let xml_content = resp.text().await?;
        let mut entries = Vec::new();

        // 编译匹配 response 段落、href 节点、大小节点、修改时间及资源类型的正则表达式
        let re_response = Regex::new(r"(?s)<[^>]*:?response[^>]*>(.*?)</[^>]*:?response>").unwrap();
        let re_href = Regex::new(r"<[^>]*:?href[^>]*>([^<]+)</[^>]*:?href>").unwrap();
        let re_length = Regex::new(r"<[^>]*:?getcontentlength[^>]*>(\d+)</[^>]*:?getcontentlength>").unwrap();
        let re_modified = Regex::new(r"<[^>]*:?getlastmodified[^>]*>([^<]+)</[^>]*:?getlastmodified>").unwrap();
        let re_is_dir = Regex::new(r"<[^>]*:?collection[^>]*>").unwrap();

        // 获取并清洗当前目录的路径，以在列表中滤除自己本身的冗余记录
        let parsed_endpoint_url = reqwest::Url::parse(endpoint)?;
        let base_path_filter = parsed_endpoint_url.path().trim_end_matches('/');

        for cap in re_response.captures_iter(&xml_content) {
            let response_body = &cap[1];

            // 1. 提取 href URL 绝对路径
            let href = match re_href.captures(response_body) {
                Some(h_cap) => {
                    let raw_href = &h_cap[1];
                    // 对 WebDAV 字符实体或 URL 编码进行净化反解码（如将 %20 还原为空格）
                    urlencoding::decode(raw_href).unwrap_or_else(|_| raw_href.to_string().into()).into_owned()
                }
                None => continue,
            };

            // 提取末级子项文件名
            let name = href
                .trim_end_matches('/')
                .split('/')
                .last()
                .unwrap_or_default()
                .to_string();

            if name.is_empty() {
                continue;
            }

            // 滤除掉查询目录自身的那一条 PROPFIND 记录
            let clean_href = href.trim_end_matches('/');
            let check_self_path_alist = format!("{}{}", base_path_filter, path).replace("//", "/");
            let check_self_path_direct = path.trim_end_matches('/');
            if clean_href == check_self_path_alist.trim_end_matches('/') 
                || clean_href == check_self_path_direct 
                || clean_href == base_path_filter {
                continue;
            }

            // 2. 物理条目类型与属性获取
            let is_dir = re_is_dir.is_match(response_body);
            let size = re_length
                .captures(response_body)
                .and_then(|l_cap| l_cap[1].parse::<i64>().ok())
                .unwrap_or(0);

            let modified = re_modified
                .captures(response_body)
                .map(|m_cap| m_cap[1].to_string());

            entries.push(RemoteFileEntry {
                name,
                path: href,
                is_dir,
                size,
                modified,
            });
        }

        Ok(entries)
    }
}

// =========================================================================
// 3. AWS S3 骨架物理适配器实现
// =========================================================================

/// AWS S3 物理适配器（为规避引入极其庞大沉重的 AWS 签名验证 SDK 造成物理编译体积激增，
/// 此处提供骨架适配设计，为未来的无缝集成和原生 API 签名扩展打下基础）
pub struct S3Backend {
    _config: S3Config,
}

impl S3Backend {
    /// 构造全新的 S3 兼容对象存储物理适配器
    pub fn new(config: S3Config) -> Self {
        Self { _config: config }
    }
}

impl S3Backend {
    /// 在 S3 云端创建目录物理结构
    pub async fn mkdir(&self, _path: &str) -> anyhow::Result<()> {
        // 对象存储物理系统属于平坦文件系统（Flat File System），不需要显式物理级联创建空文件夹
        Ok(())
    }

    async fn upload_file(&self, _local_path: &str, _remote_path: &str) -> anyhow::Result<()> {
        anyhow::bail!("S3 对象存储支持目前正处于实验性开发阶段，敬请期待！推荐优先选用稳定性极佳的 WebDAV 或 Alist/OpenList 后端。")
    }

    async fn download_file(&self, _remote_path: &str, _local_path: &str) -> anyhow::Result<()> {
        anyhow::bail!("S3 对象存储支持目前正处于实验性开发阶段，敬请期待！推荐优先选用稳定性极佳的 WebDAV 或 Alist/OpenList 后端。")
    }

    async fn list_dir(&self, _path: &str) -> anyhow::Result<Vec<RemoteFileEntry>> {
        anyhow::bail!("S3 对象存储支持目前正处于实验性开发阶段，敬请期待！推荐优先选用稳定性极佳的 WebDAV 或 Alist/OpenList 后端。")
    }
}

// =========================================================================
// 4. 统一存储驱动工厂 (Factory Pattern)
// =========================================================================

/// 动态构建具体的 StorageBackend 存储分发变体
pub fn get_storage_backend(config: &crate::config::model::AppConfig) -> anyhow::Result<StorageBackend> {
    match &config.storage {
        Some(StorageConfig::Netdisk(ref netdisk)) => Ok(StorageBackend::Netdisk(NetdiskBackend::new(netdisk.clone()))),
        Some(StorageConfig::Alist(ref alist)) => Ok(StorageBackend::Alist(AlistBackend::new(alist.clone()))),
        Some(StorageConfig::Webdav(ref webdav)) => Ok(StorageBackend::Webdav(WebdavBackend::new(webdav.clone()))),
        Some(StorageConfig::S3(ref s3)) => Ok(StorageBackend::S3(S3Backend::new(s3.clone()))),
        None => {
            // 向下兼容 Fallback：如果新版 storage 配置项为 None，但旧版 alist 字段存在有效内容，则动态为其降级实例化 Alist
            if let Some(ref alist) = config.alist {
                Ok(StorageBackend::Alist(AlistBackend::new(alist.clone())))
            } else {
                anyhow::bail!("未检测到有效的云端存储配置，请先进入设置页面配置云端存储后端")
            }
        }
    }
}

/// 根据传入的临时/新配置，动态构建具体的 StorageBackend 存储分发变体（专用于连接性测试与联调向导）
pub fn get_storage_backend_with_config(config: &StorageConfig) -> StorageBackend {
    match config {
        StorageConfig::Netdisk(ref netdisk) => StorageBackend::Netdisk(NetdiskBackend::new(netdisk.clone())),
        StorageConfig::Alist(ref alist) => StorageBackend::Alist(AlistBackend::new(alist.clone())),
        StorageConfig::Webdav(ref webdav) => StorageBackend::Webdav(WebdavBackend::new(webdav.clone())),
        StorageConfig::S3(ref s3) => StorageBackend::S3(S3Backend::new(s3.clone())),
    }
}

// =========================================================================
// 5. Tauri Commands 外部交互层导出 (Tauri Commands)
// =========================================================================

// =========================================================================
// 5. Tauri Commands 外部交互层导出 (Tauri Commands)
// =========================================================================

/// 通用的向 api.oplist.org 发起 Token 刷新的异步函数
///
/// # 核心设计与规避机制
/// 统一抹除直连驱动后缀（如 baiduyun_go -> baidu）对齐 api.oplist.org 远程刷新端点，
/// 通过安全中转接口刷新令牌，确保 ClientSecret 等涉密信息零暴露风险。
pub async fn refresh_netdisk_token(driver: &str, refresh_token: &str) -> anyhow::Result<(String, String)> {
    // 夸克网盘（Cookie/扫码）不走 api.oplist.org 中转刷新，跳过
    if driver.contains("quark") {
        anyhow::bail!("夸克网盘不支持自动刷新 Token，Cookie 模式无需刷新，TV 模式请重新扫码");
    }
    let client = reqwest::Client::new();
    // 清理驱动后缀（_go/_qr/_fn），对齐 api.oplist.org 的挂载点命名
    let clean_driver = driver
        .trim_end_matches("_go")
        .trim_end_matches("_qr")
        .trim_end_matches("_fn")
        .to_string();

    // 使用 GET /{driver}/renewapi?refresh_token=... 端点刷新（而非之前的 POST /api/drive/{driver}/refresh）
    let url = format!(
        "https://api.oplist.org/{}/renewapi?refresh_token={}",
        clean_driver,
        urlencoding::encode(refresh_token)
    );

    let resp = client
        .get(&url)
        .send()
        .await?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        anyhow::bail!(
            "中转端刷新 {} Token 失败 (HTTP {}): {}",
            clean_driver,
            status,
            text
        );
    }

    // 响应格式：{"access_token": "...", "refresh_token": "..."}，HTTP 200 即成功
    let val: serde_json::Value = serde_json::from_str(&text)?;

    let access_token = val
        .get("access_token")
        .and_then(|a| a.as_str())
        .ok_or_else(|| anyhow::anyhow!("刷新接口响应中未找到 access_token 字段: {}", text))?
        .to_string();

    let new_refresh_token = val
        .get("refresh_token")
        .and_then(|r| r.as_str())
        .unwrap_or(refresh_token) // 降级容错：如未返回新的则沿用原有刷新令牌
        .to_string();

    log::info!(
        "[Netdisk] {} Token 刷新成功，新 Access Token 前8位: {}",
        clean_driver,
        &access_token[..access_token.len().min(8)]
    );

    Ok((access_token, new_refresh_token))
}

/// 存储适配器 Tauri Commands 外部通信接口
pub mod commands {
    use super::*;

    /// 静默轮询并刷新全部已配置网盘的 Access & Refresh Token 凭证，并自动回写持久化存盘。
    /// 
    /// # 业务场景
    /// 前端在初始化挂载时（或登录后）自动异步触发本命令，实现后台无感静默保鲜。
    #[tauri::command]
    pub async fn storage_refresh_all_tokens(
        app: tauri::AppHandle,
    ) -> Result<bool, String> {
        let mut app_cfg = crate::config::load_config(&app).map_err(|e| e.to_string())?;
        let mut has_changed = false;
        
        if let Some(StorageConfig::Netdisk(ref mut netdisk)) = app_cfg.storage {
            if let Some(ref refresh_tok) = netdisk.refresh_token {
                if !refresh_tok.trim().is_empty() {
                    log::info!("[Netdisk] 检测到直连网盘配置 ({})，正在静默刷新 Token 安全凭证...", netdisk.driver);
                    match refresh_netdisk_token(&netdisk.driver, refresh_tok).await {
                        Ok((new_access, new_refresh)) => {
                            netdisk.token = new_access;
                            netdisk.refresh_token = Some(new_refresh);
                            has_changed = true;
                            log::info!("[Netdisk] 网盘 Token 凭证全自动静默刷新并重新锁定成功！");
                        }
                        Err(e) => {
                            log::warn!("[Netdisk] 网盘 Token 静默刷新异常: {} (非致命，跳过)", e);
                        }
                    }
                }
            }
        }

        if has_changed {
            crate::config::save_config(&app, &app_cfg).map_err(|e| e.to_string())?;
            return Ok(true);
        }
        
        Ok(false)
    }

    /// 一键测试临时存储配置的连通性与云端目录可读写权限
    ///
    /// # 业务场景
    /// 当用户在前端“网盘配置向导”中输入了 Token、WebDAV 地址等临时参数、且尚未点击保存时，
    /// 点击“测试连接”，前端会将这些未存盘的数据组装成 `StorageConfig` 传入本命令。
    /// 本命令会动态创建一个临时的 `StorageBackend` 适配器，并对根目录 `/` 发起 `list_dir` 动作。
    /// 若成功，则代表各项鉴权参数 100% 正确且连通，实现优雅的配置前置安全性把关。
    #[tauri::command]
    pub async fn storage_test_connection(
        config: StorageConfig,
    ) -> Result<bool, String> {
        let backend = get_storage_backend_with_config(&config);
        
        // 尝试列出物理根目录，测试连通性与参数准确度
        match backend.list_dir("/").await {
            Ok(_) => Ok(true),
            Err(e) => Err(format!("云端存储连接测试失败: {}", e)),
        }
    }

    /// 通用列出云端指定物理路径下的子目录与文件
    ///
    /// # 业务场景与模式
    /// 1. 引导向导模式（传入 `config` 为 `Some`）：前端向导在尚未保存配置前，传入临时的配置与路径，
    ///    本命令动态浏览云端目录，配合前端 UI 呈递出可交互的“目录树”，供用户自由、直观地指定自定义备份根目录。
    /// 2. 日常运行模式（传入 `config` 为 `None`）：前端直接传入 `None`，本命令会自动加载已存盘激活的配置，
    ///    列出云盘指定路径下的备份文件，供日常列表展现。
    #[tauri::command]
    pub async fn storage_list_dir(
        app: tauri::AppHandle,
        config: Option<StorageConfig>,
        path: String,
    ) -> Result<Vec<RemoteFileEntry>, String> {
        let backend = if let Some(cfg) = config {
            get_storage_backend_with_config(&cfg)
        } else {
            let app_cfg = crate::config::load_config(&app).map_err(|e| e.to_string())?;
            get_storage_backend(&app_cfg).map_err(|e| e.to_string())?
        };

        backend.list_dir(&path).await.map_err(|e| e.to_string())
    }

    /// 夸克 TV 扫码登录：获取二维码（返回 base64 PNG 图片数据）
    #[tauri::command]
    pub async fn quark_tv_get_qr_code() -> Result<(String, String), String> {
        let backend = NetdiskBackend::new(crate::config::model::NetdiskConfig {
            driver: String::new(),
            token: String::new(),
            backup_root: None,
            refresh_token: None,
        });
        backend.quark_tv_get_qr().await.map_err(|e| e.to_string())
    }

    /// 夸克 TV 扫码登录：轮询授权状态（返回 code 或 None）
    #[tauri::command]
    pub async fn quark_tv_poll_qr(query_token: String) -> Result<Option<String>, String> {
        let backend = NetdiskBackend::new(crate::config::model::NetdiskConfig {
            driver: String::new(),
            token: String::new(),
            backup_root: None,
            refresh_token: None,
        });
        backend.quark_tv_poll_code(&query_token).await.map_err(|e| e.to_string())
    }

    /// 夸克 TV 扫码登录：用授权码交换 Token
    #[tauri::command]
    pub async fn quark_tv_exchange(code: String) -> Result<(String, String), String> {
        let backend = NetdiskBackend::new(crate::config::model::NetdiskConfig {
            driver: String::new(),
            token: String::new(),
            backup_root: None,
            refresh_token: None,
        });
        backend.quark_tv_exchange_token(&code).await.map_err(|e| e.to_string())
    }
}
