// backup/incremental.rs - 增量备份（比对哈希/时间戳 + 差异上传）
use super::manifest::{BackupManifest, FileEntry};
use super::{BackupResult, BackupType};
use crate::utils::hash;
use chrono::Utc;
use std::path::Path;
use tauri::AppHandle;
use walkdir::WalkDir;

/// 执行增量备份
pub async fn perform_incremental_backup(
    app: &AppHandle,
    game_id: &str,
) -> anyhow::Result<BackupResult> {
    let config = crate::config::load_config(app)?;
    let game = config
        .games
        .iter()
        .find(|g| g.id == game_id)
        .ok_or_else(|| anyhow::anyhow!("未找到游戏: {}", game_id))?;

    let timestamp = Utc::now();
    let ts_str = timestamp.format("%Y%m%d_%H%M%S").to_string();

    // 读取上次备份清单
    let last_manifest = super::manifest::get_latest_manifest(app, game_id)?;
    let mut last_files: std::collections::HashMap<String, FileEntry> = last_manifest
        .as_ref()
        .map(|m| m.files.iter().map(|f| (f.relative_path.clone(), f.clone())).collect())
        .unwrap_or_default();

    let mut new_files = Vec::new();
    let mut changed_count = 0usize;

    // 扫描当前存档目录
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
                save_path.file_name().unwrap().to_string_lossy().to_string()
            } else {
                path.strip_prefix(save_path)?
                    .to_string_lossy()
                    .replace('\\', "/")
            };

            let meta = path.metadata()?;
            let modified_time: chrono::DateTime<Utc> = meta.modified()?.into();
            let content = std::fs::read(path)?;
            let sha256 = hash::sha256_string(&content);

            let need_upload = match last_files.get(&rel_path) {
                Some(last) => {
                    // 双重校验：先比对时间戳，时间戳相同再比对 SHA256
                    last.modified_time != modified_time || last.sha256 != sha256
                }
                None => true, // 新增文件
            };

            if need_upload {
                changed_count += 1;

                // 上传到 Alist（引入动态路由拼接，无缝支持自定义备份根路径）
                let base_remote_path = config.get_game_remote_path(game);
                let remote_dir = format!(
                    "{}/incremental/{}/",
                    base_remote_path.trim_end_matches('/'),
                    ts_str
                );
                let remote_path = format!("{}{}", remote_dir, rel_path);

                // 通过存储适配器工厂动态获取激活的物理云端后端实例
                let backend = crate::storage::get_storage_backend(&config)?;

                // 确保远程层级目录已物理创建（忽略文件夹早已存在的静默成功）
                let remote_parent = Path::new(&remote_path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| remote_dir.clone());
                let _ = backend.mkdir(&remote_parent).await;

                // 调用统一的 Trait 抽象方法上传增量变更物理文件
                backend.upload_file(path.to_str().unwrap(), &remote_path).await?;
            }

            new_files.push(FileEntry {
                relative_path: rel_path.clone(),
                size: meta.len(),
                modified_time,
                sha256,
            });

            // 从旧清单中移除，最后剩下的就是已删除文件
            last_files.remove(&rel_path);
        }
    }

    // 保存新 manifest（已删除文件不再包含，达到标记删除的效果，同时使用动态根目录路由）
    let base_remote_path = config.get_game_remote_path(game);
    let remote_base = format!(
        "{}/incremental/{}",
        base_remote_path.trim_end_matches('/'),
        ts_str
    );
    let manifest = BackupManifest {
        game_id: game_id.to_string(),
        backup_type: BackupType::Incremental,
        timestamp,
        files: new_files,
        target_path: remote_base,
        zip_file: None,
    };
    super::manifest::save_manifest(app, &manifest)?;

    // 备份成功后自动上传远端 manifest（供后续启动前同步比对）
    if changed_count > 0 {
        if let Err(e) = super::remote_manifest::upload_remote_manifest(
            app,
            game_id,
            manifest.files.clone(),
            timestamp,
        )
        .await
        {
            log::warn!("[增量备份] 远端清单上传失败（不影响备份本身）: {}", e);
        }
    }

    Ok(BackupResult {
        success: true,
        message: format!("增量备份完成，上传 {} 个变更文件", changed_count),
        files_backed_up: changed_count,
        timestamp: ts_str,
    })
}
