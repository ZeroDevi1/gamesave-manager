// alist/auth.rs - Alist 登录与 JWT 管理
use super::types::{AlistApiResponse, LoginRequest, LoginResponse};
use reqwest;

/// Alist 登录：先尝试 /api/auth/login/hash（MD5），失败后 fallback 到明文 /api/auth/login
pub async fn login(url: &str, username: &str, password: &str) -> anyhow::Result<LoginResponse> {
    let base = url.trim_end_matches('/');
    let client = reqwest::Client::new();

    // 1. 先尝试 login/hash（MD5 哈希密码）
    let password_hash = format!("{:x}", md5::compute(password));
    eprintln!("[Alist login] username={} hash={}", username, password_hash);

    let req_hash = LoginRequest {
        username: username.to_string(),
        password: password_hash,
    };
    let body_hash = serde_json::to_string(&req_hash)?;
    eprintln!("[Alist login/hash] POST {}/api/auth/login/hash", base);
    eprintln!("[Alist login/hash] body: {}", body_hash);

    let resp = client
        .post(format!("{}/api/auth/login/hash", base))
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Accept", "application/json")
        .body(body_hash)
        .send()
        .await?;

    let status = resp.status();
    let text = resp.text().await?;
    eprintln!("[Alist login/hash] status={} body={}", status, text);

    let api_resp: AlistApiResponse<serde_json::Value> = serde_json::from_str(&text)
        .unwrap_or(AlistApiResponse { code: -1, message: text.clone(), data: None });

    if api_resp.code == 200 {
        let token = extract_token(api_resp)?;
        eprintln!("[Alist login/hash] 成功");
        return Ok(LoginResponse { token });
    }

    eprintln!(
        "[Alist login/hash] 失败: code={} msg={}, 尝试 fallback",
        api_resp.code,
        api_resp.message
    );

    // 2. fallback：尝试 /api/auth/login（明文密码，兼容部分 OpenList / 旧版 Alist）
    let req_plain = LoginRequest {
        username: username.to_string(),
        password: password.to_string(),
    };
    let body_plain = serde_json::to_string(&req_plain)?;
    eprintln!("[Alist login] POST {}/api/auth/login", base);
    eprintln!("[Alist login] body: {}", body_plain);

    let resp_plain = client
        .post(format!("{}/api/auth/login", base))
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Accept", "application/json")
        .body(body_plain)
        .send()
        .await?;

    let status_plain = resp_plain.status();
    let text_plain = resp_plain.text().await?;
    eprintln!("[Alist login] status={} body={}", status_plain, text_plain);

    let api_resp_plain: AlistApiResponse<serde_json::Value> = serde_json::from_str(&text_plain)
        .unwrap_or(AlistApiResponse { code: -1, message: text_plain.clone(), data: None });

    if api_resp_plain.code == 200 {
        let token = extract_token(api_resp_plain)?;
        eprintln!("[Alist login] 成功");
        return Ok(LoginResponse { token });
    }

    anyhow::bail!(
        "login/hash: {} | login: {}",
        api_resp.message,
        api_resp_plain.message
    )
}

fn extract_token(api_resp: AlistApiResponse<serde_json::Value>) -> anyhow::Result<String> {
    api_resp
        .data
        .and_then(|d| d.get("token").and_then(|t| t.as_str().map(|s| s.to_string())))
        .ok_or_else(|| anyhow::anyhow!("响应中未找到 token"))
}
