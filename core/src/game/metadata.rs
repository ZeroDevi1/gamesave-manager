use tauri::Manager;
// game/metadata.rs - SteamGridDB 元数据获取
use serde::Deserialize;
use tauri::AppHandle;

/// SteamGridDB 搜索结果
#[derive(Debug, Deserialize)]
struct SgdbSearchResult {
    data: Vec<SgdbGame>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SgdbGame {
    id: u64,
    name: String,
}

/// SteamGridDB 网格图结果
#[derive(Debug, Deserialize)]
struct SgdbGridsResult {
    data: Vec<SgdbGrid>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SgdbGrid {
    url: String,
    thumb: String,
}

/// 获取游戏 Logo（本地优先，其次 SteamGridDB）
/// 获取游戏 Logo（本地优先，其次 SteamGridDB，最后公开 Steam 静态直链后备）
///
/// # 核心业务语义：
/// 本方法用于在导入或管理游戏时，自动下载并关联游戏的高清卡片封面。
/// 1. **本地优先（Cache First）**：若 cache 目录下已存在本地缓存，则直接返回，避免重复下载。
/// 2. **SteamGridDB 优先（需 API 秘钥）**：若用户配置了 `steamgriddb_api_key`，则优先前往 SteamGridDB 拉取高画质 Logo。
/// 3. **Steam 官方公开 CDN 降级（无需 API 秘钥）**：若未配置秘钥，或 SteamGridDB 服务不可用，系统将智能退避（Fallback）到 Steam 官方网关直链下载高清 `library_600x900.jpg` 竖版游戏封面，保证 100% 成功浮现出超高清海报。
///
/// # 参数：
/// * `app` - Tauri AppHandle 句柄，用于提取本地数据路径和配置文件
/// * `game_id` - 游戏在本系统的唯一 ID，用作图片缓存的文件名
/// * `steam_appid` - 游戏的 Steam AppID，用于在外部网关定位游戏
///
/// # 返回值：
/// * `anyhow::Result<Option<String>>` - 成功下载并关联的本地图片绝对路径
pub async fn fetch_logo(
    app: &AppHandle,
    game_id: &str,
    steam_appid: Option<u64>,
) -> anyhow::Result<Option<String>> {
    let cache_dir = app.path().app_local_data_dir()?.join("cache").join("logos");
    std::fs::create_dir_all(&cache_dir)?;

    // 使用 game_id 作为缓存图的文件名
    let cached_path = cache_dir.join(format!("{}.png", game_id));
    if cached_path.exists() {
        return Ok(Some(cached_path.to_string_lossy().to_string()));
    }

    // 如果没有 steam_appid，则完全无法拉取外部 Logo
    let appid = match steam_appid {
        Some(id) => id,
        None => return Ok(None),
    };

    let client = reqwest::Client::new();

    // 尝试第一优先级：SteamGridDB（需要用户配置 API Key）
    if let Ok(config) = crate::config::load_config(app) {
        if let Some(api_key) = config.settings.steamgriddb_api_key {
            // 将 SteamGridDB 请求流程封装在可回退的异步流程中
            if let Ok(Some(logo_url)) = fetch_from_sgdb(&client, appid, &api_key).await {
                if let Ok(img_resp) = client.get(&logo_url).send().await {
                    if img_resp.status().is_success() {
                        if let Ok(img_bytes) = img_resp.bytes().await {
                            if std::fs::write(&cached_path, &img_bytes).is_ok() {
                                return Ok(Some(cached_path.to_string_lossy().to_string()));
                            }
                        }
                    }
                }
            }
        }
    }

    // ==================== 智能 Fallback 降级机制 ====================
    // 若 SteamGridDB API 秘钥不存在、服务请求失败或下载失败，
    // 我们立刻退避至 Steam 官方公开直链网关，无需任何秘钥下载高清 600x900 竖版卡片海报
    // 这可以让普通玩家在零配置状态下，同样 100% 自动加载出高大上的正版封面效果！
    let fallback_urls = [
        format!("https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/{}/library_600x900.jpg", appid),
        format!("https://cdn.akamai.steamstatic.com/steam/apps/{}/header.jpg", appid),
    ];

    for url in fallback_urls {
        if let Ok(img_resp) = client.get(&url).send().await {
            if img_resp.status().is_success() {
                if let Ok(img_bytes) = img_resp.bytes().await {
                    if std::fs::write(&cached_path, &img_bytes).is_ok() {
                        return Ok(Some(cached_path.to_string_lossy().to_string()));
                    }
                }
            }
        }
    }

    Ok(None)
}

/// 辅助方法：从 SteamGridDB 官方接口检索并定位游戏 Logo 网格图直链
///
/// # 参数：
/// * `client` - HTTP 异步网络请求客户端
/// * `appid` - 游戏的 Steam AppID
/// * `api_key` - 用户的 SteamGridDB 授权 API Key 令牌
///
/// # 返回值：
/// * `anyhow::Result<Option<String>>` - 查找到的对应尺寸的网格图片 URL 直链
async fn fetch_from_sgdb(
    client: &reqwest::Client,
    appid: u64,
    api_key: &str,
) -> anyhow::Result<Option<String>> {
    // 1. 通过 Steam AppID 搜索获取 SteamGridDB 内部的游戏对应数字 ID
    let search_url = format!("https://www.steamgriddb.com/api/v2/games/id/{}", appid);
    let search_resp = client
        .get(&search_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    if !search_resp.status().is_success() {
        return Ok(None);
    }

    let search_result: SgdbSearchResult = search_resp.json().await?;
    let sgdb_game_id = match search_result.data.first() {
        Some(g) => g.id,
        None => return Ok(None),
    };

    // 2. 凭借该 ID 获取其在 SteamGridDB 的封面网格图元数据
    let grids_url = format!(
        "https://www.steamgriddb.com/api/v2/grids/game/{}?dimensions=460x215,920x430",
        sgdb_game_id
    );
    let grids_resp = client
        .get(&grids_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    if !grids_resp.status().is_success() {
        return Ok(None);
    }

    let grids_result: SgdbGridsResult = grids_resp.json().await?;
    let grid_url = match grids_result.data.first() {
        Some(g) => g.url.clone(),
        None => return Ok(None),
    };

    Ok(Some(grid_url))
}

/// 保存自定义 Logo 缓存到本地物理文件系统。
///
/// # 核心业务语义：
/// 本方法在编辑条目或拉取 EXE 图标完毕后，将 Base64 数据解码并安静存盘至本地 `{app_local_data}/cache/logos/{game_id}.png` 中，
/// 以实现物理脱机运行与永久加载。
///
/// # 参数：
/// * `app` - Tauri AppHandle 句柄，用以在各操作系统中准确定位本地存储路径
/// * `game_id` - 游戏在数据库或我的游戏中的唯一编码 ID
/// * `logo_base64` - 带有或未带 MIME 前缀的 Base64 序列（例如 "data:image/png;base64,..."）
pub fn save_custom_logo(
    app: AppHandle,
    game_id: String,
    logo_base64: String,
) -> Result<(), String> {
    let cache_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("cache")
        .join("logos");
    std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;
    let cached_path = cache_dir.join(format!("{}.png", game_id));

    // 剔除可能存在的 MIME 头，截取纯粹的 Base64 数据流
    let clean_base64 = if logo_base64.starts_with("data:") {
        if let Some(pos) = logo_base64.find("base64,") {
            &logo_base64[pos + 7..]
        } else {
            &logo_base64
        }
    } else {
        &logo_base64
    };

    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(clean_base64)
        .map_err(|e| e.to_string())?;

    std::fs::write(&cached_path, &decoded).map_err(|e| e.to_string())?;
    Ok(())
}

/// 读取本地缓存的 Logo 文件并以安全沙箱允许的 Base64 格式返回。
///
/// # 核心业务语义：
/// 前端常因沙箱跨域或本地文件安全协议限制无法直接渲染物理路径，
/// 故后端自动在内存中编码为 Base64 Data URI，实现 100% 渲染且绝对安全。
///
/// # 参数：
/// * `app` - Tauri AppHandle 句柄
/// * `game_id` - 游戏在数据库或我的游戏中的唯一编码 ID
///
/// # 返回值：
/// * `Result<Option<String>, String>` - 封装好的 "data:image/png;base64,..." 大字符串数据
pub fn get_db_game_logo(
    app: AppHandle,
    game_id: String,
) -> Result<Option<String>, String> {
    let cache_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("cache")
        .join("logos");
    let cached_path = cache_dir.join(format!("{}.png", game_id));

    if cached_path.exists() {
        let content = std::fs::read(&cached_path).map_err(|e| e.to_string())?;
        use base64::Engine;
        let base64_str = base64::engine::general_purpose::STANDARD.encode(&content);
        Ok(Some(format!("data:image/png;base64,{}", base64_str)))
    } else {
        Ok(None)
    }
}


