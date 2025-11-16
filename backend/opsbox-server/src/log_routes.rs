//! 日志配置 API 路由
//!
//! 提供 Server 日志配置的 REST API 端点

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, put},
    Json, Router,
};
use opsbox_core::{
    logging::{repository::{LogConfigRepository, LogConfigResponse}, LogLevel},
    SqlitePool,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// 更新日志级别请求
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateLogLevelRequest {
    /// 日志级别: "error" | "warn" | "info" | "debug" | "trace"
    pub level: String,
}

/// 更新保留数量请求
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateRetentionRequest {
    /// 保留数量（天）
    pub retention_count: usize,
}

/// 通用成功响应
#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    pub message: String,
}

/// 错误响应
#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

/// API 错误类型
#[derive(Debug)]
pub enum ApiError {
    InvalidLevel(String),
    InvalidRetention(String),
    DatabaseError(String),
    ReloadFailed(String),
    NotInitialized,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::InvalidLevel(msg) => (StatusCode::BAD_REQUEST, format!("无效的日志级别: {}", msg)),
            ApiError::InvalidRetention(msg) => {
                (StatusCode::BAD_REQUEST, format!("无效的保留数量: {}", msg))
            }
            ApiError::DatabaseError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("数据库错误: {}", msg))
            }
            ApiError::ReloadFailed(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("重载失败: {}", msg))
            }
            ApiError::NotInitialized => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "日志系统未初始化".to_string(),
            ),
        };

        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

/// 应用状态
#[derive(Clone)]
struct AppState {
    pool: SqlitePool,
    log_dir: std::path::PathBuf,
}

/// 获取日志配置
async fn get_log_config(State(state): State<AppState>) -> Result<Json<LogConfigResponse>, ApiError> {
    let repo = LogConfigRepository::new(state.pool);

    let response = repo
        .get_response("server", state.log_dir)
        .await
        .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

    Ok(Json(response))
}

/// 更新日志级别
async fn update_log_level(
    State(state): State<AppState>,
    Json(req): Json<UpdateLogLevelRequest>,
) -> Result<Json<SuccessResponse>, ApiError> {
    // 验证日志级别
    let level = LogLevel::from_str(&req.level).map_err(|e| ApiError::InvalidLevel(e.to_string()))?;

    // 更新数据库
    let repo = LogConfigRepository::new(state.pool);
    repo.update_level("server", level)
        .await
        .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

    // 动态重载日志级别
    let reload_handle = crate::server::get_log_reload_handle().ok_or(ApiError::NotInitialized)?;

    reload_handle
        .update_level(level)
        .map_err(|e| ApiError::ReloadFailed(e.to_string()))?;

    tracing::info!("日志级别已更新为: {}", level);

    Ok(Json(SuccessResponse {
        message: format!("日志级别已更新为: {}", level),
    }))
}

/// 更新日志保留数量
async fn update_log_retention(
    State(state): State<AppState>,
    Json(req): Json<UpdateRetentionRequest>,
) -> Result<Json<SuccessResponse>, ApiError> {
    // 验证保留数量
    if req.retention_count == 0 || req.retention_count > 365 {
        return Err(ApiError::InvalidRetention(
            "保留数量必须在 1-365 之间".to_string(),
        ));
    }

    // 更新数据库
    let repo = LogConfigRepository::new(state.pool);
    repo.update_retention("server", req.retention_count)
        .await
        .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

    tracing::info!("日志保留数量已更新为: {} 天", req.retention_count);

    Ok(Json(SuccessResponse {
        message: format!("日志保留数量已更新为: {} 天", req.retention_count),
    }))
}

/// 创建日志配置路由
pub fn create_log_routes(pool: SqlitePool, log_dir: std::path::PathBuf) -> Router {
    let state = AppState { pool, log_dir };
    Router::new()
        .route("/api/v1/log/config", get(get_log_config))
        .route("/api/v1/log/level", put(update_log_level))
        .route("/api/v1/log/retention", put(update_log_retention))
        .with_state(state)
}
