// alist/mod.rs - Alist / OpenList 客户端模块
pub mod auth;
pub mod fs;
pub mod types;

/// Tauri Commands 导出
pub mod commands {
    use super::*;

    /// Alist 登录命令
    #[tauri::command]
    pub async fn alist_login(
        url: String,
        username: String,
        password: String,
    ) -> Result<types::LoginResponse, String> {
        auth::login(&url, &username, &password).await.map_err(|e| e.to_string())
    }

    /// 列出 Alist 目录内容
    #[tauri::command]
    pub async fn alist_list_dir(
        url: String,
        token: String,
        path: String,
    ) -> Result<Vec<types::FileEntry>, String> {
        fs::list_dir(&url, &token, &path).await.map_err(|e| e.to_string())
    }

    /// 上传文件到 Alist
    #[tauri::command]
    pub async fn alist_upload(
        url: String,
        token: String,
        local_path: String,
        remote_path: String,
    ) -> Result<(), String> {
        fs::upload_file(&url, &token, &local_path, &remote_path).await.map_err(|e| e.to_string())
    }

    /// 创建 Alist 目录
    #[tauri::command]
    pub async fn alist_mkdir(
        url: String,
        token: String,
        path: String,
    ) -> Result<(), String> {
        fs::mkdir(&url, &token, &path).await.map_err(|e| e.to_string())
    }
}
