use std::io::Write;
// backup/full.rs - 全量备份（打包压缩 + 上传）
use super::manifest::{BackupManifest, FileEntry};
use super::{BackupResult, BackupType};
use crate::utils::hash;
use chrono::Utc;
use std::path::Path;
use tauri::AppHandle;
use walkdir::WalkDir;

/// 执行全量备份
pub async fn perform_full_backup(app: &AppHandle, game_id: &str) -> anyhow::Result<BackupResult> {
    // 读取配置
    let config = crate::config::load_config(app)?;
    let game = config
        .games
        .iter()
        .find(|g| g.id == game_id)
        .ok_or_else(|| anyhow::anyhow!("未找到游戏: {}", game_id))?;

    let timestamp = Utc::now();
    let ts_str = timestamp.format("%Y%m%d_%H%M%S").to_string();
    let zip_name = format!("{}_{}.zip", game.name.replace(' ', "_"), ts_str);

    // 本地临时 zip 路径
    let temp_dir = std::env::temp_dir().join("gamesave-manager");
    std::fs::create_dir_all(&temp_dir)?;
    let zip_path = temp_dir.join(&zip_name);

    // 打包存档目录
    let mut files = Vec::new();
    let zip_file = std::fs::File::create(&zip_path)?;
    let mut zip = zip::ZipWriter::new(zip_file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for save_path_str in &game.save_paths {
        let save_path = Path::new(save_path_str);
        if !save_path.exists() {
            continue;
        }

        if save_path.is_file() {
            let rel_path = save_path.file_name().unwrap().to_string_lossy().to_string();
            let content = std::fs::read(save_path)?;
            let sha256 = hash::sha256_string(&content);
            let meta = std::fs::metadata(save_path)?;
            let modified_time = meta.modified()?.into();

            zip.start_file(&rel_path, options)?;
            zip.write_all(&content)?;

            files.push(FileEntry {
                relative_path: rel_path,
                size: meta.len(),
                modified_time,
                sha256,
            });
        } else {
            for entry in WalkDir::new(save_path) {
                let entry = entry?;
                if entry.file_type().is_dir() {
                    continue;
                }
                let path = entry.path();
                let rel_path = path
                    .strip_prefix(save_path)?
                    .to_string_lossy()
                    .replace('\\', "/");

                let content = std::fs::read(path)?;
                let sha256 = hash::sha256_string(&content);
                let meta = entry.metadata()?;
                let modified_time = meta.modified()?.into();

                zip.start_file(&rel_path, options)?;
                zip.write_all(&content)?;

                files.push(FileEntry {
                    relative_path: rel_path,
                    size: meta.len(),
                    modified_time,
                    sha256,
                });
            }
        }
    }

    zip.finish()?;

    // 计算压缩包 SHA256
    let zip_bytes = std::fs::read(&zip_path)?;
    let _zip_sha256 = hash::sha256_string(&zip_bytes);

    // 上传到 Alist（引入动态路由拼接，无缝支持自定义备份根路径）
    let base_remote_path = config.get_game_remote_path(game);
    let remote_dir = format!("{}/{}/full/", base_remote_path.trim_end_matches('/'), game_id);
    let remote_path = format!("{}{}", remote_dir, zip_name);

    if let Some(alist) = config.alist {
        if let Some(token) = alist.token {
            crate::alist::fs::mkdir(&alist.base_url, &token, &remote_dir).await?;
            crate::alist::fs::upload_file(
                &alist.base_url,
                &token,
                zip_path.to_str().unwrap(),
                &remote_path,
            ).await?;
        }
    }

    // 保存 manifest
    let manifest = BackupManifest {
        game_id: game_id.to_string(),
        backup_type: BackupType::Full,
        timestamp,
        files,
        target_path: remote_path.clone(),
        zip_file: Some(remote_path),
    };
    super::manifest::save_manifest(app, &manifest)?;

    // 清理本地临时文件
    let _ = std::fs::remove_file(&zip_path);

    Ok(BackupResult {
        success: true,
        message: "全量备份完成".to_string(),
        files_backed_up: manifest.files.len(),
        timestamp: ts_str,
    })
}
