//! API 类型定义
//!
//! 定义 API 请求和响应的类型

use axum::{Json, http::StatusCode, response::IntoResponse};
use std::sync::Arc;

use crate::config::AgentConfig;

/// 应用状态
#[derive(Clone)]
pub struct AppState {
  pub config: Arc<AgentConfig>,
}

/// 日志配置响应
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LogConfigResponse {
  /// 日志级别
  pub level: String,
  /// 日志保留数量（天）
  pub retention_count: usize,
  /// 日志目录
  pub log_dir: String,
}

/// 更新日志级别请求
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct UpdateLogLevelRequest {
  /// 日志级别: "error" | "warn" | "info" | "debug" | "trace"
  pub level: String,
}

/// 更新保留数量请求
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct UpdateRetentionRequest {
  /// 保留数量（天）
  pub retention_count: usize,
}

/// 通用成功响应
#[derive(Debug, serde::Serialize)]
pub struct SuccessResponse {
  pub message: String,
}

/// 错误响应
#[derive(Debug, serde::Serialize)]
struct ErrorResponse {
  error: String,
}

/// API 错误类型
#[derive(Debug)]
pub enum ApiError {
  InvalidLevel(String),
  InvalidRetention(String),
  ReloadFailed(String),
  NotInitialized,
}

impl IntoResponse for ApiError {
  fn into_response(self) -> axum::response::Response {
    let (status, message) = match self {
      ApiError::InvalidLevel(msg) => (StatusCode::BAD_REQUEST, format!("无效的日志级别: {}", msg)),
      ApiError::InvalidRetention(msg) => (StatusCode::BAD_REQUEST, format!("无效的保留数量: {}", msg)),
      ApiError::ReloadFailed(msg) => (StatusCode::INTERNAL_SERVER_ERROR, format!("重载失败: {}", msg)),
      ApiError::NotInitialized => (StatusCode::INTERNAL_SERVER_ERROR, "日志系统未初始化".to_string()),
    };

    (status, Json(ErrorResponse { error: message })).into_response()
  }
}
