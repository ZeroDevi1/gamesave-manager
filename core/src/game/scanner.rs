// game/scanner.rs - 本地存档扫描器
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use walkdir::WalkDir;

/// 存档文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveFile {
    pub path: String,
    pub relative_path: String,
    pub size: u64,
    pub modified_time: String,
}

/// 扫描指定游戏的存档文件
pub fn scan_game(app: &AppHandle, game_id: &str) -> anyhow::Result<Vec<SaveFile>> {
    let config = crate::config::load_config(app)?;
    let game = config
        .games
        .iter()
        .find(|g| g.id == game_id)
        .ok_or_else(|| anyhow::anyhow!("未找到游戏: {}", game_id))?;

    let mut results = Vec::new();

    for save_path_str in &game.save_paths {
        for save_path in crate::utils::path::resolve_save_paths(save_path_str) {
            if save_path.is_file() {
                let meta = std::fs::metadata(&save_path)?;
                let modified_time = meta
                    .modified()?
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs();
                results.push(SaveFile {
                    path: save_path.to_string_lossy().to_string(),
                    relative_path: save_path.file_name().unwrap().to_string_lossy().to_string(),
                    size: meta.len(),
                    modified_time: chrono::DateTime::from_timestamp(modified_time as i64, 0)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default(),
                });
            } else {
                for entry in WalkDir::new(&save_path).into_iter().filter_map(|e| e.ok()) {
                    if entry.file_type().is_dir() {
                        continue;
                    }
                    let path = entry.path();
                    let rel_path = path
                        .strip_prefix(&save_path)?
                        .to_string_lossy()
                        .replace('\\', "/");
                    let meta = entry.metadata()?;
                    let modified_time = meta
                        .modified()?
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_secs();
                    results.push(SaveFile {
                        path: path.to_string_lossy().to_string(),
                        relative_path: rel_path,
                        size: meta.len(),
                        modified_time: chrono::DateTime::from_timestamp(modified_time as i64, 0)
                            .map(|dt| dt.to_rfc3339())
                            .unwrap_or_default(),
                    });
                }
            }
        }
    }

    Ok(results)
}
