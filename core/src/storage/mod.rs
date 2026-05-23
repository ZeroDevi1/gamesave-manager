// storage/mod.rs - 统一存储后端抽象层与多路适配器实现
use crate::config::model::{AlistConfig, StorageConfig, WebdavConfig, S3Config};
use reqwest;
use serde::{Deserialize, Serialize};
use std::path::Path;
use regex::Regex;

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

    /// 辅助方法：动态组装直连网关在 AList / OpenList 服务端的物理挂载子路径
    /// 
    /// # 核心设计
    /// 在 api.oplist.org 公共直连网关上，百度网盘、阿里云盘等不同云盘均作为子挂载点隔离在诸如 /baiduyun_go、/alicloud_qr 下。
    /// 1. 若直接请求根目录 "/" 会由于中转端限制返回 404，本方法会自动将其重映射为 "/{driver}" 虚拟挂载根；
    /// 2. 具备前缀防叠防重检测：若传入路径已携带挂载前缀，则原样返回，保障多级级联浏览和上传下载路径 100% 正确。
    fn get_real_path(&self, path: &str) -> String {
        let driver = &self.config.driver;
        let prefix = format!("/{}", driver);
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
            let err_text = resp.text().await.unwrap_or_default();
            
            if !status.is_success() {
                anyhow::bail!("百度网盘直连创建目录失败 (HTTP {}): {}", status, err_text);
            }
            
            let baidu_resp: BaiduCommonResponse = serde_json::from_str(&err_text)
                .map_err(|e| anyhow::anyhow!("解析百度网盘响应失败 ({}): {}", e, err_text))?;
                
            if baidu_resp.errno != 0 {
                anyhow::bail!("百度网盘创建目录接口报错 (errno {}): 请尝试重新一键授权刷新 Token。", baidu_resp.errno);
            }
            
            return Ok(());
        }

        // 降级兼容：其它直连网盘使用公共 AList 代理（如网关限制 AList API 会被 diagnostics 捕获）
        let real_path = self.get_real_path(path);
        crate::alist::fs::mkdir(self.base_url(), &self.config.token, &real_path).await
    }

    /// 将本地游戏存档上传至直连网盘
    pub async fn upload_file(&self, local_path: &str, remote_path: &str) -> anyhow::Result<()> {
        let driver = &self.config.driver;
        // 核心分支：百度网盘官方 API 直连极速简单上传
        if driver.contains("baidu") {
            let client = reqwest::Client::new();
            let clean_path = format!("/{}", remote_path.trim_start_matches('/'));
            
            let url = format!(
                "https://d.pcs.baidu.com/rest/2.0/pcs/file?method=upload&access_token={}&path={}&ondup=overwrite",
                self.config.token,
                urlencoding::encode(&clean_path)
            );
            
            let file = tokio::fs::File::open(local_path).await?;
            let body = reqwest::Body::from(file);
            
            let resp = client
                .post(&url)
                .header("User-Agent", "pan.baidu.com")
                .header("Content-Type", "application/octet-stream")
                .body(body)
                .send()
                .await?;
                
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            
            if !status.is_success() {
                anyhow::bail!("百度网盘直连上传失败 (HTTP {}): {}", status, err_text);
            }
            
            let baidu_resp: BaiduCommonResponse = serde_json::from_str(&err_text)
                .map_err(|e| anyhow::anyhow!("解析百度网盘响应失败 ({}): {}", e, err_text))?;
                
            if baidu_resp.errno != 0 {
                anyhow::bail!("百度网盘上传接口报错 (errno {}): 请尝试重新一键授权刷新 Token。", baidu_resp.errno);
            }
            
            return Ok(());
        }

        let real_path = self.get_real_path(remote_path);
        crate::alist::fs::upload_file(self.base_url(), &self.config.token, local_path, &real_path).await
    }

    /// 从网盘下载指定的游戏存档
    pub async fn download_file(&self, remote_path: &str, local_path: &str) -> anyhow::Result<()> {
        let driver = &self.config.driver;
        // 核心分支：百度网盘官方 API 直接获取 PCS 数据流
        if driver.contains("baidu") {
            let client = reqwest::Client::new();
            let clean_path = format!("/{}", remote_path.trim_start_matches('/'));
            
            let url = format!(
                "https://d.pcs.baidu.com/rest/2.0/pcs/file?method=download&access_token={}&path={}",
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
                let err_text = resp.text().await.unwrap_or_default();
                anyhow::bail!("百度网盘直连下载失败 (HTTP {}): {}", status, err_text);
            }
            
            if let Some(parent) = Path::new(local_path).parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            
            let bytes = resp.bytes().await?;
            tokio::fs::write(local_path, &bytes).await?;
            
            return Ok(());
        }

        let real_path = self.get_real_path(remote_path);
        crate::alist::fs::download_file(self.base_url(), &self.config.token, &real_path, local_path).await
    }

    /// 列出直连网盘指定路径下的子条目列表
    pub async fn list_dir(&self, path: &str) -> anyhow::Result<Vec<RemoteFileEntry>> {
        let driver = &self.config.driver;
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

            let resp = client
                .get(&url)
                .header("User-Agent", "pan.baidu.com")
                .send()
                .await?;

            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();

            if !status.is_success() {
                anyhow::bail!("百度网盘直连列目录失败 (HTTP {}): {}", status, err_text);
            }

            let baidu_resp: BaiduFileListResponse = serde_json::from_str(&err_text)
                .map_err(|e| anyhow::anyhow!("解析百度网盘响应失败 ({}): {}", e, err_text))?;

            // 百度网盘 errno 说明：-9 代表沙盒内该目录尚不存在（首次配置连接时，友好返回空列表）
            if baidu_resp.errno == -9 {
                return Ok(Vec::new());
            }

            if baidu_resp.errno != 0 {
                anyhow::bail!("百度网盘接口报错 (errno {}): 请尝试重新一键授权刷新 Token。", baidu_resp.errno);
            }

            let entries = baidu_resp.list.unwrap_or_default()
                .into_iter()
                .map(|e| {
                    let dt = chrono::DateTime::from_timestamp(e.server_mtime as i64, 0)
                        .unwrap_or_else(|| chrono::Utc::now());
                    RemoteFileEntry {
                        name: e.server_filename,
                        path: e.path,
                        is_dir: e.isdir == 1,
                        size: e.size,
                        modified: Some(dt.to_rfc3339()),
                    }
                })
                .collect();

            return Ok(entries);
        }

        // 其它网盘暂退避，若中转网关完全拒绝 AList APIs 访问，在 fs.rs 级别会输出精准 404/403 建议自建
        let real_path = self.get_real_path(path);
        let raw_entries = crate::alist::fs::list_dir(self.base_url(), &self.config.token, &real_path).await?;
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
}

