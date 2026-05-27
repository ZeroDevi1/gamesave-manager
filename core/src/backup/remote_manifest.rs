// backup/remote_manifest.rs - 远端备份清单管理
//
// 在远端存储的每个游戏根目录下维护一个 manifest.json，记录最近一次备份的文件清单（含 SHA256 和时间戳）。
// 用途：启动游戏前下载此清单与本地存档比对，避免为比对而下载整个存档文件。

use super::manifest::FileEntry;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::AppHandle;

/// 远端备份清单（存储在云端游戏根目录下的 manifest.json）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteManifest {
    /// 游戏唯一标识
    pub game_id: String,
    /// 最近一次备份的时间戳
    pub timestamp: DateTime<Utc>,
    /// 备份时的文件清单（含 SHA256）
    pub files: Vec<FileEntry>,
}

/// 远端 manifest 文件名
const REMOTE_MANIFEST_FILENAME: &str = "manifest.json";

/// 获取远端 manifest 的完整路径
///
/// 格式：{game_remote_path}/manifest.json
fn remote_manifest_path(config: &crate::config::model::AppConfig, game: &crate::config::model::GameConfig) -> String {
    let base = config.get_game_remote_path(game);
    format!("{}/{}", base.trim_end_matches('/'), REMOTE_MANIFEST_FILENAME)
}

/// 上传远端 manifest 到云端
///
/// 在备份成功后调用，将最新的文件清单（含 SHA256 和时间戳）序列化为 JSON 并上传到远端。
/// 使用临时文件中转，避免各存储后端需要支持内存直传。
pub async fn upload_remote_manifest(
    app: &AppHandle,
    game_id: &str,
    files: Vec<FileEntry>,
    timestamp: DateTime<Utc>,
) -> anyhow::Result<()> {
    let config = crate::config::load_config(app)?;
    let game = config
        .games
        .iter()
        .find(|g| g.id == game_id)
        .ok_or_else(|| anyhow::anyhow!("未找到游戏: {}", game_id))?;

    let manifest = RemoteManifest {
        game_id: game_id.to_string(),
        timestamp,
        files,
    };

    // 序列化为 JSON 并写入临时文件
    let json = serde_json::to_string_pretty(&manifest)?;
    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join(format!("gamesave_manifest_{}.json", game_id));
    std::fs::write(&temp_path, &json)?;

    // 上传到远端
    let remote_path = remote_manifest_path(&config, game);
    let backend = crate::storage::get_storage_backend(&config)?;

    // 确保远端目录存在
    let remote_parent = Path::new(&remote_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            let base = config.get_game_remote_path(game);
            base.trim_end_matches('/').to_string()
        });
    let _ = backend.mkdir(&remote_parent).await;

    backend
        .upload_file(temp_path.to_str().unwrap(), &remote_path)
        .await?;

    // 清理临时文件
    let _ = std::fs::remove_file(&temp_path);

    log::info!("[远端清单] 已上传游戏 {} 的 manifest 到 {}", game_id, remote_path);
    Ok(())
}

/// 从远端下载 manifest 并解析
///
/// 启动游戏前调用，用于与本地存档比对。
/// 如果远端不存在 manifest（如从未备份过），返回 None。
pub async fn download_remote_manifest(
    app: &AppHandle,
    game_id: &str,
) -> anyhow::Result<Option<RemoteManifest>> {
    let config = crate::config::load_config(app)?;
    let game = config
        .games
        .iter()
        .find(|g| g.id == game_id)
        .ok_or_else(|| anyhow::anyhow!("未找到游戏: {}", game_id))?;

    let remote_path = remote_manifest_path(&config, game);
    let backend = crate::storage::get_storage_backend(&config)?;

    // 下载到临时文件
    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join(format!("gamesave_manifest_remote_{}.json", game_id));

    match backend
        .download_file(&remote_path, temp_path.to_str().unwrap())
        .await
    {
        Ok(_) => {
            let content = std::fs::read_to_string(&temp_path)?;
            let _ = std::fs::remove_file(&temp_path);
            let manifest: RemoteManifest = serde_json::from_str(&content)?;
            Ok(Some(manifest))
        }
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            // 远端不存在 manifest 时静默返回 None
            if msg.contains("not found")
                || msg.contains("object not found")
                || msg.contains("路径不存在")
                || msg.contains("404")
                || msg.contains("no such file")
            {
                return Ok(None);
            }
            Err(e)
        }
    }
}
