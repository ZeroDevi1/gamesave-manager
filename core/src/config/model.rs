// config/model.rs - 配置数据结构
use serde::{Deserialize, Serialize};

/// 应用根配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub alist: Option<AlistConfig>,
    pub games: Vec<GameConfig>,
    pub settings: Settings,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            alist: None,
            games: Vec::new(),
            settings: Settings::default(),
        }
    }
}

/// Alist 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlistConfig {
    pub base_url: String,
    pub username: String,
    pub token: Option<String>,
    pub provider: String, // "alist" | "openlist"
    /// 全局云端备份根路径，例如 "/Download/GameSave"
    pub backup_root: Option<String>,
}

impl AppConfig {
    /// 动态获取游戏的云端物理备份路径。
    /// 如果配置了自定义的 backup_root，则会将原有的 "/GameSaves/" 替换为指定的备份根路径前缀，
    /// 从而在上传备份时无缝飘移到目标目录，同时不影响历史数据的 manifests 绝对位置。
    pub fn get_game_remote_path(&self, game: &GameConfig) -> String {
        if let Some(ref alist) = self.alist {
            if let Some(ref root) = alist.backup_root {
                if !root.is_empty() {
                    // 剥除默认的前缀以避免重复拼接路径层级
                    let game_sub = if game.remote_path.starts_with("/GameSaves/") {
                        game.remote_path.trim_start_matches("/GameSaves/")
                    } else {
                        game.remote_path.trim_start_matches('/')
                    };
                    return format!("{}/{}", root.trim_end_matches('/'), game_sub);
                }
            }
        }
        game.remote_path.clone()
    }
}


/// 游戏配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
    pub id: String,
    pub name: String,
    pub save_paths: Vec<String>,
    pub remote_path: String,
    pub last_backup: Option<String>,
    pub logo_path: Option<String>,
    pub steam_appid: Option<u64>,
}

/// 全局设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String, // "system" | "light" | "dark"
    pub steamgriddb_api_key: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            steamgriddb_api_key: None,
        }
    }
}
