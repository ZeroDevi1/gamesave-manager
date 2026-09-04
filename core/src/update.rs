// update.rs - 对照 GitHub Releases 检查应用更新
use serde::{Deserialize, Serialize};

const GITHUB_REPO: &str = "ZeroDevi1/gamesave-manager";
const GITHUB_RELEASES_API: &str =
    "https://api.github.com/repos/ZeroDevi1/gamesave-manager/releases/latest";
const GITHUB_TAGS_API: &str = "https://api.github.com/repos/ZeroDevi1/gamesave-manager/tags";
const GITHUB_RELEASES_PAGE: &str = "https://github.com/ZeroDevi1/gamesave-manager/releases";
const GITHUB_REPO_PAGE: &str = "https://github.com/ZeroDevi1/gamesave-manager";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub has_update: bool,
    pub release_url: String,
    pub release_notes: Option<String>,
    pub published_at: Option<String>,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: Option<String>,
    body: Option<String>,
    published_at: Option<String>,
    draft: Option<bool>,
    prerelease: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct GithubTag {
    name: String,
}

fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 将 `v1.2.3` / `1.2.3-beta` 解析为可比较的三元组。
fn version_tuple(s: &str) -> [u64; 3] {
    let cleaned = s.trim().trim_start_matches(['v', 'V']);
    let mut out = [0u64; 3];
    for (i, part) in cleaned
        .split(|c: char| c == '.' || c == '-' || c == '+')
        .take(3)
        .enumerate()
    {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        out[i] = digits.parse().unwrap_or(0);
    }
    out
}

fn is_newer(latest: &str, current: &str) -> bool {
    version_tuple(latest) > version_tuple(current)
}

fn http_client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(format!("GameSave-Manager/{}", current_version()))
        .build()?)
}

async fn fetch_latest_release(
    client: &reqwest::Client,
) -> anyhow::Result<Option<GithubRelease>> {
    let resp = client
        .get(GITHUB_RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("GitHub Releases 请求失败 (HTTP {}): {}", status, body);
    }
    let release: GithubRelease = resp.json().await?;
    if release.draft.unwrap_or(false) {
        return Ok(None);
    }
    Ok(Some(release))
}

async fn fetch_latest_tag(client: &reqwest::Client) -> anyhow::Result<Option<String>> {
    let resp = client
        .get(GITHUB_TAGS_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("GitHub Tags 请求失败 (HTTP {}): {}", status, body);
    }
    let tags: Vec<GithubTag> = resp.json().await?;
    Ok(tags.into_iter().next().map(|t| t.name))
}

pub async fn check_for_updates() -> anyhow::Result<UpdateCheckResult> {
    let current = current_version().to_string();
    let client = http_client()?;

    match fetch_latest_release(&client).await {
        Ok(Some(release)) => {
            let latest = release.tag_name.trim().to_string();
            let has_update = is_newer(&latest, &current) && !release.prerelease.unwrap_or(false);
            let url = release
                .html_url
                .unwrap_or_else(|| GITHUB_RELEASES_PAGE.to_string());
            let message = if has_update {
                format!("发现新版本 {}（当前 {}）", latest, current)
            } else {
                format!("已是最新版本 {}", current)
            };
            return Ok(UpdateCheckResult {
                current_version: current,
                latest_version: Some(latest),
                has_update,
                release_url: url,
                release_notes: release.body.filter(|s| !s.trim().is_empty()),
                published_at: release.published_at,
                message,
            });
        }
        Ok(None) => {}
        Err(e) => {
            log::warn!("[update] 读取 GitHub Release 失败，回退到 tags: {}", e);
        }
    }

    match fetch_latest_tag(&client).await {
        Ok(Some(tag)) => {
            let has_update = is_newer(&tag, &current);
            let message = if has_update {
                format!("发现新版本 {}（当前 {}）", tag, current)
            } else {
                format!("已是最新版本 {}", current)
            };
            Ok(UpdateCheckResult {
                current_version: current,
                latest_version: Some(tag),
                has_update,
                release_url: GITHUB_RELEASES_PAGE.to_string(),
                release_notes: None,
                published_at: None,
                message,
            })
        }
        Ok(None) => Ok(UpdateCheckResult {
            current_version: current.clone(),
            latest_version: None,
            has_update: false,
            release_url: GITHUB_REPO_PAGE.to_string(),
            release_notes: None,
            published_at: None,
            message: format!(
                "当前版本 {}。仓库 {} 尚未发布 Release，可稍后在 GitHub 查看。",
                current, GITHUB_REPO
            ),
        }),
        Err(e) => anyhow::bail!("检查更新失败: {}", e),
    }
}

fn is_allowed_update_url(url: &str) -> bool {
    url.starts_with("https://github.com/ZeroDevi1/gamesave-manager")
}

pub fn open_release_page(url: &str) -> anyhow::Result<()> {
    let target = if url.trim().is_empty() || !is_allowed_update_url(url) {
        GITHUB_RELEASES_PAGE.to_string()
    } else {
        url.trim().to_string()
    };

    #[cfg(target_os = "windows")]
    let spawn_result = std::process::Command::new("cmd")
        .args(["/C", "start", "", &target])
        .spawn();

    #[cfg(target_os = "macos")]
    let spawn_result = std::process::Command::new("open").arg(&target).spawn();

    #[cfg(all(unix, not(target_os = "macos")))]
    let spawn_result = std::process::Command::new("xdg-open").arg(&target).spawn();

    spawn_result
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("无法打开浏览器: {}", e))
}

pub mod commands {
    use super::*;

    #[tauri::command]
    pub fn get_app_version() -> String {
        current_version().to_string()
    }

    #[tauri::command]
    pub async fn check_for_updates() -> Result<UpdateCheckResult, String> {
        super::check_for_updates().await.map_err(|e| e.to_string())
    }

    #[tauri::command]
    pub fn open_release_page(url: String) -> Result<(), String> {
        super::open_release_page(&url).map_err(|e| e.to_string())
    }
}
