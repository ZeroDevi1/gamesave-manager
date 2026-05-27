use std::io::Write;
// backup/full.rs - 全量备份（打包压缩 + 上传）
use super::manifest::{BackupManifest, FileEntry};
use super::{BackupResult, BackupType};
use crate::utils::hash;
use chrono::Utc;
use std::path::Path;
use tauri::AppHandle;
use walkdir::WalkDir;

/// 辅助函数：清洗（Sanitize）备份压缩包的文件名
///
/// # 核心设计与规避
/// 1. Windows 本地系统限制：Windows 本地 NTFS 文件系统不支持文件名中夹带冒号 `:` 等非法字符（这会导致系统
///    将其解释为 NTFS 备用数据流，造成文件隐藏或物理读写失败）。
/// 2. 百度网盘等云端限制：百度网盘等主流云盘在调用文件创建 API 时，如果文件名中包含冒号、问号等，会报 `-errno: -7`
///    导致整个上传链路崩溃。
/// 本函数通过将文件名中所有的特殊高危字符和空格统一替换为下划线 `_`，实现了本地和云端的双重安全性保障。
fn sanitize_filename(name: &str) -> String {
    name.chars().map(|c| {
        match c {
            ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\\' | '/' | ' ' => '_',
            _ => c
        }
    }).collect()
}

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
    let safe_game_name = sanitize_filename(&game.name);
    let zip_name = format!("{}_{}.zip", safe_game_name, ts_str);

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

    // 计算清洗后的云端备份主路径与具体全量备份子文件夹/目标文件名
    let base_remote_path = config.get_game_remote_path(game);
    let remote_dir = format!("{}/full/", base_remote_path.trim_end_matches('/'));
    let remote_path = format!("{}{}", remote_dir, zip_name);

    // 通过存储适配器工厂动态获取激活的物理云端后端实例，实现与具体底层协议完全解耦
    let backend = crate::storage::get_storage_backend(&config)?;
    
    // 调用统一的 Trait 抽象方法级联创建云端物理文件夹
    backend.mkdir(&remote_dir).await?;
    
    // 调用统一的 Trait 抽象方法将本地物理存档 ZIP 上传至云端
    backend.upload_file(zip_path.to_str().unwrap(), &remote_path).await?;

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

    // 备份成功后自动上传远端 manifest（供后续启动前同步比对）
    if let Err(e) = super::remote_manifest::upload_remote_manifest(
        app,
        game_id,
        manifest.files.clone(),
        timestamp,
    )
    .await
    {
        log::warn!("[全量备份] 远端清单上传失败（不影响备份本身）: {}", e);
    }

    // 清理本地临时文件
    let _ = std::fs::remove_file(&zip_path);

    Ok(BackupResult {
        success: true,
        message: "全量备份完成".to_string(),
        files_backed_up: manifest.files.len(),
        timestamp: ts_str,
    })
}
