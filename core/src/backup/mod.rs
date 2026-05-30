// backup/mod.rs - 备份引擎入口
pub mod full;
pub mod incremental;
pub mod manifest;
pub mod remote_manifest;
pub mod restore;
pub mod sync;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
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

/// 根据全局设置清理旧的全量/增量远程备份，防止云端存储无限膨胀
///
/// # 参数
/// * `app` - Tauri 应用句柄
/// * `game` - 游戏配置
/// * `config` - 应用配置（含 settings.backup_retention_limit）
/// * `backend` - 已初始化的云端存储后端
async fn cleanup_old_backups(
    app: &AppHandle,
    game: &crate::config::model::GameConfig,
    config: &crate::config::model::AppConfig,
    backend: &crate::storage::StorageBackend,
) {
    let limit = match config.settings.backup_retention_limit {
        Some(n) if n > 0 => n as usize,
        _ => return,
    };
    let base_remote_path = config.get_game_remote_path(game);
    // 1. 清理全量备份 ZIP
    let full_dir = format!("{}/full/", base_remote_path.trim_end_matches('/'));
    if let Ok(entries) = backend.list_dir(&full_dir).await {
        let mut zips: Vec<_> = entries
            .into_iter()
            .filter(|e| !e.is_dir && e.name.ends_with(".zip"))
            .collect();
        zips.sort_by(|a, b| b.modified.cmp(&a.modified));
        if zips.len() > limit {
            for old in &zips[limit..] {
                if let Err(e) = backend.delete(&old.path).await {
                    log::warn!("[清理旧备份] 删除全量备份失败 {}: {}", old.path, e);
                } else {
                    log::info!("[清理旧备份] 已删除旧全量备份: {}", old.path);
                }
            }
        }
    }
    // 2. 清理增量备份目录
    let inc_dir = format!("{}/incremental/", base_remote_path.trim_end_matches('/'));
    if let Ok(entries) = backend.list_dir(&inc_dir).await {
        let mut dirs: Vec<_> = entries.into_iter().filter(|e| e.is_dir).collect();
        // 增量目录名即时间戳，按名称降序等同于按时间降序
        dirs.sort_by(|a, b| b.name.cmp(&a.name));
        if dirs.len() > limit {
            for old in &dirs[limit..] {
                let dir_path = format!("{}{}/", inc_dir.trim_end_matches('/'), old.path.trim_start_matches('/'));
                if let Err(e) = backend.delete(&dir_path).await {
                    log::warn!("[清理旧备份] 删除增量目录失败 {}: {}", dir_path, e);
                } else {
                    log::info!("[清理旧备份] 已删除旧增量目录: {}", dir_path);
                }
            }
        }
    }
    // 3. 同步清理本地 manifest 中已不存在的旧记录
    if let Ok(manifests) = manifest::load_manifests(app, &game.id) {
        let keep_count = limit.max(1);
        if manifests.len() > keep_count {
            let mut sorted = manifests;
            sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            let pruned: Vec<_> = sorted.into_iter().take(keep_count).collect();
            let _ = save_manifests_raw(app, &game.id, &pruned);
        }
    }
}
/// 直接覆写某游戏的本地 manifest 列表（用于清理旧记录）
fn save_manifests_raw(
    app: &AppHandle,
    game_id: &str,
    manifests: &[manifest::BackupManifest],
) -> anyhow::Result<()> {
    let dir = app.path().app_local_data_dir()?.join("manifests");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", game_id));
    let json = serde_json::to_string_pretty(manifests)?;
    std::fs::write(&path, json)?;
    Ok(())
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
    /// 目录不存在时静默返回空列表，避免对未备份过的游戏报错
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
        let remote_dir = format!("{}/full/", base_remote_path.trim_end_matches('/'));

        // 通过存储适配器工厂动态获取激活的物理云端后端实例
        let backend = crate::storage::get_storage_backend(&config).map_err(|e| e.to_string())?;

        let entries = match backend.list_dir(&remote_dir).await {
            Ok(entries) => entries,
            Err(e) => {
                let msg = e.to_string().to_lowercase();
                // 兼容各云存储驱动抛出的“未找到/路径不存在”错误，静默返回空列表
                if msg.contains("not found") || msg.contains("object not found") || msg.contains("路径不存在") || msg.contains("404") {
                    return Ok(Vec::new());
                }
                return Err(e.to_string());
            }
        };

        // 只保留 ZIP 文件
        let backups: Vec<RemoteBackupEntry> = entries
            .into_iter()
            .filter(|e| !e.is_dir && e.name.ends_with(".zip"))
            .map(|e| RemoteBackupEntry {
                name: e.name,
                path: e.path,
                size: e.size as u64,
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

    /// 检查所有游戏是否有未备份的存档变更
    ///
    /// 遍历所有游戏的 save_paths，对比最新 manifest 中的 SHA256 + mtime，
    /// 返回有变更的游戏 (游戏ID, 变更文件数) 列表
    #[tauri::command]
    pub async fn check_all_games_for_changes(
        app: AppHandle,
    ) -> Result<Vec<(String, usize)>, String> {
        let config = crate::config::load_config(&app).map_err(|e| e.to_string())?;
        let mut changed_games: Vec<(String, usize)> = Vec::new();

        for game in &config.games {
            // 获取最新备份清单
            let last_manifest = super::manifest::get_latest_manifest(&app, &game.id)
                .map_err(|e| e.to_string())?;
            
            // 构建旧文件索引：relative_path → FileEntry
            let last_files: std::collections::HashMap<String, super::manifest::FileEntry> = last_manifest
                .as_ref()
                .map(|m| {
                    m.files
                        .iter()
                        .map(|f| (f.relative_path.clone(), f.clone()))
                        .collect()
                })
                .unwrap_or_default();

            let mut changed_count = 0usize;

            // 扫描每个存档路径
            for save_path_str in &game.save_paths {
                for save_path in crate::utils::path::resolve_save_paths(save_path_str) {
                    // 获取文件列表（如果是目录则递归）
                    let entries: Vec<std::path::PathBuf> = if save_path.is_file() {
                        vec![save_path.to_path_buf()]
                    } else {
                        walkdir::WalkDir::new(&save_path)
                            .into_iter()
                            .filter_map(|e| e.ok())
                            .filter(|e| e.file_type().is_file())
                            .map(|e| e.path().to_path_buf())
                            .collect()
                    };
                    for path in &entries {
                        // 计算相对路径
                        let rel_path = if save_path.is_file() {
                            save_path
                                .file_name()
                                .unwrap()
                                .to_string_lossy()
                                .to_string()
                        } else {
                            match path.strip_prefix(&save_path) {
                                Ok(p) => p.to_string_lossy().replace('\\', "/"),
                                Err(_) => continue,
                            }
                        };
                        // 获取文件元信息
                        let meta = match path.metadata() {
                            Ok(m) => m,
                            Err(_) => continue,
                        };
                        let modified_time: chrono::DateTime<chrono::Utc> =
                            match meta.modified() {
                                Ok(t) => t.into(),
                                Err(_) => continue,
                            };
                        // 计算 SHA256
                        let content = match std::fs::read(path) {
                            Ok(c) => c,
                            Err(_) => continue,
                        };
                        let sha256 = crate::utils::hash::sha256_string(&content);
                        // 双重校验：对比 mtime 和 SHA256
                        let has_changed = match last_files.get(&rel_path) {
                            Some(last) => {
                                last.modified_time != modified_time || last.sha256 != sha256
                            }
                            None => true, // 新文件
                        };
                        if has_changed {
                            changed_count += 1;
                        }
                    }
                }
            }

            if changed_count > 0 {
                log::info!(
                    "[变更检测] {}: {} 个文件有变更",
                    game.name,
                    changed_count
                );
                changed_games.push((game.id.clone(), changed_count));
            }
        }

        log::info!(
            "[变更检测] 完成，{} 个游戏有变更",
            changed_games.len()
        );

        Ok(changed_games)
    }

    /// 一键增量备份所有有变更的游戏
    ///
    /// 先检测所有有变更的游戏，然后对每个执行增量备份
    /// 返回备份结果摘要：(游戏ID, 成功与否, 消息)
    #[tauri::command]
    pub async fn backup_all_changed_games(
        app: AppHandle,
    ) -> Result<Vec<(String, bool, String)>, String> {
        // 先检测变更
        let changed = check_all_games_for_changes(
            app.clone(),
        )
        .await?;

        if changed.is_empty() {
            return Ok(Vec::new());
        }

        let mut results: Vec<(String, bool, String)> = Vec::new();
        for (game_id, changed_count) in &changed {
            log::info!(
                "[批量备份] 开始备份: game_id={}, 变更文件数={}",
                game_id,
                changed_count
            );
            match incremental::perform_incremental_backup(&app, game_id).await {
                Ok(result) => {
                    log::info!("[批量备份] {}: 成功 — {}", game_id, result.message);
                    results.push((game_id.clone(), true, result.message));
                }
                Err(e) => {
                    log::error!("[批量备份] {}: 失败 — {}", game_id, e);
                    results.push((game_id.clone(), false, e.to_string()));
                }
            }
        }

        log::info!("[批量备份] 完成，共处理 {} 个游戏", results.len());
        Ok(results)
    }
}
