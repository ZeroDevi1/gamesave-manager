// game/pcgw.rs - PCGamingWiki 数据抓取
// 通过 Cargo API 搜索游戏，通过 Parse API 获取存档路径
// 支持 Steam Store API 中文名→英文名桥接

use serde::{Deserialize, Serialize};
use std::time::Duration;

const PCGW_API_BASE: &str = "https://www.pcgamingwiki.com/w/api.php";
const STEAM_STORE_API: &str = "https://store.steampowered.com/api/storesearch";
const USER_AGENT: &str = "GameSaveManager/0.1.0 (https://github.com/gamesave-manager)";

/// PCGamingWiki 搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcgwSearchResult {
    pub page_name: String,
    pub steam_appid: Option<u64>,
}

/// PCGamingWiki 游戏详情（含存档路径）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcgwGameDetail {
    pub page_name: String,
    pub steam_appid: Option<u64>,
    pub windows_save_paths: Vec<String>,
    pub notes: Option<String>,
}

/// Steam Store 搜索结果条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamStoreItem {
    pub name: String,
    pub id: u32,
}

/// Steam Store 搜索响应
#[derive(Debug, Deserialize)]
struct SteamStoreResponse {
    items: Option<Vec<SteamStoreItem>>,
}

/// 创建带超时的 HTTP 客户端
fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_default()
}

// ==================== Steam Store API 中文搜索 ====================

/// Steam 商店详情数据载荷结构
#[derive(Debug, Deserialize)]
struct SteamAppDetailsData {
    /// 游戏在对应语言下的官方名称
    name: String,
}

/// Steam 商店详情 API 响应结构
#[derive(Debug, Deserialize)]
struct SteamAppDetails {
    /// 请求是否成功
    success: bool,
    /// 游戏详细数据
    data: Option<SteamAppDetailsData>,
}

/// 通过 Steam Store API 搜索游戏，若为中文搜索则自动并发桥接 appdetails 翻译为官方英文名称
pub async fn search_steam_store(query: &str) -> anyhow::Result<Vec<SteamStoreItem>> {
    let client = http_client();
    
    // 1. 判断搜索词是否包含中文字符，以决定采用哪种语言区域检索
    let has_chinese = query.chars().any(|c| (c as u32) >= 0x4e00 && (c as u32) <= 0x9fa5);
    
    // 2. 根据是否存在中文选择不同的商店搜索参数
    // 若含中文，采用国区 cc=CN 配合简体中文 l=schinese 进行搜索以提高分词与条目匹配精度
    // 若无中文，则保持默认的美区 cc=US 与英语 l=english 搜索
    let url = if has_chinese {
        format!(
            "{}?term={}&cc=CN&l=schinese",
            STEAM_STORE_API,
            urlencoding::encode(query)
        )
    } else {
        format!(
            "{}?term={}&cc=US&l=english",
            STEAM_STORE_API,
            urlencoding::encode(query)
        )
    };

    let resp = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Steam Store API 请求失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!(
            "Steam Store API 返回 HTTP {}",
            resp.status()
        ));
    }

    let body = resp.text().await.map_err(|e| {
        anyhow::anyhow!("读取 Steam Store 响应失败: {}", e)
    })?;

    // Steam Store API 返回的是 JSONP 格式，需剔除可能的回调函数包装
    let json_str = if body.starts_with('"') || body.starts_with('{') {
        body
    } else {
        let start = body.find('{').unwrap_or(0);
        let end = body.rfind('}').map(|i| i + 1).unwrap_or(body.len());
        body[start..end].to_string()
    };

    let result: SteamStoreResponse = serde_json::from_str(&json_str)
        .map_err(|e| anyhow::anyhow!("解析 Steam Store 响应失败: {} | 原始: {}", e, &json_str[..json_str.len().min(200)]))?;

    let mut items = result.items.unwrap_or_default();

    // 3. 如果原本是中文搜索，或者结果不为空，我们统一并发请求 Steam AppDetails 接口将其“翻译为官方英文名”
    if !items.is_empty() {
        use std::collections::HashMap;
        
        let mut futures = Vec::new();
        // 限制最多并发翻译前 5 个结果，防止触发 Steam 接口限流，且已足够覆盖常用选项
        let limit = items.len().min(5);
        
        for i in 0..limit {
            let appid = items[i].id;
            let client_clone = client.clone();
            
            // 构造并发异步 Future 块以获取该 AppID 在英语下的详细信息
            futures.push(async move {
                let detail_url = format!(
                    "https://store.steampowered.com/api/appdetails?appids={}&l=english",
                    appid
                );
                
                if let Ok(detail_resp) = client_clone
                    .get(&detail_url)
                    .header("User-Agent", USER_AGENT)
                    .send()
                    .await 
                {
                    if detail_resp.status().is_success() {
                        if let Ok(detail_text) = detail_resp.text().await {
                            // 解析嵌套的外层 AppID 键值映射
                            if let Ok(mut detail_map) = serde_json::from_str::<HashMap<String, SteamAppDetails>>(&detail_text) {
                                if let Some(app_info) = detail_map.remove(&appid.to_string()) {
                                    if app_info.success && app_info.data.is_some() {
                                        return Some((appid, app_info.data.unwrap().name));
                                    }
                                }
                            }
                        }
                    }
                }
                None
            });
        }

        // 使用 join_all 并行处理所有翻译请求
        let translated_results = futures::future::join_all(futures).await;
        let mut translation_map = HashMap::new();
        for res in translated_results {
            if let Some((appid, eng_name)) = res {
                translation_map.insert(appid, eng_name);
            }
        }

        // 回填替换为翻译后的官方英文名称，从而实现全自动中译英桥接
        for item in &mut items {
            if let Some(eng_name) = translation_map.get(&item.id) {
                item.name = eng_name.clone();
            }
        }
    }

    Ok(items)
}

