// game/db.rs - 内置游戏数据库
// 存储常见游戏的存档路径模板，支持用户自定义和分发

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri::AppHandle;
use std::path::PathBuf;

/// 游戏数据库条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameDbEntry {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>, // 常见 exe 名、别名
    pub save_paths: Vec<String>, // 存档路径模板（含 %APPDATA% 等通配符）
    pub platforms: Vec<String>,  // 如 ["windows"]
    pub steam_appid: Option<u64>,
    pub notes: Option<String>,
    pub source: String, // "builtin" | "user"
}

/// 游戏数据库
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GameDatabase {
    pub entries: Vec<GameDbEntry>,
    pub version: u32,
}

impl Default for GameDatabase {
    fn default() -> Self {
        Self {
            entries: builtin_entries(),
            version: 1,
        }
    }
}

/// 获取数据库文件路径
fn db_path(app: &AppHandle) -> anyhow::Result<PathBuf> {
    let dir = app.path().app_local_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("game_database.json"))
}

/// 加载游戏数据库
pub fn load_db(app: &AppHandle) -> anyhow::Result<GameDatabase> {
    let path = db_path(app)?;
    if !path.exists() {
        return Ok(GameDatabase::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let db: GameDatabase = serde_json::from_str(&content)?;
    Ok(merge_builtin(db))
}

/// 保存游戏数据库
pub fn save_db(app: &AppHandle, db: &GameDatabase) -> anyhow::Result<()> {
    let path = db_path(app)?;
    let json = serde_json::to_string_pretty(db)?;
    std::fs::write(&path, json)?;
    Ok(())
}

/// 内置游戏条目 —— 常见游戏的存档路径模板
/// 路径使用 Windows 环境变量占位符，实际使用前需展开
fn builtin_entries() -> Vec<GameDbEntry> {
    vec![
        GameDbEntry {
            id: "elden-ring".to_string(),
            name: "Elden Ring".to_string(),
            aliases: vec!["eldenring.exe".to_string()],
            save_paths: vec![
                "%APPDATA%/EldenRing".to_string(),
            ],
            platforms: vec!["windows".to_string()],
            steam_appid: Some(1245620),
            notes: Some("存档位于 AppData\\Roaming\\EldenRing".to_string()),
            source: "builtin".to_string(),
        },
        GameDbEntry {
            id: "witcher-3".to_string(),
            name: "The Witcher 3: Wild Hunt".to_string(),
            aliases: vec!["witcher3.exe".to_string()],
            save_paths: vec![
                "%USERPROFILE%/Documents/The Witcher 3/gamesaves".to_string(),
            ],
            platforms: vec!["windows".to_string()],
            steam_appid: Some(292030),
            notes: Some("Steam 版与 GOG 版路径相同".to_string()),
            source: "builtin".to_string(),
        },
        GameDbEntry {
            id: "dark-souls-3".to_string(),
            name: "Dark Souls III".to_string(),
            aliases: vec!["DarkSoulsIII.exe".to_string()],
            save_paths: vec![
                "%APPDATA%/DarkSoulsIII".to_string(),
            ],
            platforms: vec!["windows".to_string()],
            steam_appid: Some(374320),
            notes: None,
            source: "builtin".to_string(),
        },
        GameDbEntry {
            id: "baldurs-gate-3".to_string(),
            name: "Baldur's Gate 3".to_string(),
            aliases: vec!["bg3.exe".to_string(), "bg3_dx11.exe".to_string()],
            save_paths: vec![
                "%LOCALAPPDATA%/Larian Studios/Baldur's Gate 3/PlayerProfiles/Public/Savegames".to_string(),
            ],
            platforms: vec!["windows".to_string()],
            steam_appid: Some(1086940),
            notes: Some("Profile 名称可能不同，默认是 Public".to_string()),
            source: "builtin".to_string(),
        },
        GameDbEntry {
            id: "stardew-valley".to_string(),
            name: "Stardew Valley".to_string(),
            aliases: vec!["Stardew Valley.exe".to_string()],
            save_paths: vec![
                "%APPDATA%/StardewValley/Saves".to_string(),
            ],
            platforms: vec!["windows".to_string()],
            steam_appid: Some(413150),
            notes: None,
            source: "builtin".to_string(),
        },
        GameDbEntry {
            id: "cyberpunk-2077".to_string(),
            name: "Cyberpunk 2077".to_string(),
            aliases: vec!["Cyberpunk2077.exe".to_string()],
            save_paths: vec![
                "%LOCALAPPDATA%/CD Projekt Red/Cyberpunk 2077".to_string(),
            ],
            platforms: vec!["windows".to_string()],
            steam_appid: Some(1091500),
            notes: None,
            source: "builtin".to_string(),
        },
        GameDbEntry {
            id: "hollow-knight".to_string(),
            name: "Hollow Knight".to_string(),
            aliases: vec!["hollow_knight.exe".to_string()],
            save_paths: vec![
                "%APPDATA%/unity3d/Team Cherry/Hollow Knight".to_string(),
            ],
            platforms: vec!["windows".to_string()],
            steam_appid: Some(367520),
            notes: None,
            source: "builtin".to_string(),
        },
        GameDbEntry {
            id: "hades".to_string(),
            name: "Hades".to_string(),
            aliases: vec!["Hades.exe".to_string()],
            save_paths: vec![
                "%LOCALAPPDATA%/Hades/Saved/SaveGames".to_string(),
            ],
            platforms: vec!["windows".to_string()],
            steam_appid: Some(1145360),
            notes: None,
            source: "builtin".to_string(),
        },
        GameDbEntry {
            id: "sekiro".to_string(),
            name: "Sekiro: Shadows Die Twice".to_string(),
            aliases: vec!["sekiro.exe".to_string()],
            save_paths: vec![
                "%APPDATA%/Sekiro".to_string(),
            ],
            platforms: vec!["windows".to_string()],
            steam_appid: Some(814380),
            notes: None,
            source: "builtin".to_string(),
        },
        GameDbEntry {
            id: "hades-2".to_string(),
            name: "Hades II".to_string(),
            aliases: vec!["Hades2.exe".to_string()],
            save_paths: vec![
                "%LOCALAPPDATA%/Hades II/Saved/SaveGames".to_string(),
            ],
            platforms: vec!["windows".to_string()],
            steam_appid: Some(1145350),
            notes: None,
            source: "builtin".to_string(),
        },
    ]
}

/// 合并内置条目：确保用户数据库始终包含最新内置数据
fn merge_builtin(mut db: GameDatabase) -> GameDatabase {
    let builtins = builtin_entries();
    let existing_ids: std::collections::HashSet<String> =
        db.entries.iter().map(|e| e.id.clone()).collect();
    for entry in builtins {
        if !existing_ids.contains(&entry.id) {
            db.entries.push(entry);
        }
    }
    db
}

/// 搜索数据库条目（按名称、别名模糊匹配）
pub fn search_entries(db: &GameDatabase, query: &str) -> Vec<GameDbEntry> {
    let q = query.to_lowercase();
    db.entries
        .iter()
        .filter(|e| {
            e.name.to_lowercase().contains(&q)
                || e.aliases.iter().any(|a| a.to_lowercase().contains(&q))
                || e.id.to_lowercase().contains(&q)
        })
        .cloned()
        .collect()
}

// ==================== Tauri Commands ====================

/// 游戏数据库命令模块
pub mod commands {
    use super::*;
    use tauri::AppHandle;

    /// 获取完整游戏数据库
    #[tauri::command]
    pub fn get_game_db(app: AppHandle) -> Result<GameDatabase, String> {
        load_db(&app).map_err(|e| e.to_string())
    }

    /// 搜索游戏数据库
    #[tauri::command]
    pub fn search_game_db(app: AppHandle, query: String) -> Result<Vec<GameDbEntry>, String> {
        let db = load_db(&app).map_err(|e| e.to_string())?;
        Ok(search_entries(&db, &query))
    }

    /// 添加或更新数据库条目
    ///
    /// # 核心增强逻辑：
    /// 本方法修改为在将用户添加或修改的游戏数据库条目（`GameDbEntry`）持久化序列化之前，
    /// 自动对 `entry.save_paths` 内包含的存档绝对物理路径进行全量“环境变量反向折叠”过滤。
    /// 这意味着如果用户在界面上手动输入了形如 `C:\Users\demon\Saved Games\kingdomcome2\saves\playline0`
    /// 这种富含本地机器特异性特征的绝对物理路径，系统会彻底拦截并将其反向收缩折叠为
    /// `%USERPROFILE%/Saved Games/kingdomcome2/saves/playline0`。从而保障从该界面直接导出的
    /// JSON 游戏模板，能够直接分发共享给其他设备无感跨平台使用，从架构层面根除特定用户名硬编码。
    #[tauri::command]
    pub fn upsert_game_db_entry(app: AppHandle, mut entry: GameDbEntry) -> Result<GameDbEntry, String> {
        let mut db = load_db(&app).map_err(|e| e.to_string())?;

        // 遍历整个存档路径列表，对物理路径实施智能折叠收缩为 Windows 环境变量占位符格式
        entry.save_paths = entry
            .save_paths
            .into_iter()
            .map(|p| crate::utils::path::shrink_env(&p))
            .collect();

        // 如果已存在则更新，否则追加
        if let Some(idx) = db.entries.iter().position(|e| e.id == entry.id) {
            db.entries[idx] = entry.clone();
        } else {
            db.entries.push(entry.clone());
        }
        save_db(&app, &db).map_err(|e| e.to_string())?;
        Ok(entry)
    }

    /// 删除数据库条目（只能删除用户自定义条目，内置条目仅重置）
    #[tauri::command]
    pub fn remove_game_db_entry(app: AppHandle, id: String) -> Result<(), String> {
        let mut db = load_db(&app).map_err(|e| e.to_string())?;
        db.entries.retain(|e| e.id != id);
        save_db(&app, &db).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 导出数据库为 JSON 字符串
    #[tauri::command]
    pub fn export_game_db(app: AppHandle) -> Result<String, String> {
        let db = load_db(&app).map_err(|e| e.to_string())?;
        serde_json::to_string_pretty(&db).map_err(|e| e.to_string())
    }

    /// 导入数据库（JSON 字符串）
    #[tauri::command]
    pub fn import_game_db(app: AppHandle, json: String) -> Result<bool, String> {
        let imported: GameDatabase = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        let mut db = load_db(&app).map_err(|e| e.to_string())?;
        // 合并导入的条目，用户自定义的以导入为准
        for entry in imported.entries {
            if let Some(idx) = db.entries.iter().position(|e| e.id == entry.id) {
                // 仅覆盖用户自定义条目；内置条目保留原值
                if db.entries[idx].source == "user" || entry.source == "user" {
                    db.entries[idx] = entry;
                }
            } else {
                db.entries.push(entry);
            }
        }
        save_db(&app, &db).map_err(|e| e.to_string())?;
        Ok(true)
    }

    /// 从数据库条目创建本地游戏配置
    /// 从选定的游戏数据库模板条目一键初始化，并为本地创建全新的游戏同步备份配置
    /// 
    /// # 核心增强:
    /// 本命令修改为**异步实现**：
    /// 1. 获取目标条目的所有基本配置（名称、存档路径模板），并展开路径中的 Windows 环境变量；
    /// 2. 检测该数据库条目是否关联了 `steam_appid`。若存在，则**顺便发起异步网络请求**，通过
    ///    `super::metadata::fetch_logo` 自动前往缓存或 SteamGridDB 官方 API 网关爬取该游戏的高清卡片 Logo 封面图；
    /// 3. 将拉取到的 Logo 本地绝对路径写入配置 `logo_path` 字段中；
    /// 4. 追加保存回主配置文件，实现最省心、高画质的全自动导入关联体验！
    #[tauri::command]
    pub async fn create_game_from_db(
        app: AppHandle,
        db_id: String,
    ) -> Result<crate::config::model::GameConfig, String> {
        let db = load_db(&app).map_err(|e| e.to_string())?;
        let entry = db
            .entries
            .iter()
            .find(|e| e.id == db_id)
            .ok_or_else(|| "未找到该游戏数据库条目".to_string())?;

        // 展开路径模板中的 Windows 环境变量（如 %APPDATA% 等）
        let expanded_paths: Vec<String> = entry
            .save_paths
            .iter()
            .map(|p| crate::utils::path::expand_env(p))
            .collect();

        // 使用物理 ID 与当前毫秒时间戳生成防冲突的本地游戏唯一标识
        let id = format!(
            "{}_{}",
            entry.id.clone(),
            chrono::Utc::now().timestamp_millis()
        );

        // 自动拉取并关联游戏图片 Logo 封面
        // 1. 优先尝试继承来自数据库模板条目已经存在的本地缓存（例如用户此前关联 exe 物理提取的自定义图标）
        let cache_dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("cache").join("logos");
        let entry_logo_path = cache_dir.join(format!("{}.png", entry.id));
        let new_logo_path = cache_dir.join(format!("{}.png", id));

        let logo_path = if entry_logo_path.exists() {
            // 物理复制图标，完成无缝继承
            if std::fs::copy(&entry_logo_path, &new_logo_path).is_ok() {
                Some(new_logo_path.to_string_lossy().to_string())
            } else {
                None
            }
        } else if let Some(appid) = entry.steam_appid {
            // 2. 否则，如果关联了 steam_appid，则异步拉取
            match crate::game::metadata::fetch_logo(&app, &id, Some(appid)).await {
                Ok(Some(path)) => Some(path),
                _ => None,
            }
        } else {
            None
        };

        let game = crate::config::model::GameConfig {
            id: id.clone(),
            name: entry.name.clone(),
            save_paths: expanded_paths,
            remote_path: format!("/GameSaves/{}", id),
            last_backup: None,
            logo_path,
            steam_appid: entry.steam_appid,
        };

        let mut config = crate::config::load_config(&app).map_err(|e| e.to_string())?;
        config.games.push(game.clone());
        crate::config::save_config(&app, &config).map_err(|e| e.to_string())?;

        Ok(game)
    }
}
