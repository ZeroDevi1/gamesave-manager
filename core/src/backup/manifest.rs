use tauri::Manager;
// backup/manifest.rs - 备份清单管理（记录每次备份状态）
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;

use super::BackupType;

/// 文件条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub relative_path: String,
    pub size: u64,
    pub modified_time: DateTime<Utc>,
    pub sha256: String,
}

/// 备份清单
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub game_id: String,
    pub backup_type: BackupType,
    pub timestamp: DateTime<Utc>,
    pub files: Vec<FileEntry>,
    pub target_path: String,
    pub zip_file: Option<String>, // 全量备份时的压缩包路径
}

/// 获取 manifest 存储目录
fn manifest_dir(app: &AppHandle) -> anyhow::Result<PathBuf> {
    let dir = app.path().app_local_data_dir()?.join("manifests");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// 保存 manifest 到本地
pub fn save_manifest(app: &AppHandle, manifest: &BackupManifest) -> anyhow::Result<()> {
    let dir = manifest_dir(app)?;
    let path = dir.join(format!("{}.json", manifest.game_id));

    let mut manifests = load_manifests_raw(&path)?;
    manifests.push(manifest.clone());

    let json = serde_json::to_string_pretty(&manifests)?;
    std::fs::write(&path, json)?;
    Ok(())
}

/// 加载某游戏的全部备份清单
pub fn load_manifests(app: &AppHandle, game_id: &str) -> anyhow::Result<Vec<BackupManifest>> {
    let dir = manifest_dir(app)?;
    let path = dir.join(format!("{}.json", game_id));
    load_manifests_raw(&path)
}

/// 加载原始 manifest 文件
fn load_manifests_raw(path: &PathBuf) -> anyhow::Result<Vec<BackupManifest>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path)?;
    let manifests: Vec<BackupManifest> = serde_json::from_str(&content)?;
    Ok(manifests)
}

/// 获取最近一次备份清单（用于增量备份比对）
pub fn get_latest_manifest(app: &AppHandle, game_id: &str) -> anyhow::Result<Option<BackupManifest>> {
    let mut manifests = load_manifests(app, game_id)?;
    manifests.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(manifests.into_iter().next())
}
