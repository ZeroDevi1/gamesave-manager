// utils/path.rs - 跨平台路径处理（目前仅 Windows）

/// 展开 Windows 环境变量（如 %APPDATA%）
pub fn expand_env(path: &str) -> String {
    let mut result = path.to_string();

    // 展开常见环境变量
    for (var, fallback) in [
        ("%APPDATA%", dirs::data_dir()),
        ("%LOCALAPPDATA%", dirs::cache_dir()),
        ("%USERPROFILE%", dirs::home_dir()),
        ("%PUBLIC%", dirs::public_dir()),
    ] {
        if result.contains(var) {
            if let Some(value) = fallback {
                result = result.replace(var, &value.to_string_lossy());
            }
        }
    }

    // 处理 %APPDATA%/../LocalLow 这类路径
    let path_buf = std::path::PathBuf::from(&result);
    if let Ok(cleaned) = path_buf.canonicalize() {
        cleaned.to_string_lossy().to_string()
    } else {
        result
    }
}

/// 确保路径使用正斜杠（用于远程路径）
pub fn to_unix_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// 将物理路径中的用户特定主目录折叠为 Windows 环境变量占位符，实现跨机器通用性
///
/// # 核心业务语义：
/// 本方法用于在保存或分发游戏存档路径模板时，自动将当前机器的特定用户绝对物理路径
/// （例如 `C:\Users\demon\Saved Games\kingdomcome2\saves\playline0`）智能反向压缩为
/// 平台无关的通用占位符形式（例如 `%USERPROFILE%/Saved Games/kingdomcome2/saves/playline0`）。
/// 别人导入该条目后即可在各自的用户名下完美展开，实现配置的高度共享。
///
/// # 参数：
/// * `path` - 待压缩折叠的物理绝对路径（或已含占位符的混合路径）
///
/// # 返回值：
/// * `String` - 折叠为 `%USERPROFILE%`、`%APPDATA%` 等通用环境变量占位符后的正斜杠规范路径。
///
/// # 边界情况与潜在坑点：
/// 1. **最长前缀匹配优先**：由于 `%LOCALAPPDATA%`（通常为 `C:/Users/demon/AppData/Local`）必然包含 `%USERPROFILE%`（通常为 `C:/Users/demon`），
///    如果优先匹配替换 `%USERPROFILE%`，会导致结果沦为不完美的 `%USERPROFILE%/AppData/Local`。因此必须按照物理路径的字符长度降序排列（`LOCALAPPDATA` 优先于 `USERPROFILE`），确保最精准的最长路径优先匹配。
/// 2. **大小写不敏感匹配**：Windows 下路径大小写不敏感，但用户输入的物理路径和系统返回的环境变量可能具有大小写差异，因此必须将它们在比对前转换为小写形式进行匹配。
/// 3. **保留后缀大小写**：折叠前缀后，后缀部分的子目录结构可能存在特定的注册表或文件系统大小写依赖，本函数会计算索引偏移，只切除前缀并替换为环境变量占位符，完整保留后缀部分的原始大小写。
pub fn shrink_env(path: &str) -> String {
    // 1. 统一将所有 Windows 反斜杠替换为正斜杠，以消除转义符或路径拼接带来的阻碍
    let mut normalized_path = path.replace('\\', "/");

    // 2. 收集 Windows 环境下常用的关键路径及对应的占位符
    let mut env_mappings = Vec::new();

    // `%LOCALAPPDATA%` 对应 AppData/Local 目录
    if let Some(local_appdata) = dirs::cache_dir() {
        env_mappings.push(("%LOCALAPPDATA%", local_appdata.to_string_lossy().replace('\\', "/")));
    }
    // `%APPDATA%` 对应 AppData/Roaming 目录
    if let Some(appdata) = dirs::data_dir() {
        env_mappings.push(("%APPDATA%", appdata.to_string_lossy().replace('\\', "/")));
    }
    // `%USERPROFILE%` 对应用户主目录 C:/Users/<Username>
    if let Some(userprofile) = dirs::home_dir() {
        env_mappings.push(("%USERPROFILE%", userprofile.to_string_lossy().replace('\\', "/")));
    }
    // `%PUBLIC%` 对应公用目录 C:/Users/Public
    if let Some(public_dir) = dirs::public_dir() {
        env_mappings.push(("%PUBLIC%", public_dir.to_string_lossy().replace('\\', "/")));
    }

    // 3. 按照物理路径长度从大到小（降序）排序，防止 `%USERPROFILE%` 抢占匹配 `%LOCALAPPDATA%` 等深层子集
    env_mappings.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    // 4. 逐一遍历并进行不区分大小写的前缀智能匹配折叠
    for (var, physical_path) in env_mappings {
        let lower_path = normalized_path.to_lowercase();
        let lower_physical = physical_path.to_lowercase();

        // 仅当找到的物理前缀精确位于输入路径的最头部时，才进行占位符折叠
        if let Some(idx) = lower_path.find(&lower_physical) {
            if idx == 0 {
                let suffix = &normalized_path[physical_path.len()..];
                normalized_path = format!("{}{}", var, suffix);
                // 只要成功折叠了一个最长匹配的环境变量，由于路径性质，应当直接跳出循环，避免二次重叠替换
                break;
            }
        }
    }

    normalized_path
}

/// 解析游戏存档路径，展开环境变量与 glob 通配符，返回实际存在的物理路径列表
///
/// # 核心语义
/// 用户配置的存档路径可能包含 Windows 环境变量占位符（如 `%APPDATA%`）以及
/// glob 通配符（如 `HK Autosave*`）。本函数先将环境变量展开为绝对物理路径，
/// 再对含 `*`、`?` 的路径执行 glob 匹配，最终返回所有真实存在的文件或目录路径。
///
/// # 返回值
/// 若路径不含通配符且存在，返回包含该路径的单元素 Vec；
/// 若含通配符，返回所有匹配且真实存在的路径；
/// 若路径不存在或 glob 无匹配，返回空 Vec。
pub fn resolve_save_paths(save_path_str: &str) -> Vec<std::path::PathBuf> {
    let expanded = expand_env(save_path_str);
    // 若路径中包含 glob 通配符，执行模式匹配
    if expanded.contains('*') || expanded.contains('?') || expanded.contains('[') {
        match glob::glob(&expanded) {
            Ok(paths) => paths
                .filter_map(|p| p.ok().filter(|p| p.exists()))
                .collect(),
            Err(_) => Vec::new(),
        }
    } else {
        let path = std::path::Path::new(&expanded);
        if path.exists() {
            vec![path.to_path_buf()]
        } else {
            Vec::new()
        }
    }
}

