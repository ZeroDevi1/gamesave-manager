// backup/mod.rs - 备份引擎入口
pub mod full;
pub mod incremental;
pub mod manifest;
pub mod restore;

use serde::{Deserialize, Serialize};

/// 备份类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupType {
    Full,
    Incremental,
}

/// 备份结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupResult {
    pub success: bool,
    pub message: String,
    pub files_backed_up: usize,
    pub timestamp: String,
}

/// 恢复结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResult {
    pub success: bool,
    pub message: String,
}

/// Tauri Commands 导出
pub mod commands {
    use super::*;
    use tauri::AppHandle;

    /// 全量备份命令
    #[tauri::command]
    pub async fn backup_full(
        app: AppHandle,
        game_id: String,
    ) -> Result<BackupResult, String> {
        full::perform_full_backup(&app, &game_id).await.map_err(|e| e.to_string())
    }

    /// 增量备份命令
    #[tauri::command]
    pub async fn backup_incremental(
        app: AppHandle,
        game_id: String,
    ) -> Result<BackupResult, String> {
        incremental::perform_incremental_backup(&app, &game_id).await.map_err(|e| e.to_string())
    }

    /// 恢复备份命令
    #[tauri::command]
    pub async fn restore_backup(
        app: AppHandle,
        game_id: String,
        backup_timestamp: String,
    ) -> Result<RestoreResult, String> {
        restore::perform_restore(&app, &game_id, &backup_timestamp).await.map_err(|e| e.to_string())
    }

    /// 获取备份历史
    #[tauri::command]
    pub async fn get_backup_history(
        app: AppHandle,
        game_id: String,
    ) -> Result<Vec<manifest::BackupManifest>, String> {
        manifest::load_manifests(&app, &game_id).map_err(|e| e.to_string())
    }
}