// ==================== Cargo API 搜索 ====================

/// 通过 Cargo API 搜索 PCGamingWiki 游戏
pub async fn search_games(query: &str) -> anyhow::Result<Vec<PcgwSearchResult>> {
    let client = http_client();
    let url = format!(
        "{}?action=cargoquery&tables=Infobox_game&fields=Infobox_game._pageName=Page,Infobox_game.Steam_AppID&where=Infobox_game._pageName%20LIKE%20%22%25{}%25%22&limit=20&format=json&origin=*",
        PCGW_API_BASE,
        urlencoding::encode(query)
    );

    log::info!("[PCGW] 搜索游戏: url={}", url);

    let resp = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("请求 PCGamingWiki Cargo API 失败: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "PCGamingWiki Cargo API 返回 HTTP {}: {}",
            status,
            body
        ));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| {
        anyhow::anyhow!("解析 PCGamingWiki Cargo 响应失败: {}", e)
    })?;

    let mut results = Vec::new();

    if let Some(rows) = json.get("cargoquery").and_then(|v| v.as_array()) {
        for row in rows {
            if let Some(title) = row.get("title") {
                let page_name = title
                    .get("Page")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let steam_appid = title
                    .get("Steam AppID")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.split(',').next())
                    .and_then(|s| s.trim().parse::<u64>().ok());

                if !page_name.is_empty() {
                    results.push(PcgwSearchResult {
                        page_name,
                        steam_appid,
                    });
                }
            }
        }
    }

    Ok(results)
}

// ==================== AppID 精确查 PCGW ====================

/// 通过 Steam AppID 精确查询 PCGamingWiki 页面名
/// 使用 Cargo API `WHERE Steam_AppID = {appid}` 精确匹配
/// 返回对应的 PCGW 搜索结果列表（通常最多一条）
pub async fn search_games_by_steam_appid(appid: u64) -> anyhow::Result<Vec<PcgwSearchResult>> {
    let client = http_client();
    // Cargo API：按 Steam_AppID 精确查询
    let url = format!(
        "{}?action=cargoquery&tables=Infobox_game&fields=Infobox_game._pageName=Page,Infobox_game.Steam_AppID&where=Infobox_game.Steam_AppID%20=%20{}&limit=5&format=json&origin=*",
        PCGW_API_BASE,
        appid
    );

    log::info!("[PCGW] 按 AppID 查询: url={}", url);

    let resp = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("请求 PCGamingWiki Cargo API (AppID) 失败: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "PCGamingWiki Cargo API (AppID) 返回 HTTP {}: {}",
            status,
            body
        ));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| {
        anyhow::anyhow!("解析 PCGamingWiki Cargo (AppID) 响应失败: {}", e)
    })?;

    let mut results = Vec::new();
    if let Some(rows) = json.get("cargoquery").and_then(|v| v.as_array()) {
        for row in rows {
            if let Some(title) = row.get("title") {
                let page_name = title
                    .get("Page")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let steam_appid = title
                    .get("Steam AppID")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.split(',').next())
                    .and_then(|s| s.trim().parse::<u64>().ok());

                if !page_name.is_empty() {
                    results.push(PcgwSearchResult {
                        page_name,
                        steam_appid,
                    });
                }
            }
        }
    }

    Ok(results)
}

