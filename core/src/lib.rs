// lib.rs - Tauri 应用入口与模块导出
// 注册所有 Tauri Commands 并初始化应用

pub mod alist;
pub mod backup;
pub mod config;
pub mod game;
pub mod storage;
pub mod utils;

use tauri::Manager;

/// 运行 Tauri 应用
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 调试模式下启用日志插件
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            // 初始化配置目录
            let app_local_data = app.handle().path().app_local_data_dir()?;
            std::fs::create_dir_all(&app_local_data)?;
            std::fs::create_dir_all(app_local_data.join("manifests"))?;
            std::fs::create_dir_all(app_local_data.join("cache").join("logos"))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Alist 相关命令
            alist::commands::alist_login,
            alist::commands::alist_list_dir,
            alist::commands::alist_upload,
            alist::commands::alist_mkdir,
            // 统一存储适配器通用交互命令 (专专于网盘向导引导与自定义云端备份)
            storage::commands::storage_test_connection,
            storage::commands::storage_list_dir,
            storage::commands::storage_refresh_all_tokens,
            // 夸克 TV 扫码登录命令
            storage::commands::quark_tv_get_qr_code,
            storage::commands::quark_tv_poll_qr,
            storage::commands::quark_tv_exchange,
            // 游戏相关命令
            game::commands::get_games,
            game::commands::add_game,
            game::commands::remove_game,
            game::commands::scan_game_saves,
            game::commands::get_game_logo,
            game::commands::select_and_extract_exe_icon,
            game::commands::save_custom_logo,
            game::commands::get_db_game_logo,
            game::commands::launch_game,
            // 游戏数据库相关命令
            game::db_commands::get_game_db,
            game::db_commands::search_game_db,
            game::db_commands::upsert_game_db_entry,
            game::db_commands::remove_game_db_entry,
            game::db_commands::export_game_db,
            game::db_commands::import_game_db,
            game::db_commands::create_game_from_db,
            // PCGamingWiki 相关命令
            game::pcgw_commands::search_pcgw_games,
            game::pcgw_commands::fetch_pcgw_save_paths,
            game::pcgw_commands::search_steam_store_cmd,
            // 备份相关命令
            backup::commands::backup_full,
            backup::commands::backup_incremental,
            backup::commands::restore_backup,
            backup::commands::get_backup_history,
            backup::commands::list_remote_backups,
            backup::commands::restore_remote_backup,
            // 配置相关命令
            config::commands::load_config,
            config::commands::save_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
