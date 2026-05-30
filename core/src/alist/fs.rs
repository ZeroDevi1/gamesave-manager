// alist/fs.rs - Alist 文件系统操作（目录列表、上传、删除、创建目录）
use super::types::{AlistApiResponse, FileEntry, ListDirRequest, MkdirRequest};
use reqwest;
use std::path::Path;

/// 列出 Alist 目录内容
pub async fn list_dir(url: &str, token: &str, path: &str) -> anyhow::Result<Vec<FileEntry>> {
    let client = reqwest::Client::new();
    let req = ListDirRequest {
        path: path.to_string(),
        password: "".to_string(),
        page: 1,
        per_page: 0,
        refresh: false,
    };

    let resp = client
        .post(format!("{}/api/fs/list", url.trim_end_matches('/')))
        .header("Authorization", token)
        .json(&req)
        .send()
        .await?;

    let status = resp.status();
    let err_text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        anyhow::bail!(
            "中转网关响应非预期 (HTTP {}): {}。请检查您的网盘授权是否被限制，或 Token 是否已失效。",
            status,
            if err_text.len() > 150 { format!("{}...", &err_text[..150]) } else { err_text }
        );
    }

    let api_resp: AlistApiResponse<serde_json::Value> = serde_json::from_str(&err_text)
        .map_err(|e| anyhow::anyhow!(
            "解析中转网关响应失败 ({}): {}",
            e,
            if err_text.len() > 150 { format!("{}...", &err_text[..150]) } else { err_text }
        ))?;
    if api_resp.code != 200 {
        anyhow::bail!("列出目录失败: {}", api_resp.message);
    }

    let content = api_resp
        .data
        .and_then(|d| d.get("content").cloned())
        .ok_or_else(|| anyhow::anyhow!("响应中未找到 content"))?;

    // 核心兼容性容错：若目标文件夹下没有任何子文件或文件夹，Alist/OpenList 会在 content 中返回 null。
    // 如果对 null 进行反序列化则会引发 "invalid type: null, expected a sequence" 的解析崩溃。
    // 这里通过防御性拦截 content.is_null()，在空目录下直接安全返回空列表。
    if content.is_null() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<FileEntry> = serde_json::from_value(content)?;

    // 如果 Alist 返回的条目缺少 path 字段，则根据当前目录路径补全
    for entry in &mut entries {
        if entry.path.is_empty() {
            let separator = if path.ends_with('/') || path == "/" { "" } else { "/" };
            entry.path = format!("{}{}{}", path.trim_end_matches('/'), separator, entry.name);
        }
    }

    Ok(entries)
}

