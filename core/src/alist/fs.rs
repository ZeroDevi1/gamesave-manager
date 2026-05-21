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

    let api_resp: AlistApiResponse<serde_json::Value> = resp.json().await?;
    if api_resp.code != 200 {
        anyhow::bail!("列出目录失败: {}", api_resp.message);
    }

    let content = api_resp
        .data
        .and_then(|d| d.get("content").cloned())
        .ok_or_else(|| anyhow::anyhow!("响应中未找到 content"))?;

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

/// 流式上传文件到 Alist
pub async fn upload_file(url: &str, token: &str, local_path: &str, remote_path: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let file = tokio::fs::File::open(local_path).await?;
    let stream = tokio_util::io::ReaderStream::new(file);
    let body = reqwest::Body::wrap_stream(stream);

    let file_name = Path::new(local_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("upload");

    let resp = client
        .put(format!(
            "{}/api/fs/put",
            url.trim_end_matches('/')
        ))
        .header("Authorization", token)
        .header("File-Path", remote_path)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Disposition", format!("attachment; filename=\"{}\"", file_name))
        .body(body)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("上传失败: HTTP {}", resp.status());
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

    if !resp.status().is_success() {
        anyhow::bail!("创建目录失败: HTTP {}", resp.status());
    }

    Ok(())
}
