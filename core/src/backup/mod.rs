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

/// 远程备份条目（Alist 网盘上的 ZIP 文件）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteBackupEntry {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified: Option<String>,
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

    /// 恢复备份命令（基于本地 manifest 时间戳）
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

    /// 列出远程备份版本（Alist 网盘上的 ZIP 文件列表）
    #[tauri::command]
    pub async fn list_remote_backups(
        app: AppHandle,
        game_id: String,
    ) -> Result<Vec<RemoteBackupEntry>, String> {
        let config = crate::config::load_config(&app).map_err(|e| e.to_string())?;
        let game = config
            .games
            .iter()
            .find(|g| g.id == game_id)
            .ok_or_else(|| "未找到游戏".to_string())?;

        let base_remote_path = config.get_game_remote_path(game);
        let alist = config.alist.ok_or("未配置 Alist")?;
        let token = alist.token.ok_or("未登录 Alist")?;
        let remote_dir = format!("{}/full/", base_remote_path.trim_end_matches('/'));

        let entries = crate::alist::fs::list_dir(&alist.base_url, &token, &remote_dir)
            .await
            .map_err(|e| e.to_string())?;

        // 只保留 ZIP 文件
        let backups: Vec<RemoteBackupEntry> = entries
            .into_iter()
            .filter(|e| !e.is_dir && e.name.ends_with(".zip"))
            .map(|e| RemoteBackupEntry {
                name: e.name,
                path: e.path,
                size: e.size,
                modified: e.modified,
            })
            .collect();

        Ok(backups)
    }

    /// 从远程备份 ZIP 文件恢复存档
    #[tauri::command]
    pub async fn restore_remote_backup(
        app: AppHandle,
        game_id: String,
        remote_zip_path: String,
    ) -> Result<RestoreResult, String> {
        restore::perform_restore_from_remote(&app, &game_id, &remote_zip_path)
            .await
            .map_err(|e| e.to_string())
    }
}