// ==================== 多策略 Steam 搜索 ====================

/// 多策略鲁棒搜索 Steam 商店
/// 
/// 核心思路：Steam storesearch 对中文极度敏感，完整中文名可能搜不出结果。
/// 本函数采用渐进式退化策略：
/// 1. 先用完整查询 → 无结果则逐字缩短重试（从末尾删除）
/// 2. 每轮缩短后检查，找到结果即返回
/// 3. 仍无结果则尝试去标点、仅保留中文字符再搜
/// 
/// 返回 (搜索到的条目列表, 实际使用的查询字符串)
pub async fn search_steam_store_robust(query: &str) -> anyhow::Result<(Vec<SteamStoreItem>, String)> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok((Vec::new(), String::new()));
    }

    // 策略1：逐步缩短查询，从末尾删字符直到找到结果
    let mut current = trimmed.to_string();
    while current.len() >= 2 {
        let results = search_steam_store(&current).await?;
        if !results.is_empty() {
            log::info!(
                "[Steam鲁棒] 查询 '{}' → 缩短到 '{}' 得到 {} 条结果",
                trimmed,
                current,
                results.len()
            );
            return Ok((results, current));
        }
        // 去掉末尾字符继续尝试
        let chars: Vec<char> = current.chars().collect();
        if chars.len() <= 2 {
            break;
        }
        current = chars[..chars.len() - 1].iter().collect();
    }

    // 策略2：去标点，仅保留中文+字母+数字，再试
    let cleaned: String = trimmed
        .chars()
        .filter(|c| c.is_alphanumeric() || (*c as u32) >= 0x4e00 && (*c as u32) <= 0x9fa5)
        .collect();
    if cleaned.len() >= 2 && cleaned != trimmed {
        let results = search_steam_store(&cleaned).await?;
        if !results.is_empty() {
            log::info!(
                "[Steam鲁棒] 去标点后查询 '{}' 得到 {} 条结果",
                cleaned,
                results.len()
            );
            return Ok((results, cleaned));
        }
    }

    // 策略3：仅用纯中文字符再试（如果有的话）
    let chinese_only: String = trimmed
        .chars()
        .filter(|c| (*c as u32) >= 0x4e00 && (*c as u32) <= 0x9fa5)
        .collect();
    if chinese_only.len() >= 2 && chinese_only != trimmed && chinese_only != cleaned {
        let results = search_steam_store(&chinese_only).await?;
        if !results.is_empty() {
            log::info!(
                "[Steam鲁棒] 仅用中文 '{}' 得到 {} 条结果",
                chinese_only,
                results.len()
            );
            return Ok((results, chinese_only));
        }
    }

    Ok((Vec::new(), String::new()))
}

// ==================== Parse API 获取存档路径 ====================

