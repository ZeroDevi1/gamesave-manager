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

/// 加载应用配置（如果不存在则返回默认配置，支持旧配置平滑演进与自动物理迁移）
pub fn load_config(app: &AppHandle) -> anyhow::Result<AppConfig> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let mut config: AppConfig = serde_json::from_str(&content)?;
    
    // 平滑兼容迁移逻辑：若新版统一存储配置 storage 为 None，但旧版 alist 存在有效内容，
    // 则自动升级迁移至新的 StorageConfig::Alist 中，并立刻静默持久化回写 config.json，
    // 以实现数据格式的无感安全物理演进，避免用户配置数据丢失。
    if config.storage.is_none() && config.alist.is_some() {
        if let Some(alist_cfg) = config.alist.clone() {
            config.storage = Some(model::StorageConfig::Alist(alist_cfg));
            // 尝试物理写回，忽略单次失败（不阻断主启动流程）
            let _ = save_config(app, &config);
        }
    }
    
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