/// 上传文件到 Alist/OpenList 文件系统
///
/// # 参数说明
/// * `url` - Alist/OpenList 的服务端基础 URL (例如 `http://localhost:5244`)
/// * `token` - 登录授权获取到的 Bearer Token
/// * `local_path` - 本地待上传文件的绝对物理路径
/// * `remote_path` - 上传至 Alist/OpenList 的目标网盘绝对路径 (例如 `/baidu/backups/save.zip`)
///
/// # 核心设计与规避机制
/// 1. **空文件容错**：由于百度网盘等大多数云盘 API 的设计缺陷，它们不支持上传 0 字节的空文件。为了防止整个
///    游戏存档备份链条因为一个占位的空文件（如元数据空标记）而意外中断，当检测到本地文件大小为 0 时，我们将其
///    特殊处理为包含一个 `\0` 字节（大小为 1）的内容进行上传，实现优雅的降级容错。
/// 2. **显式 Content-Length**：避免使用分块流传输（Chunked Transfer Encoding）。如果使用未知长度的流式上传，
///    reqwest 会默认使用 `Transfer-Encoding: chunked`，这会导致 AList 无法提前获取文件大小，进而导致百度网盘
///    驱动无法调用预上传接口（`/api/precreate` 需要 `size`）而报错。这里通过显式读取文件大小并调用 `reqwest::Body::from(file)`，
///    配合显式设置 `Content-Length` 请求头，彻底解决大文件上传在百度网盘等驱动下报错的问题。
/// 3. **详尽错误诊断**：如果 HTTP 请求失败，我们不再仅仅抛出状态码，而是尝试解析 AList 返回 of JSON 报文中的
///    `message` 字段，把例如“百度网盘 31299 参数错误”或“无写权限”等深层网盘报错信息提取出来，提供直观的排查指引。
pub async fn upload_file(url: &str, token: &str, local_path: &str, remote_path: &str) -> anyhow::Result<()> {
    // 初始化 HTTP 请求客户端
    let client = reqwest::Client::new();
    
    // 获取待上传文件的物理元数据以查询大小
    let metadata = tokio::fs::metadata(local_path).await?;
    let mut file_size = metadata.len();

    // 针对 0 字节文件实施百度网盘等云盘的空文件兼容性规避策略
    let body = if file_size == 0 {
        file_size = 1;
        reqwest::Body::from(vec![0u8]) // 写入 1 字节空字符进行规避
    } else {
        let file = tokio::fs::File::open(local_path).await?;
        reqwest::Body::from(file)
    };

    // 提取本地文件名作为附件名
    let file_name = Path::new(local_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("upload");

    // 构造请求并发送
    let resp = client
        .put(format!(
            "{}/api/fs/put",
            url.trim_end_matches('/')
        ))
        .header("Authorization", token)
        .header("File-Path", remote_path)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Length", file_size.to_string()) // 显式传递文件大小，解决百度网盘预上传 size 缺失报错
        .header("Content-Disposition", format!("attachment; filename=\"{}\"", file_name))
        .body(body)
        .send()
        .await?;

    // 先获取 HTTP 状态码（status 实现了 Copy 属性，可有效防范 ownership 转移导致后续 borrow 崩溃）
    let status = resp.status();
    let err_text = resp.text().await.unwrap_or_default();
    
    if let Ok(api_resp) = serde_json::from_str::<AlistApiResponse<serde_json::Value>>(&err_text) {
        if api_resp.code != 200 {
            anyhow::bail!("上传失败 (业务代码 {}): {}", api_resp.code, api_resp.message);
        }
    } else {
        // 兼容非 JSON 报文的网卡或代理层异常（如 413 Payload Too Large 等）
        if !status.is_success() {
            anyhow::bail!("上传失败 (HTTP {}): {}", status, err_text);
        }
    }

    Ok(())
}

/// 在 Alist 创建目录
pub async fn mkdir(url: &str, token: &str, path: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let req = MkdirRequest {
        path: path.to_string(),
    };

    let resp = client
        .post(format!("{}/api/fs/mkdir", url.trim_end_matches('/')))
        .header("Authorization", token)
        .json(&req)
        .send()
        .await?;

    // 同样，先提取状态码防止所有权转移
    let status = resp.status();
    let err_text = resp.text().await.unwrap_or_default();
    
    if let Ok(api_resp) = serde_json::from_str::<AlistApiResponse<serde_json::Value>>(&err_text) {
        if api_resp.code != 200 {
            anyhow::bail!("创建目录失败 (业务代码 {}): {}", api_resp.code, api_resp.message);
        }
    } else {
        if !status.is_success() {
            anyhow::bail!("创建目录失败 (HTTP {}): {}", status, err_text);
        }
    }

    Ok(())
}
/// 从 Alist 下载文件到本地路径
pub async fn download_file(url: &str, token: &str, remote_path: &str, local_path: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let download_url = format!(
        "{}/d{}",
        url.trim_end_matches('/'),
        remote_path
    );
    let resp = client
        .get(&download_url)
        .header("Authorization", token)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("下载备份文件失败: HTTP {}", resp.status());
    }

    let bytes = resp.bytes().await?;
    if let Some(parent) = Path::new(local_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(local_path, &bytes)?;
    Ok(())
}
/// 从 Alist 删除文件或目录
///
/// # 参数说明
/// * `url` - Alist/OpenList 的服务端基础 URL
/// * `token` - 登录授权获取到的 Bearer Token
/// * `remote_path` - 待删除的网盘绝对路径
pub async fn remove(url: &str, token: &str, remote_path: &str) -> anyhow::Result<()> {
    let path = Path::new(remote_path);
    let dir = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "/".to_string());
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| anyhow::anyhow!("无法从路径提取文件名: {}", remote_path))?;
    let client = reqwest::Client::new();
    let req = super::types::RemoveRequest {
        dir,
        names: vec![name],
    };
    let resp = client
        .post(format!("{}/api/fs/remove", url.trim_end_matches('/')))
        .header("Authorization", token)
        .json(&req)
        .send()
        .await?;
    let status = resp.status();
    let err_text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("删除失败 (HTTP {}): {}", status, err_text);
    }
    let api_resp: super::types::AlistApiResponse<serde_json::Value> = serde_json::from_str(&err_text)
        .map_err(|e| anyhow::anyhow!(
            "解析删除响应失败 ({}): {}",
            e,
            err_text
        ))?;
    if api_resp.code != 200 {
        anyhow::bail!("删除失败: {}", api_resp.message);
    }
    Ok(())
}
