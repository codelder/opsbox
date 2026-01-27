//! API 类型定义
//!
//! 定义 API 请求和响应的类型

use axum::{Json, http::StatusCode, response::IntoResponse};
use std::sync::Arc;

use crate::config::AgentConfig;

// 从 opsbox-core 重新导出共享类型
pub use opsbox_core::logging::repository::LogConfigResponse;
pub use opsbox_core::logging::{UpdateLogLevelRequest, UpdateRetentionRequest};
pub use opsbox_core::response::SuccessResponse;

/// 应用状态
#[derive(Clone)]
pub struct AppState {
  pub config: Arc<AgentConfig>,
}

// 使用 opsbox-core 的 SuccessResponse<T>，T=() 表示无数据
// pub use opsbox_core::response::SuccessResponse; 已在上面重新导出

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
  NotFound(String),
  Internal(String),
}

impl IntoResponse for ApiError {
  fn into_response(self) -> axum::response::Response {
    let (status, message) = match self {
      ApiError::InvalidLevel(msg) => (StatusCode::BAD_REQUEST, format!("无效的日志级别: {}", msg)),
      ApiError::InvalidRetention(msg) => (StatusCode::BAD_REQUEST, format!("无效的保留数量: {}", msg)),
      ApiError::ReloadFailed(msg) => (StatusCode::INTERNAL_SERVER_ERROR, format!("重载失败: {}", msg)),
      ApiError::NotInitialized => (StatusCode::INTERNAL_SERVER_ERROR, "日志系统未初始化".to_string()),
      ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
      ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, format!("内部错误: {}", msg)),
    };

    (status, Json(ErrorResponse { error: message })).into_response()
  }
}