/// 通过 Parse API 获取页面 wikitext，提取 Windows 存档路径
pub async fn fetch_save_paths(page_name: &str) -> anyhow::Result<PcgwGameDetail> {
    fetch_save_paths_inner(page_name, true).await
}
/// 内部实现：支持 missingtitle 容错重试
///
/// # 参数
/// - `page_name`: PCGW 页面名
/// - `allow_fallback`: 是否允许在 missingtitle 时通过 Cargo API 搜索并重新获取
fn fetch_save_paths_inner<'a>(
    page_name: &'a str,
    allow_fallback: bool,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<PcgwGameDetail>> + Send + 'a>> {
    Box::pin(async move {
        // 清理 Steam 商店名称中常见的商标符号，PCGW 页面名不含这些字符
        let clean_name = page_name.replace('®', "").replace('™', "");
        let query_name = if clean_name.is_empty() { page_name } else { &clean_name };
        let client = http_client();
        let url = format!(
            "{}?action=parse&page={}&prop=wikitext&format=json&origin=*",
            PCGW_API_BASE,
            urlencoding::encode(query_name)
        );
        log::info!("[PCGW] 获取存档路径: page_name={}, url={}", query_name, url);
        let resp = client
            .get(&url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("请求 PCGamingWiki Parse API 失败: {}", e))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "PCGamingWiki Parse API 返回 HTTP {}: {}",
                status,
                body
            ));
        }
        let body_text = resp.text().await.map_err(|e| {
            anyhow::anyhow!("读取 PCGamingWiki Parse 响应体失败: {}", e)
        })?;
        log::debug!("[PCGW] Parse 响应原始文本长度: {}", body_text.len());
        let json: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
            anyhow::anyhow!(
                "解析 PCGamingWiki Parse JSON 失败: {} | 原始文本前 500 字: {}",
                e,
                &body_text[..body_text.len().min(500)]
            )
        })?;
        // 检查 API 错误
        if let Some(error) = json.get("error") {
            let code = error.get("code").and_then(|v| v.as_str()).unwrap_or("unknown");
            let info = error.get("info").and_then(|v| v.as_str()).unwrap_or("");
            // 若页面不存在且允许回退，尝试通过 Cargo API 搜索最匹配的页面名
            if allow_fallback && code == "missingtitle" {
                log::info!(
                    "[PCGW] 页面 '{}' 不存在，尝试 Cargo API 回退搜索...",
                    query_name
                );
                return try_fetch_via_cargo_fallback(query_name).await;
            }
            return Err(anyhow::anyhow!(
                "PCGamingWiki API 错误 [{}]: {}",
                code,
                info
            ));
        }
        let wikitext = json
            .get("parse")
            .and_then(|p| p.get("wikitext"))
            .and_then(|w| w.get("*"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if wikitext.is_empty() {
            return Err(anyhow::anyhow!(
                "PCGamingWiki 返回了空 wikitext，页面 '{}' 可能不存在",
                query_name
            ));
        }
        let (paths, notes) = parse_windows_save_paths(wikitext);
        let steam_appid = extract_steam_appid_from_wikitext(wikitext);
        log::info!(
            "[PCGW] 解析完成: page='{}', paths={:?}, appid={:?}",
            query_name,
            paths,
            steam_appid
        );
        Ok(PcgwGameDetail {
            page_name: query_name.to_string(),
            steam_appid,
            windows_save_paths: paths,
            notes,
        })
    })
}
/// Cargo API 回退搜索：当 Parse API 返回 missingtitle 时，
/// 用页面名搜索 Cargo API，取第一条结果的 page_name 重新获取存档路径
fn try_fetch_via_cargo_fallback<'a>(
    page_name: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<PcgwGameDetail>> + Send + 'a>> {
    Box::pin(async move {
        // 搜索前先清理商标符号，确保 Cargo 搜索不受®™干扰
        let clean_query = page_name.replace('®', "").replace('™', "");
        let search_term = if clean_query.is_empty() { page_name } else { &clean_query };
        let candidates = search_games(search_term).await.map_err(|e| {
            anyhow::anyhow!(
                "PCGamingWiki 页面 '{}' 不存在，且 Cargo 回退搜索也失败: {}",
                page_name,
                e
            )
        })?;
        if candidates.is_empty() {
            return Err(anyhow::anyhow!(
                "PCGamingWiki 页面 '{}' 不存在，Cargo 回退搜索也未找到匹配游戏",
                page_name
            ));
        }
        let fallback_name = &candidates[0].page_name;
        log::info!(
            "[PCGW] Cargo 回退搜索命中: '{}' → '{}'，重新获取存档路径...",
            page_name,
            fallback_name
        );
        // 递归调用但关闭再次回退，防止无限循环
        fetch_save_paths_inner(fallback_name, false).await.map_err(|e| {
            anyhow::anyhow!(
                "PCGamingWiki 页面 '{}' 不存在，回退到 '{}' 后仍然失败: {}",
                page_name,
                fallback_name,
                e
            )
        })
    })
}
/// 解析 wikitext 中的 Windows 存档路径
fn parse_windows_save_paths(wikitext: &str) -> (Vec<String>, Option<String>) {
    let mut paths = Vec::new();
    let mut notes = Vec::new();

    // 采用不区分大小写和空格的正则表达式来寻找“Save game data location”块
    let re_title = regex::Regex::new(r"(?i)==+\s*Save\s+game\s+data\s+location\s*==+").unwrap();
    let parts: Vec<&str> = re_title.split(wikitext).collect();
    let save_section = parts.get(1).unwrap_or(&"");

    // 限制只解析到下一个同层级标题（以等号开头的下一行）之前
    let re_next_title = regex::Regex::new(r"\n==+").unwrap();
    let save_section = re_next_title.split(save_section).next().unwrap_or(save_section);

    // 匹配 Windows 存档路径行
    // 格式: {{Game data/saves|Windows|{{p|appdata}}\EldenRing\...}}
    for line in save_section.lines() {
        let trimmed = line.trim();
        // 移除所有空格并转为小写来匹配开头
        let collapsed = trimmed.replace(" ", "").to_lowercase();
        if collapsed.starts_with("{{gamedata/saves|windows|") {
            if let Some(path) = extract_path_from_saves_line(trimmed) {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        } else if collapsed.starts_with("{{gamedata/saves|osx|")
            || collapsed.starts_with("{{gamedata/saves|linux|")
            || collapsed.starts_with("{{gamedata/saves|dos|")
            || collapsed.starts_with("{{gamedata/saves|steam|")
            || collapsed.starts_with("{{gamedata/saves|microsoftstore|")
        {
            // 跳过非 Windows 平台
            continue;
        } else if trimmed.starts_with("{{--}}") {
            // 备注信息
            let note = trimmed
                .trim_start_matches("{{--}}")
                .trim()
                .trim_end_matches(".")
                .to_string();
            if !note.is_empty() {
                notes.push(note);
            }
        } else if trimmed == "}}" || trimmed == "{{Game data|" {
            // 块边界
            continue;
        }
    }

    let notes_str = if notes.is_empty() {
        None
    } else {
        Some(notes.join("; "))
    };

    (paths, notes_str)
}

/// 从单行 wikitext 存档数据配置项中智能提取 Windows 存档路径
/// 
/// # 核心原理 (嵌套花括号解析算法):
/// PCGamingWiki 上的存档行路径可能包含嵌套的子占位符（例如：`{{p|userprofile}}\Saved Games\...`）。
/// 原本使用正则 `([^}]+)` 提取路径时，会在遇到子占位符中间的第一个外露大括号 `}` 时提早截断，
/// 最终导致路径被完全截断破坏。
/// 本算法引入了**嵌套花括号计数器 (`brace_depth`)**：
/// 1. 使用不区分大小写的正则表达式匹配头部前缀 `{{Game data/saves|Windows|`；
/// 2. 从匹配到的路径起点开始，逐字符遍历后续后缀字符串；
/// 3. 若遇到 `{`，使 `brace_depth` 加 1；若遇到 `}`，使 `brace_depth` 减 1；
/// 4. 仅在 `brace_depth == 0` 的平衡状态下，一旦遇到备注分隔符 `|` 或最外层宏标签的闭合括号 `}` 时，才结束路径提取；
/// 5. 这样能够 100% 完整提取包含嵌套子宏标签的整个复杂物理路径，并避免在提取时包含尾部备注。
fn extract_path_from_saves_line(line: &str) -> Option<String> {
    // 使用大小写不敏感与多空格宽容的正则表达式精准检索头部匹配位置
    let prefix_re = regex::Regex::new(r"(?i)^\s*\{\{\s*Game\s+data/saves\s*\|\s*Windows\s*\|\s*").ok()?;
    let mat = prefix_re.find(line)?;
    
    // 截取匹配前缀终点之后的剩余字符子串
    let suffix = &line[mat.end()..];
    
    let mut brace_depth = 0;
    let mut raw_path = String::new();
    
    // 逐字符遍历，智能识别花括号闭合嵌套
    for c in suffix.chars() {
        // 如果在花括号配对平衡状态下遇到了参数分隔符 '|'，说明已进入下一个备注等参数，停止提取
        if c == '|' && brace_depth == 0 {
            break;
        }
        // 如果在花括号配对平衡状态下遇到了 '}'，说明已到达最外层宏标签的闭合边界，停止提取
        if c == '}' && brace_depth == 0 {
            break;
        }
        
        // 跟踪计数器花括号深度
        if c == '{' {
            brace_depth += 1;
        } else if c == '}' {
            brace_depth -= 1;
        }
        
        raw_path.push(c);
    }
    
    // 清除边缘零碎空白
    let raw_path = raw_path.trim();
    if raw_path.is_empty() {
        return None;
    }
    
    // 将 PCGamingWiki 自定义路径宏（如 {{p|userprofile}}）翻译成合法的 Windows 环境变量占位符
    let converted = convert_pcgw_placeholders(raw_path);
    Some(converted)
}

/// 转换 PCGamingWiki 路径占位符
fn convert_pcgw_placeholders(text: &str) -> String {
    let mut result = text.to_string();
    let replacements = [
        (r"(?i)\{\{\s*p\s*\|\s*appdata\s*\}\}", "%APPDATA%"),
        (r"(?i)\{\{\s*p\s*\|\s*localappdata\s*\}\}", "%LOCALAPPDATA%"),
        (r"(?i)\{\{\s*p\s*\|\s*userprofile\\documents\s*\}\}", "%USERPROFILE%/Documents"),
        (r"(?i)\{\{\s*p\s*\|\s*userprofile/documents\s*\}\}", "%USERPROFILE%/Documents"),
        (r"(?i)\{\{\s*p\s*\|\s*userprofile\s*\}\}", "%USERPROFILE%"),
        (r"(?i)\{\{\s*p\s*\|\s*programfiles\s*\}\}", "%PROGRAMFILES%"),
        (r"(?i)\{\{\s*p\s*\|\s*public\s*\}\}", "%PUBLIC%"),
        (r"(?i)\{\{\s*p\s*\|\s*steam\s*\}\}", "%STEAMPATH%"),
        // Uplay 存档路径：Ubisoft Connect 默认安装在 Program Files (x86) 下
        (r"(?i)\{\{\s*p\s*\|\s*uplay\s*\}\}", "%PROGRAMFILES(X86)%/Ubisoft Game Launcher"),
        // UID 为动态数字，替换为 * 通配符以利用 glob 匹配
        (r"(?i)\{\{\s*p\s*\|\s*uid\s*\}\}", "*"),
        (r"(?i)\{\{\s*p\s*\|\s*game\s*\}\}", "<path-to-game>"),
    ];
    for (pattern, to) in &replacements {
        if let Ok(re) = regex::Regex::new(pattern) {
            result = re.replace_all(&result, *to).to_string();
        }
    }
    // 统一使用正斜杠
    result.replace('\\', "/")
}

/// 从 wikitext 中提取 Steam AppID
fn extract_steam_appid_from_wikitext(wikitext: &str) -> Option<u64> {
    // 查找 Infobox 中的 Steam AppID
    let re = regex::Regex::new(r"\|Steam AppID\s*=\s*(\d+)").ok()?;
    re.captures(wikitext)
        .and_then(|cap| cap.get(1))
        .and_then(|m| m.as_str().parse::<u64>().ok())
}

// ==================== Tauri Commands ====================

pub mod commands {
    use super::*;

    /// 搜索 PCGamingWiki 游戏
    #[tauri::command]
    pub async fn search_pcgw_games(query: String) -> Result<Vec<PcgwSearchResult>, String> {
        search_games(&query).await.map_err(|e| e.to_string())
    }

    /// 获取 PCGamingWiki 游戏存档路径
    #[tauri::command]
    pub async fn fetch_pcgw_save_paths(page_name: String) -> Result<PcgwGameDetail, String> {
        fetch_save_paths(&page_name).await.map_err(|e| e.to_string())
    }

    /// 通过 Steam Store API 搜索游戏（中文→英文）
    #[tauri::command]
    pub async fn search_steam_store_cmd(query: String) -> Result<Vec<SteamStoreItem>, String> {
        super::search_steam_store(&query).await.map_err(|e| e.to_string())
    }

    /// 通过 Steam AppID 精确查询 PCGamingWiki 游戏页面
    #[tauri::command]
    pub async fn search_pcgw_by_steam_appid(appid: u64) -> Result<Vec<PcgwSearchResult>, String> {
        search_games_by_steam_appid(appid).await.map_err(|e| e.to_string())
    }

    /// 多策略鲁棒搜索 Steam 商店（逐步缩短 + 去标点回退）
    /// 返回搜索结果和实际使用的查询字符串
    #[tauri::command]
    pub async fn search_steam_store_robust_cmd(query: String) -> Result<(Vec<SteamStoreItem>, String), String> {
        super::search_steam_store_robust(&query).await.map_err(|e| e.to_string())
    }
}
