// game/registry.rs - 内置游戏存档路径数据库
use serde::{Deserialize, Serialize};

/// 内置游戏条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinGame {
    pub id: String,
    pub name: String,
    pub steam_appid: Option<u64>,
    pub save_paths: Vec<String>,
}

/// 获取内置游戏数据库
pub fn get_builtin_games() -> Vec<BuiltinGame> {
    vec![
        BuiltinGame {
            id: "elden-ring".to_string(),
            name: "Elden Ring".to_string(),
            steam_appid: Some(1245620),
            save_paths: vec![
                "%APPDATA%/EldenRing/".to_string(),
            ],
        },
        BuiltinGame {
            id: "witcher3".to_string(),
            name: "The Witcher 3".to_string(),
            steam_appid: Some(292030),
            save_paths: vec![
                "%USERPROFILE%/Documents/The Witcher 3/gamesaves/".to_string(),
            ],
        },
        BuiltinGame {
            id: "cyberpunk2077".to_string(),
            name: "Cyberpunk 2077".to_string(),
            steam_appid: Some(1091500),
            save_paths: vec![
                "%LOCALAPPDATA%/../LocalLow/CD Projekt Red/Cyberpunk 2077/".to_string(),
            ],
        },
        BuiltinGame {
            id: "hollow-knight".to_string(),
            name: "Hollow Knight".to_string(),
            steam_appid: Some(367520),
            save_paths: vec![
                "%APPDATA%/../LocalLow/Team Cherry/Hollow Knight/".to_string(),
            ],
        },
        BuiltinGame {
            id: "stardew-valley".to_string(),
            name: "Stardew Valley".to_string(),
            steam_appid: Some(413150),
            save_paths: vec![
                "%APPDATA%/StardewValley/Saves/".to_string(),
            ],
        },
        BuiltinGame {
            id: "dark-souls-3".to_string(),
            name: "Dark Souls III".to_string(),
            steam_appid: Some(374320),
            save_paths: vec![
                "%APPDATA%/DarkSoulsIII/".to_string(),
            ],
        },
        BuiltinGame {
            id: "sekiro".to_string(),
            name: "Sekiro: Shadows Die Twice".to_string(),
            steam_appid: Some(814380),
            save_paths: vec![
                "%APPDATA%/Sekiro/".to_string(),
            ],
        },
        BuiltinGame {
            id: "baldurs-gate-3".to_string(),
            name: "Baldur's Gate 3".to_string(),
            steam_appid: Some(1086940),
            save_paths: vec![
                "%LOCALAPPDATA%/Larian Studios/Baldur's Gate 3/PlayerProfiles/Public/Savegames/".to_string(),
            ],
        },
        BuiltinGame {
            id: "minecraft".to_string(),
            name: "Minecraft".to_string(),
            steam_appid: None,
            save_paths: vec![
                "%APPDATA%/.minecraft/saves/".to_string(),
            ],
        },
        BuiltinGame {
            id: "genshin-impact".to_string(),
            name: "Genshin Impact".to_string(),
            steam_appid: None,
            save_paths: vec![
                "%USERPROFILE%/AppData/LocalLow/miHoYo/Genshin Impact/".to_string(),
            ],
        },
    ]
}
