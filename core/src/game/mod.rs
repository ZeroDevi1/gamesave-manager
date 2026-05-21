// game/mod.rs - 游戏管理入口
pub mod pcgw;
pub mod db;
pub mod registry;
pub mod scanner;
pub mod metadata;
pub mod icon_extractor;

use serde::{Deserialize, Serialize};

/// 游戏配置（与 config model 共享，但这里用于前端交互）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInfo {
    pub id: String,
    pub name: String,
    pub save_paths: Vec<String>,
    pub remote_path: String,
    pub last_backup: Option<String>,
    pub logo_path: Option<String>,
    pub steam_appid: Option<u64>,
}

/// Tauri Commands 导出
pub mod commands {
    use super::*;
    use tauri::AppHandle;

    /// 获取游戏列表
    ///
    /// # 核心增强：
    /// 自动植入后台静默封面补全协程：
    /// 1. 对于已配置 steam_appid 但缺失 logo 的游戏，直接从 Steam CDN 拉取封面
    /// 2. 对于未配置 steam_appid 的游戏，尝试通过游戏名称在游戏数据库中模糊匹配，
    ///    若匹配到条目且该条目包含 steam_appid，则借用此 AppID 获取封面并补齐
    /// 3. 全部静默持久化存盘，保证首页游戏卡片在加载后 2 秒内展现高清海报
    #[tauri::command]
    pub fn get_games(app: AppHandle) -> Result<Vec<crate::config::model::GameConfig>, String> {
        let config = crate::config::load_config(&app).map_err(|e| e.to_string())?;
        let games_clone = config.games.clone();

        // 加载游戏数据库供后台匹配使用
        let game_db = match crate::game::db::load_db(&app) {
            Ok(db) => db,
            Err(_) => crate::game::db::GameDatabase::default(),
        };

        // 启动后台静默微协程，对缺失 logo_path 的游戏进行全自动补全
        let app_handle_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut need_save = false;
            let mut current_config = match crate::config::load_config(&app_handle_clone) {
                Ok(c) => c,
                Err(_) => return,
            };

            for game in &mut current_config.games {
                if game.logo_path.is_some() {
                    continue;
                }

                let appid = if let Some(id) = game.steam_appid {
                    Some(id)
                } else {
                    // 通过游戏名在数据库中模糊匹配，借用 steam_appid
                    let matches = crate::game::db::search_entries(&game_db, &game.name);
                    matches.into_iter().next().and_then(|e| e.steam_appid)
                };

                if let Some(id) = appid {
                    if let Ok(Some(logo_path)) = crate::game::metadata::fetch_logo(
                        &app_handle_clone,
                        &game.id,
                        Some(id),
                    )
                    .await
                    {
                        game.logo_path = Some(logo_path);
                        game.steam_appid = Some(id);
                        need_save = true;
                    }
                }
            }

            // 仅在真实发生封面补全时才静默存盘
            if need_save {
                let _ = crate::config::save_config(&app_handle_clone, &current_config);
            }
        });

        Ok(games_clone)
    }

    /// 添加游戏
    #[tauri::command]
    pub fn add_game(
        app: AppHandle,
        name: String,
        save_paths: Vec<String>,
    ) -> Result<crate::config::model::GameConfig, String> {
        let mut config = crate::config::load_config(&app).map_err(|e| e.to_string())?;
        let id = format!(
            "{}_{}",
            name.to_lowercase().replace(' ', "-"),
            chrono::Utc::now().timestamp_millis()
        );
        let game = crate::config::model::GameConfig {
            id: id.clone(),
            name: name.clone(),
            save_paths: save_paths.clone(),
            remote_path: format!("/GameSaves/{}", name),
            last_backup: None,
            logo_path: None,
            steam_appid: None,
        };
        config.games.push(game.clone());
        crate::config::save_config(&app, &config).map_err(|e| e.to_string())?;
        Ok(game)
    }

    /// 删除游戏
    #[tauri::command]
    pub fn remove_game(app: AppHandle, game_id: String) -> Result<(), String> {
        let mut config = crate::config::load_config(&app).map_err(|e| e.to_string())?;
        config.games.retain(|g| g.id != game_id);
        crate::config::save_config(&app, &config).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 扫描游戏存档
    #[tauri::command]
    pub fn scan_game_saves(
        app: AppHandle,
        game_id: String,
    ) -> Result<Vec<scanner::SaveFile>, String> {
        scanner::scan_game(&app, &game_id).map_err(|e| e.to_string())
    }

    /// 获取游戏 Logo
    #[tauri::command]
    pub async fn get_game_logo(
        app: AppHandle,
        game_id: String,
        steam_appid: Option<u64>,
    ) -> Result<Option<String>, String> {
        metadata::fetch_logo(&app, &game_id, steam_appid)
            .await
            .map_err(|e| e.to_string())
    }

    // 游戏数据库命令在 db::commands 中定义
    #[tauri::command]
    pub async fn select_and_extract_exe_icon(app: tauri::AppHandle) -> Result<Option<serde_json::Value>, String> {
        super::icon_extractor::select_and_extract_exe_icon(app).await
    }

    #[tauri::command]
    pub fn save_custom_logo(app: tauri::AppHandle, game_id: String, logo_base64: String) -> Result<(), String> {
        super::metadata::save_custom_logo(app, game_id, logo_base64)
    }

    #[tauri::command]
    pub fn get_db_game_logo(app: tauri::AppHandle, game_id: String) -> Result<Option<String>, String> {
        super::metadata::get_db_game_logo(app, game_id)
    }
    #[tauri::command]
    pub fn launch_game(steam_appid: Option<u64>) -> Result<(), String> {
        let Some(appid) = steam_appid else {
            return Err("该游戏未配置 Steam AppID，无法自动启动".to_string());
        };
        let url = format!("steam://rungameid/{}", appid);
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/C", "start", "", &url])
                .spawn()
                .map_err(|e| format!("启动失败: {}", e))?;
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = url;
            return Err("当前仅支持 Windows 平台启动游戏".to_string());
        }
        Ok(())
    }
}

/// 重新导出数据库命令以便在 lib.rs 中统一注册
pub use db::commands as db_commands;
/// 重新导出 PCGamingWiki 命令
pub use pcgw::commands as pcgw_commands;
