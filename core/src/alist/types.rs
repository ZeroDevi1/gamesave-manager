// alist/types.rs - Alist API 请求/响应 DTO
use serde::{Deserialize, Serialize};

/// 登录响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
}

/// 目录条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default = "default_true")]
    pub is_dir: bool,
    pub modified: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Alist API 通用响应包装
#[derive(Debug, Deserialize)]
pub struct AlistApiResponse<T> {
    pub code: i32,
    pub message: String,
    pub data: Option<T>,
}

/// 登录请求体
#[derive(Debug, Serialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// 目录列表请求体
#[derive(Debug, Serialize)]
pub struct ListDirRequest {
    pub path: String,
    pub password: String,
    pub page: u32,
    pub per_page: u32,
    pub refresh: bool,
}

/// 创建目录请求体
#[derive(Debug, Serialize)]
pub struct MkdirRequest {
    pub path: String,
}
