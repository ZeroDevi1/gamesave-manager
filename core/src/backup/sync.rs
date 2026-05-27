// backup/sync.rs - 启动前存档同步
//
// 在启动游戏前，自动比对本地存档与远端 manifest，按时间戳决定同步方向：
// - 远端更新 → 下载远端存档覆盖本地
// - 本地更新 → 上传本地存档到远端
// - 相同 → 跳过同步

use super::manifest::{BackupManifest, FileEntry};
use super::remote_manifest;
use super::{BackupType};
use crate::utils::hash;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tauri::AppHandle;
use walkdir::WalkDir;

/// 同步方向
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncAction {
    /// 无远端 manifest 或无需同步
    Skipped,
    /// 远端更新，已拉取到本地
    Pulled,
    /// 本地更新，已推送到远端
    Pushed,
}

/// 同步结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    /// 执行的同步动作
    pub action: SyncAction,
    /// 人类可读的结果描述
    pub message: String,
    /// 同步的文件数量
    pub files_changed: usize,
}

/// 扫描本地存档文件，返回 (relative_path → FileEntry) 映射
fn scan_local_saves(
    game: &crate::config::model::GameConfig,
) -> anyhow::Result<HashMap<String, FileEntry>> {
    let mut local_files: HashMap<String, FileEntry> = HashMap::new();

    for save_path_str in &game.save_paths {
        let save_path = Path::new(save_path_str);
        if !save_path.exists() {
            continue;
        }

        let entries: Vec<_> = if save_path.is_file() {
            vec![save_path.to_path_buf()]
        } else {
            WalkDir::new(save_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .map(|e| e.path().to_path_buf())
                .collect()
        };

        for path in &entries {
            let rel_path = if save_path.is_file() {
                save_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            } else {
                path.strip_prefix(save_path)?
                    .to_string_lossy()
                    .replace('\\', "/")
            };

            let meta = path.metadata()?;
            let modified_time: DateTime<Utc> = meta.modified()?.into();
            let content = std::fs::read(path)?;
            let sha256 = hash::sha256_string(&content);

            local_files.insert(
                rel_path.clone(),
                FileEntry {
                    relative_path: rel_path,
                    size: meta.len(),
                    modified_time,
                    sha256,
                },
            );
        }
    }

    Ok(local_files)
}

/// 启动前同步：比对本地存档与远端 manifest，按时间戳决定同步方向
///
/// # 参数
/// * `app` - Tauri 应用句柄
/// * `game_id` - 游戏唯一标识
///
/// # 返回
/// `SyncResult` 描述同步动作和结果
pub async fn pre_launch_sync(app: &AppHandle, game_id: &str) -> anyhow::Result<SyncResult> {
    let config = crate::config::load_config(app)?;
    let game = config
        .games
        .iter()
        .find(|g| g.id == game_id)
        .ok_or_else(|| anyhow::anyhow!("未找到游戏: {}", game_id))?;

    // 1. 下载远端 manifest
    let remote_manifest = remote_manifest::download_remote_manifest(app, game_id).await?;

    let remote_manifest = match remote_manifest {
        Some(m) => m,
        None => {
            log::info!("[启动同步] 游戏 {} 无远端清单，跳过同步", game.name);
            return Ok(SyncResult {
                action: SyncAction::Skipped,
                message: "远端无备份清单，跳过同步".to_string(),
                files_changed: 0,
            });
        }
    };

    // 2. 扫描本地存档
    let local_files = scan_local_saves(game)?;

    // 3. 获取本地最新 manifest 的时间戳
    let local_manifest = super::manifest::get_latest_manifest(app, game_id)?;
    let local_timestamp = local_manifest.as_ref().map(|m| m.timestamp);

    // 4. 比对时间戳决定同步方向
    let remote_ts = remote_manifest.timestamp;

    let should_pull = match local_timestamp {
        Some(local_ts) => remote_ts > local_ts,
        None => true, // 本地无备份，远端有 → 拉取
    };

    if should_pull {
        // 远端更新 → 拉取远端存档覆盖本地
        let changed = pull_from_remote(app, game, &remote_manifest.files).await?;
        log::info!(
            "[启动同步] 游戏 {} 从远端拉取了 {} 个文件",
            game.name,
            changed
        );
        Ok(SyncResult {
            action: SyncAction::Pulled,
            message: format!("远端存档更新（{}），已拉取 {} 个文件到本地", remote_ts.format("%Y-%m-%d %H:%M"), changed),
            files_changed: changed,
        })
    } else {
        // 本地更新或相同 → 检查是否有实际差异需要推送
        let remote_files: HashMap<String, FileEntry> = remote_manifest
            .files
            .iter()
            .map(|f| (f.relative_path.clone(), f.clone()))
            .collect();

        let mut diff_count = 0usize;
        for (rel_path, local_entry) in &local_files {
            let need_push = match remote_files.get(rel_path) {
                Some(remote_entry) => {
                    local_entry.modified_time > remote_entry.modified_time
                        || local_entry.sha256 != remote_entry.sha256
                }
                None => true, // 远端无此文件
            };
            if need_push {
                diff_count += 1;
            }
        }

        if diff_count == 0 {
            return Ok(SyncResult {
                action: SyncAction::Skipped,
                message: "本地与远端存档一致，无需同步".to_string(),
                files_changed: 0,
            });
        }

        // 本地更新 → 推送到远端
        push_to_remote(app, game, &local_files).await?;
        log::info!(
            "[启动同步] 游戏 {} 推送了 {} 个文件到远端",
            game.name,
            diff_count
        );
        Ok(SyncResult {
            action: SyncAction::Pushed,
            message: format!("本地存档更新，已推送 {} 个文件到远端", diff_count),
            files_changed: diff_count,
        })
    }
}

/// 从远端拉取存档文件覆盖本地
///
/// 逐个下载远端清单中列出的文件到本地存档路径
async fn pull_from_remote(
    app: &AppHandle,
    game: &crate::config::model::GameConfig,
    remote_files: &[FileEntry],
) -> anyhow::Result<usize> {
    let config = crate::config::load_config(app)?;
    let backend = crate::storage::get_storage_backend(&config)?;
    let base_remote_path = config.get_game_remote_path(game);

    let mut changed = 0usize;

    // 取第一个 save_path 作为本地目标目录（多数游戏只有一个存档路径）
    let local_base = game
        .save_paths
        .first()
        .ok_or_else(|| anyhow::anyhow!("游戏无存档路径"))?;

    for entry in remote_files {
        // 远端文件路径：使用最近一次增量备份目录下的相对路径
        // 由于远端 manifest 记录的是文件清单，我们需要从远端下载实际文件
        // 使用 base_remote_path 下的文件结构
        let remote_file_path = format!(
            "{}/{}",
            base_remote_path.trim_end_matches('/'),
            entry.relative_path
        );
        let local_file_path = Path::new(local_base).join(&entry.relative_path);

        // 确保本地目录存在
        if let Some(parent) = local_file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        match backend
            .download_file(&remote_file_path, local_file_path.to_str().unwrap())
            .await
        {
            Ok(_) => {
                changed += 1;
            }
            Err(e) => {
                log::warn!(
                    "[启动同步] 下载失败 {}: {}",
                    entry.relative_path,
                    e
                );
                // 单文件失败不中断整体同步
            }
        }
    }

    // 更新本地 manifest 以匹配远端状态
    let manifest = BackupManifest {
        game_id: game.id.clone(),
        backup_type: BackupType::Incremental,
        timestamp: Utc::now(),
        files: remote_files.to_vec(),
        target_path: base_remote_path.clone(),
        zip_file: None,
    };
    super::manifest::save_manifest(app, &manifest)?;

    Ok(changed)
}

/// 推送本地存档到远端
///
/// 逐个上传本地变更文件到远端，并更新远端 manifest
async fn push_to_remote(
    app: &AppHandle,
    game: &crate::config::model::GameConfig,
    local_files: &HashMap<String, FileEntry>,
) -> anyhow::Result<()> {
    let config = crate::config::load_config(app)?;
    let backend = crate::storage::get_storage_backend(&config)?;
    let base_remote_path = config.get_game_remote_path(game);
    let timestamp = Utc::now();
    let ts_str = timestamp.format("%Y%m%d_%H%M%S").to_string();

    let local_base = game
        .save_paths
        .first()
        .ok_or_else(|| anyhow::anyhow!("游戏无存档路径"))?;

    for entry in local_files.values() {
        let local_path = Path::new(local_base).join(&entry.relative_path);
        if !local_path.exists() {
            continue;
        }

        let remote_path = format!(
            "{}/incremental/{}/{}",
            base_remote_path.trim_end_matches('/'),
            ts_str,
            entry.relative_path
        );

        // 确保远端目录存在
        let remote_parent = Path::new(&remote_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| base_remote_path.clone());
        let _ = backend.mkdir(&remote_parent).await;

        if let Err(e) = backend
            .upload_file(local_path.to_str().unwrap(), &remote_path)
            .await
        {
            log::warn!("[启动同步] 上传失败 {}: {}", entry.relative_path, e);
        }
    }

    // 更新远端 manifest
    let files_vec: Vec<FileEntry> = local_files.values().cloned().collect();
    remote_manifest::upload_remote_manifest(app, &game.id, files_vec, timestamp).await?;

    Ok(())
}
