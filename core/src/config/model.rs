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

/// 辅助函数：对网盘远程路径进行非法字符“清洗”（Sanitize）
///
/// # 核心设计与规避
/// 百度网盘（以及 OneDrive 等主流云盘）的云端文件系统对文件名及文件夹名有着严苛的安全限制，
/// 严禁使用以下非法特殊字符：`\ : * ? " < > |`。
/// 特别地，如《Kingdom Come: Deliverance II》等带副标题的游戏，其名称自带英文冒号 `:`。若直接发送给
/// 百度网盘 API 创建文件夹，会立刻触发 `errno: -7` (非法字符限制) 异常。
/// 本函数将路径中除层级分隔符 `/` 以外的所有非法特殊字符统一洗消替换为下划线 `_`，确保网盘云端顺利建档。
fn sanitize_remote_path(path: &str) -> String {
    path.chars().map(|c| {
        match c {
            ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\\' => '_',
            _ => c
        }
    }).collect()
}

impl AppConfig {
    /// 动态获取并洗消游戏的云端物理备份路径。
    ///
    /// # 参数说明
    /// * `game` - 目标游戏配置
    ///
    /// # 核心设计
    /// 1. 动态迁移：如果配置了自定义的 backup_root，则会将原有的 "/GameSaves/" 替换为指定的备份根路径前缀。
    /// 2. 非法路径净化：在计算出原始路径后，主动调用 `sanitize_remote_path` 过滤所有可能引起百度网盘 `-errno: -7` 
    ///    等云盘物理文件系统创建目录失败的非法特殊字符（如冒号），实现跨平台、跨网盘驱动的极高鲁棒性。
    pub fn get_game_remote_path(&self, game: &GameConfig) -> String {
        let raw_path = if let Some(ref alist) = self.alist {
            if let Some(ref root) = alist.backup_root {
                if !root.is_empty() {
                    // 剥除默认的前缀以避免重复拼接路径层级
                    let game_sub = if game.remote_path.starts_with("/GameSaves/") {
                        game.remote_path.trim_start_matches("/GameSaves/")
                    } else {
                        game.remote_path.trim_start_matches('/')
                    };
                    format!("{}/{}", root.trim_end_matches('/'), game_sub)
                } else {
                    game.remote_path.clone()
                }
            } else {
                game.remote_path.clone()
            }
        } else {
            game.remote_path.clone()
        };

        // 统一对计算完成的路径进行非法字符净化
        sanitize_remote_path(&raw_path)
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
