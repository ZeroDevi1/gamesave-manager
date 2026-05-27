// game/mod.rs - 游戏管理入口
pub mod pcgw;
pub mod db;
pub mod registry;
pub mod scanner;
pub mod metadata;
pub mod icon_extractor;
pub mod process_watcher;

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
    /// 对于缺失 logo 的游戏，直接通过 Steam CDN 公开直链获取封面图，
    /// 无需等待异步下载到本地。优先使用游戏自身配置的 steam_appid；
    /// 若未配置，则通过游戏名称在数据库中模糊匹配借用 AppID。
    /// 图片 URL 会同步写入配置并持久化，保证首页卡片首次加载即有图标。
    #[tauri::command]
    pub fn get_games(app: AppHandle) -> Result<Vec<crate::config::model::GameConfig>, String> {
        let mut config = crate::config::load_config(&app).map_err(|e| e.to_string())?;

        // 加载游戏数据库供匹配使用
        let game_db = match crate::game::db::load_db(&app) {
            Ok(db) => db,
            Err(_) => crate::game::db::GameDatabase::default(),
        };

        let mut need_save = false;
        for game in &mut config.games {
            // 已有 HTTP 远程封面 URL 则跳过；本地路径或缺失则重新获取
            if game.logo_path.as_ref().map(|p| p.starts_with("http")).unwrap_or(false) {
                continue;
            }

            let appid = game.steam_appid.or_else(|| {
                let matches = crate::game::db::search_entries(&game_db, &game.name);
                matches.into_iter().next().and_then(|e| e.steam_appid)
            });

            if let Some(id) = appid {
                // 直接使用 Steam CDN 公开直链，无需下载到本地
                game.logo_path = Some(format!(
                    "https://steamcdn-a.akamaihd.net/steam/apps/{}/library_600x900_2x.jpg",
                    id
                ));
                if game.steam_appid.is_none() {
                    game.steam_appid = Some(id);
                }
                need_save = true;
            }
        }

        if need_save {
            let _ = crate::config::save_config(&app, &config);
        }

        Ok(config.games)
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
            exe_name: None,           // 手动添加的游戏需用户后续设置 exe 名
            auto_backup_enabled: None, // 跟随全局设置
            confirm_before_sync: None, // 跟随全局设置
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

    /// 启动游戏前的同步结果（前端用于展示同步状态）
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LaunchWithSyncResult {
        /// 游戏是否成功启动
        pub launched: bool,
        /// 同步结果描述（无同步时为空字符串）
        pub sync_message: String,
        /// 同步的文件数
        pub sync_files_changed: usize,
        /// 是否启动了进程监控（exe_name 有值且 auto_backup 开启时为 true）
        pub watcher_active: bool,
    }

    /// 带自动同步和进程监控的游戏启动命令
    ///
    /// 完整流程：
    /// 1. 检查是否需要手动确认（全局 require_confirmation + 单游戏 confirm_before_sync）
    /// 2. 执行 pre_launch_sync（比对本地/远端存档，按时间戳取新覆盖旧）
    /// 3. 启动游戏（通过 Steam 协议）
    /// 4. 如果配置了 exe_name 且自动备份开启，启动 process_watcher 监控进程
    ///
    /// 注意：需要手动确认时，前端应先调用此命令获取同步预览，确认后再调用 launch_game_after_sync
    #[tauri::command]
    pub async fn launch_game_with_sync(
        app: AppHandle,
        game_id: String,
    ) -> Result<LaunchWithSyncResult, String> {
        let config = crate::config::load_config(&app).map_err(|e| e.to_string())?;
        let game = config
            .games
            .iter()
            .find(|g| g.id == game_id)
            .ok_or_else(|| "未找到游戏".to_string())?
            .clone();

        // 1. 执行启动前同步
        let sync_result = crate::backup::sync::pre_launch_sync(&app, &game_id)
            .await
            .map_err(|e| e.to_string())?;

        log::info!(
            "[启动] 游戏 {} 同步结果: {:?} — {}",
            game.name,
            sync_result.action,
            sync_result.message
        );

        // 2. 启动游戏
        let launched = match game.steam_appid {
            Some(appid) => {
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
                true
            }
            None => {
                log::warn!("[启动] 游戏 {} 未配置 Steam AppID，跳过自动启动", game.name);
                false
            }
        };

        // 3. 判断是否需要启动进程监控
        let auto_backup_enabled = game.auto_backup_enabled.unwrap_or(config.settings.auto_backup);
        let watcher_active = if auto_backup_enabled {
            if let Some(ref exe_name) = game.exe_name {
                let exe_name = exe_name.clone();
                let app_handle = app.clone();
                let game_id_clone = game_id.clone();
                let game_name = game.name.clone();

                super::process_watcher::ProcessWatcher::start(
                    exe_name,
                    Box::new(move || {
                        // 进程退出后自动触发增量备份
                        log::info!("[自动备份] 游戏 {} 进程退出，开始增量备份", game_name);
                        let rt = tokio::runtime::Handle::current();
                        rt.spawn(async move {
                            match crate::backup::incremental::perform_incremental_backup(
                                &app_handle,
                                &game_id_clone,
                            )
                            .await
                            {
                                Ok(result) => {
                                    log::info!("[自动备份] {}: {}", game_name, result.message);
                                    // 备份成功后上传远端 manifest
                                    if let Ok(manifest) =
                                        crate::backup::manifest::get_latest_manifest(&app_handle, &game_id_clone)
                                    {
                                        if let Some(m) = manifest {
                                            let _ = crate::backup::remote_manifest::upload_remote_manifest(
                                                &app_handle,
                                                &game_id_clone,
                                                m.files,
                                                m.timestamp,
                                            )
                                            .await;
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!("[自动备份] {} 失败: {}", game_name, e);
                                }
                            }
                        });
                    }),
                );
                true
            } else {
                false
            }
        } else {
            false
        };

        Ok(LaunchWithSyncResult {
            launched,
            sync_message: sync_result.message,
            sync_files_changed: sync_result.files_changed,
            watcher_active,
        })
    }

    /// 更新单个游戏的设置（exe_name、auto_backup_enabled、confirm_before_sync）
    ///
    /// 前端在游戏详情页调用，允许用户手动配置 exe 名称和覆盖全局的自动备份/确认设置
    #[tauri::command]
    pub fn update_game_settings(
        app: AppHandle,
        game_id: String,
        exe_name: Option<String>,
        auto_backup_enabled: Option<bool>,
        confirm_before_sync: Option<bool>,
    ) -> Result<(), String> {
        let mut config = crate::config::load_config(&app).map_err(|e| e.to_string())?;
        let game = config
            .games
            .iter_mut()
            .find(|g| g.id == game_id)
            .ok_or_else(|| "未找到游戏".to_string())?;

        if let Some(name) = exe_name {
            game.exe_name = Some(name);
        }
        if let Some(enabled) = auto_backup_enabled {
            game.auto_backup_enabled = Some(enabled);
        }
        if let Some(confirm) = confirm_before_sync {
            game.confirm_before_sync = Some(confirm);
        }

        crate::config::save_config(&app, &config).map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// 重新导出数据库命令以便在 lib.rs 中统一注册
pub use db::commands as db_commands;
/// 重新导出 PCGamingWiki 命令
pub use pcgw::commands as pcgw_commands;
