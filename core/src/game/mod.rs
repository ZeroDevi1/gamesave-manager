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
    /// 自动植入**后台静默保洁清洗协程 (Background Cover Sweep Engine)**：
    /// 当每次加载游戏配置列表时，系统会在后台默默扫视所有已配置的本地游戏。
    /// 一旦发现某个游戏关联了 `steam_appid`，但因为未设置 SteamGridDB API 秘钥等历史成因
    /// 导致 `logo_path` 依然为 `None`（呈现简陋蓝色占位），后台协程将立刻异步向 Steam CDN
    /// 免费公开直链发起并发抓取高清海报并下载，自动补齐该空缺，并静默持久化存盘。
    /// 这保证了不管用户的游戏是从何处导入、是否配了 Key，都能在加载后 2 秒内静默蜕变成超清海报视觉！
    #[tauri::command]
    pub fn get_games(app: AppHandle) -> Result<Vec<crate::config::model::GameConfig>, String> {
        let config = crate::config::load_config(&app).map_err(|e| e.to_string())?;
        let games_clone = config.games.clone();

        // 启动后台静默微协程，对缺失 logo_path 的游戏进行全自动补网式抓取
        let app_handle_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut need_save = false;
            let mut current_config = match crate::config::load_config(&app_handle_clone) {
                Ok(c) => c,
                Err(_) => return,
            };

            for game in &mut current_config.games {
                // 如果发现游戏包含 Steam AppID，但是封面图片路径为空，则立刻触发后台补全
                if game.logo_path.is_none() && game.steam_appid.is_some() {
                    // 异步向网络获取封面绝对物理路径
                    if let Ok(Some(logo_path)) = crate::game::metadata::fetch_logo(
                        &app_handle_clone,
                        &game.id,
                        game.steam_appid,
                    )
                    .await
                    {
                        game.logo_path = Some(logo_path);
                        need_save = true;
                    }
                }
            }

            // 仅在真实发生封面补全时才静默存盘，防范无意义的磁盘 I/O 开销
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
