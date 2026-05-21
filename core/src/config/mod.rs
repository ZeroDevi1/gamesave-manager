use tauri::Manager;
// config/mod.rs - 配置读写
pub mod model;

use model::AppConfig;
use std::path::PathBuf;
use tauri::AppHandle;

/// 获取配置文件路径
fn config_path(app: &AppHandle) -> anyhow::Result<PathBuf> {
    let dir = app.path().app_local_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("config.json"))
}

/// 加载应用配置（如果不存在则返回默认配置）
pub fn load_config(app: &AppHandle) -> anyhow::Result<AppConfig> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let config: AppConfig = serde_json::from_str(&content)?;
    Ok(config)
}

/// 保存应用配置
pub fn save_config(app: &AppHandle, config: &AppConfig) -> anyhow::Result<()> {
    let path = config_path(app)?;
    let json = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, json)?;
    Ok(())
}

/// Tauri Commands 导出
pub mod commands {
    use super::*;

    /// 加载配置命令
    #[tauri::command]
    pub fn load_config(app: AppHandle) -> Result<AppConfig, String> {
        super::load_config(&app).map_err(|e| e.to_string())
    }

    /// 保存配置命令
    #[tauri::command]
    pub fn save_config(app: AppHandle, config: AppConfig) -> Result<bool, String> {
        super::save_config(&app, &config).map_err(|e| e.to_string())?;
        Ok(true)
    }
}
