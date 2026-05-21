// backup/restore.rs - 全量恢复（下载 + 解压覆盖）
use super::RestoreResult;
use std::path::Path;
use tauri::AppHandle;

/// 执行全量恢复（基于本地 manifest 的时间戳）
pub async fn perform_restore(
    app: &AppHandle,
    game_id: &str,
    backup_timestamp: &str,
) -> anyhow::Result<RestoreResult> {
    let config = crate::config::load_config(app)?;
    let game = config
        .games
        .iter()
        .find(|g| g.id == game_id)
        .ok_or_else(|| anyhow::anyhow!("未找到游戏: {}", game_id))?;

    // 查找对应时间戳的 manifest
    let manifests = super::manifest::load_manifests(app, game_id)?;
    let manifest = manifests
        .into_iter()
        .find(|m| m.timestamp.format("%Y%m%d_%H%M%S").to_string() == backup_timestamp)
        .ok_or_else(|| anyhow::anyhow!("未找到备份: {}", backup_timestamp))?;

    // 仅支持全量备份恢复
    let zip_remote_path = manifest
        .zip_file
        .ok_or_else(|| anyhow::anyhow!("该备份不支持恢复"))?;

    perform_restore_from_remote_zip(app, game, &zip_remote_path).await
}

/// 从远程 ZIP 文件恢复存档（不依赖本地 manifest，直接指定远程 ZIP 路径）
pub async fn perform_restore_from_remote(
    app: &AppHandle,
    game_id: &str,
    remote_zip_path: &str,
) -> anyhow::Result<RestoreResult> {
    let config = crate::config::load_config(app)?;
    let game = config
        .games
        .iter()
        .find(|g| g.id == game_id)
        .ok_or_else(|| anyhow::anyhow!("未找到游戏: {}", game_id))?;

    perform_restore_from_remote_zip(app, game, remote_zip_path).await
}

/// 核心恢复逻辑：下载远程 ZIP 并解压到存档路径
async fn perform_restore_from_remote_zip(
    app: &AppHandle,
    game: &crate::config::model::GameConfig,
    remote_zip_path: &str,
) -> anyhow::Result<RestoreResult> {
    let config = crate::config::load_config(app)?;

    // 下载压缩包到临时目录
    let temp_dir = std::env::temp_dir().join("gamesave-manager").join("restore");
    std::fs::create_dir_all(&temp_dir)?;
    let zip_name = Path::new(remote_zip_path)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let local_zip = temp_dir.join(&zip_name);

    if let Some(ref alist) = config.alist {
        if let Some(ref token) = alist.token {
            crate::alist::fs::download_file(
                &alist.base_url,
                token,
                remote_zip_path,
                local_zip.to_str().unwrap(),
            ).await?;
        } else {
            anyhow::bail!("未登录 Alist");
        }
    } else {
        anyhow::bail!("未配置 Alist");
    }

    let ts_str = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();

    // 解压前先备份现有存档
    for save_path_str in &game.save_paths {
        let save_path = Path::new(save_path_str);
        if save_path.exists() {
            let bak_path = format!("{}.bak_{}", save_path_str, ts_str);
            if save_path.is_dir() {
                let _ = std::fs::remove_dir_all(&bak_path);
                copy_dir_all(save_path, Path::new(&bak_path))?;
            } else {
                let _ = std::fs::remove_file(&bak_path);
                std::fs::copy(save_path, &bak_path)?;
            }
        }
    }

    // 解压覆盖
    let zip_file = std::fs::File::open(&local_zip)?;
    let mut zip_archive = zip::ZipArchive::new(zip_file)?;
    let extract_base = Path::new(
        game.save_paths.first().ok_or_else(|| anyhow::anyhow!("无存档路径"))?
    )
    .parent()
    .unwrap_or(Path::new("."));

    for i in 0..zip_archive.len() {
        let mut file = zip_archive.by_index(i)?;
        let out_path = extract_base.join(file.name());
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out_file = std::fs::File::create(&out_path)?;
        std::io::copy(&mut file, &mut out_file)?;
    }

    // 清理临时文件
    let _ = std::fs::remove_file(&local_zip);

    Ok(RestoreResult {
        success: true,
        message: "恢复完成".to_string(),
    })
}

/// 递归复制目录（简单实现）
fn copy_dir_all(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