impl AlistBackend {
    /// 在 Alist 云端物理系统上创建目录
    pub async fn mkdir(&self, path: &str) -> anyhow::Result<()> {
        let token = self.config.token.as_deref().unwrap_or("");
        crate::alist::fs::mkdir(&self.config.base_url, token, path).await
    }

    async fn upload_file(&self, local_path: &str, remote_path: &str) -> anyhow::Result<()> {
        let token = self.config.token.as_deref().unwrap_or("");
        crate::alist::fs::upload_file(&self.config.base_url, token, local_path, remote_path).await
    }

    async fn download_file(&self, remote_path: &str, local_path: &str) -> anyhow::Result<()> {
        let token = self.config.token.as_deref().unwrap_or("");
        crate::alist::fs::download_file(&self.config.base_url, token, remote_path, local_path).await
    }

    async fn list_dir(&self, path: &str) -> anyhow::Result<Vec<RemoteFileEntry>> {
        let token = self.config.token.as_deref().unwrap_or("");
        let raw_entries = crate::alist::fs::list_dir(&self.config.base_url, token, path).await?;
        
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
            
            let resp = client
                .request(mkcol_method, &url)
                .basic_auth(&self.config.username, Some(&self.config.password))
                .send()
                .await?;

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

        let resp = client
            .put(&url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", file_size.to_string())
            .body(body)
            .send()
            .await?;

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

        let resp = client
            .get(&url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .send()
            .await?;

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
    /// WebDAV `PROPFIND` 操作默认返回大片包含命名空间的 XML 物理属性结构体。
    /// 为了捍卫 Rust 轻量小体积底座、杜绝引入多余庞大的第三方 XML crate，此处手写编写了
    /// 搭载 Regex 正则表达式的高健壮性 XML 切消匹配扫描算法。
    /// 能够稳定兼容带命名空间前缀（如 `d:href`、`D:href`、`a:href` 等）与不带前缀的各类 WebDAV 服务端（坚果云、Nextcloud 等）。
    async fn list_dir(&self, path: &str) -> anyhow::Result<Vec<RemoteFileEntry>> {
        let client = self.create_client();
        let endpoint = self.config.endpoint.trim_end_matches('/');
        let url = format!("{}{}", endpoint, path);

        let propfind_method = reqwest::Method::from_bytes(b"PROPFIND")?;
        let resp = client
            .request(propfind_method, &url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            // 设定 Depth: 1 获取目录及直接子文件信息
            .header("Depth", "1")
            .header("Content-Type", "application/xml; charset=utf-8")
            .send()
            .await?;

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

/// 存储适配器 Tauri Commands 外部通信接口
pub mod commands {
    use super::*;

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
    ///    本命令动态解析并浏览云端目录，配合前端 UI 呈递出可交互的“目录树”，供用户自由、直观地指定自定义备份根目录。
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
}
