// utils/http.rs - 带鉴权头的流式下载（夸克/OpenList 直链跨域重定向时保留 Cookie）
use anyhow::Context;
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, COOKIE};
use reqwest::redirect::Policy;
use reqwest::Url;
use std::path::Path;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const MAX_REDIRECTS: usize = 8;

/// 将 OpenList `/api/fs/link` 返回的 `header` 字段解析为请求头。
///
/// 兼容三种形态：JSON 对象（值为字符串或字符串数组）、以及 `Cookie: ...\nUser-Agent: ...` 文本。
pub fn parse_header_map(value: &serde_json::Value) -> HeaderMap {
    let mut map = HeaderMap::new();
    match value {
        serde_json::Value::Object(obj) => {
            for (k, v) in obj {
                let values: Vec<String> = match v {
                    serde_json::Value::String(s) => vec![s.clone()],
                    serde_json::Value::Array(arr) => arr
                        .iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect(),
                    _ => continue,
                };
                if let Ok(name) = HeaderName::from_bytes(k.as_bytes()) {
                    for val in values {
                        if let Ok(hv) = HeaderValue::from_str(&val) {
                            map.append(name.clone(), hv);
                        }
                    }
                }
            }
        }
        serde_json::Value::String(s) if !s.trim().is_empty() => {
            for line in s.lines() {
                if let Some((k, v)) = line.split_once(':') {
                    if let (Ok(name), Ok(val)) = (
                        HeaderName::from_bytes(k.trim().as_bytes()),
                        HeaderValue::from_str(v.trim()),
                    ) {
                        map.append(name, val);
                    }
                }
            }
        }
        _ => {}
    }
    map
}

fn host_of(url: &Url) -> String {
    url.host_str().unwrap_or("").to_lowercase()
}

fn is_quark_family(host: &str) -> bool {
    host == "quark.cn" || host.ends_with(".quark.cn")
}

/// 夸克直链会从 drive.quark.cn 302 到 dl-*.pds.quark.cn；reqwest 默认会剥掉跨域 Cookie，
/// 导致 PDS 回调 `require login [auth miss]`。同源或同属夸克下载域时必须继续携带敏感头。
fn should_forward_sensitive(from: &Url, to: &Url) -> bool {
    let a = host_of(from);
    let b = host_of(to);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    is_quark_family(&a) && is_quark_family(&b)
}

fn apply_headers(mut req: reqwest::RequestBuilder, headers: &HeaderMap) -> reqwest::RequestBuilder {
    for (k, v) in headers.iter() {
        req = req.header(k, v);
    }
    req
}

/// 跟随重定向并在夸克下载域之间保留 Cookie/UA/Referer，流式写入本地文件。
pub async fn download_url_to_file(
    url: &str,
    extra_headers: HeaderMap,
    local_path: &str,
) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(600))
        .build()?;

    let mut current = Url::parse(url).with_context(|| format!("无效下载地址: {}", url))?;
    let mut headers = extra_headers;
    let mut hops = 0usize;

    loop {
        hops += 1;
        if hops > MAX_REDIRECTS {
            anyhow::bail!("下载重定向次数过多: {}", current);
        }

        let resp = apply_headers(client.get(current.clone()), &headers)
            .send()
            .await
            .with_context(|| format!("请求下载地址失败: {}", current))?;
        let status = resp.status();

        if status.is_redirection() {
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| anyhow::anyhow!("下载重定向缺少 Location: HTTP {}", status))?;
            let next = current
                .join(location)
                .with_context(|| format!("无法解析重定向: {}", location))?;
            if next.scheme() != "http" && next.scheme() != "https" {
                anyhow::bail!("拒绝跟随非 HTTP 重定向: {}", next);
            }
            if !should_forward_sensitive(&current, &next) {
                headers.remove(COOKIE);
                headers.remove(AUTHORIZATION);
            }
            log::debug!("[download] {} → {} ({})", current, next, status);
            current = next;
            continue;
        }

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let preview = if body.len() > 800 {
                format!("{}...", &body[..800])
            } else {
                body
            };
            anyhow::bail!("下载失败 (HTTP {}): {}", status, preview);
        }

        if let Some(parent) = Path::new(local_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::File::create(local_path).await?;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.with_context(|| format!("读取下载流失败: {}", current))?;
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        return Ok(());
    }
}
