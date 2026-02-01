use axum::{
  Json,
  http::StatusCode,
  response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

use crate::logging::LogError;

/// 错误响应 URL 基础路径
const ERROR_BASE_URL: &str = "https://opsbox.dev/errors/";

/// 统一错误类型
#[derive(Error, Debug)]
pub enum AppError {
  /// 数据库错误
  #[error("数据库错误: {0}")]
  Database(#[from] sqlx::Error),

  /// 配置错误
  #[error("配置错误: {0}")]
  Config(String),

  /// 内部服务器错误
  #[error("内部错误: {0}")]
  Internal(String),

  /// 请求参数错误
  #[error("请求参数错误: {0}")]
  BadRequest(String),

  /// 资源未找到
  #[error("资源未找到: {0}")]
  NotFound(String),

  /// 外部服务错误
  #[error("外部服务错误: {0}")]
  ExternalService(String),
}

impl AppError {
  /// 创建配置错误
  pub fn config(msg: impl Into<String>) -> Self {
    Self::Config(msg.into())
  }

  /// 创建内部错误
  pub fn internal(msg: impl Into<String>) -> Self {
    Self::Internal(msg.into())
  }

  /// 创建参数错误
  pub fn bad_request(msg: impl Into<String>) -> Self {
    Self::BadRequest(msg.into())
  }

  /// 创建未找到错误
  pub fn not_found(msg: impl Into<String>) -> Self {
    Self::NotFound(msg.into())
  }

  /// 创建外部服务错误
  pub fn external_service(msg: impl Into<String>) -> Self {
    Self::ExternalService(msg.into())
  }

  /// 获取错误的 HTTP 状态码和类型标识（合并以避免重复的 match）
  fn error_metadata(&self) -> (StatusCode, &'static str) {
    match self {
      Self::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "database_error"),
      Self::Config(_) => (StatusCode::INTERNAL_SERVER_ERROR, "configuration_error"),
      Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
      Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
      Self::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
      Self::ExternalService(_) => (StatusCode::BAD_GATEWAY, "external_service_error"),
    }
  }

  /// 获取 HTTP 状态码
  fn status_code(&self) -> StatusCode {
    self.error_metadata().0
  }

  /// 获取错误类型标识
  fn error_type(&self) -> &'static str {
    self.error_metadata().1
  }
}

impl From<LogError> for AppError {
  fn from(err: LogError) -> Self {
    match err {
      LogError::DirectoryCreation(e) => AppError::Internal(format!("日志目录创建失败: {}", e)),
      LogError::InvalidConfig(msg) => AppError::Config(msg),
      LogError::InvalidLevel(msg) => AppError::BadRequest(format!("无效的日志级别: {}", msg)),
      LogError::ReloadFailed(msg) => AppError::Internal(format!("日志重载失败: {}", msg)),
    }
  }
}

impl IntoResponse for AppError {
  fn into_response(self) -> Response {
    let status = self.status_code();
    let error_msg = self.to_string();
    let error_type = self.error_type();

    // 记录错误日志
    match status {
      StatusCode::INTERNAL_SERVER_ERROR | StatusCode::BAD_GATEWAY => {
        tracing::error!("[{}] {}", error_type, error_msg);
      }
      StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND => {
        tracing::warn!("[{}] {}", error_type, error_msg);
      }
      _ => {
        tracing::info!("[{}] {}", error_type, error_msg);
      }
    }

    // 使用 RFC 7807 Problem Details 格式响应
    #[derive(Serialize)]
    struct ProblemDetail {
      r#type: String,
      title: String,
      status: u16,
      detail: String,
    }

    let problem = ProblemDetail {
      r#type: format!("{}{}", ERROR_BASE_URL, error_type),
      title: error_msg.clone(),
      status: status.as_u16(),
      detail: error_msg,
    };

    (status, Json(problem)).into_response()
  }
}

// 注意：不需要手动实现 From<AppError> for Box<dyn Error + Send + Sync>
// 因为 AppError 实现了 Error + Send + Sync，Rust 标准库已经提供了自动转换
// 如果确实需要显式转换，可以使用 Box::new(err) as Box<dyn std::error::Error + Send + Sync>

/// Result 类型别名
pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
  use super::*;
  use crate::logging::LogError;

  #[test]
  fn test_error_constructors() {
    let err = AppError::config("config error");
    assert!(matches!(err, AppError::Config(msg) if msg == "config error"));

    let err = AppError::internal("internal error");
    assert!(matches!(err, AppError::Internal(msg) if msg == "internal error"));

    let err = AppError::bad_request("bad request");
    assert!(matches!(err, AppError::BadRequest(msg) if msg == "bad request"));

    let err = AppError::not_found("not found");
    assert!(matches!(err, AppError::NotFound(msg) if msg == "not found"));

    let err = AppError::external_service("external error");
    assert!(matches!(err, AppError::ExternalService(msg) if msg == "external error"));
  }

  #[test]
  fn test_status_codes() {
    assert_eq!(AppError::config("").status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(AppError::internal("").status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(AppError::bad_request("").status_code(), StatusCode::BAD_REQUEST);
    assert_eq!(AppError::not_found("").status_code(), StatusCode::NOT_FOUND);
    assert_eq!(AppError::external_service("").status_code(), StatusCode::BAD_GATEWAY);
  }

  #[test]
  fn test_error_type_method() {
    assert_eq!(AppError::config("").error_type(), "configuration_error");
    assert_eq!(AppError::internal("").error_type(), "internal_error");
    assert_eq!(AppError::bad_request("").error_type(), "bad_request");
    assert_eq!(AppError::not_found("").error_type(), "not_found");
    assert_eq!(AppError::external_service("").error_type(), "external_service_error");

    // Database error needs special handling since it's created via from conversion
    let db_err = AppError::Database(sqlx::Error::PoolClosed);
    assert_eq!(db_err.error_type(), "database_error");
  }

  #[test]
  fn test_log_error_conversion() {
    // Test DirectoryCreation variant
    let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let log_err = LogError::DirectoryCreation(io_error);
    let app_err: AppError = log_err.into();

    assert!(matches!(app_err, AppError::Internal(msg) if msg.contains("日志目录创建失败")));

    // Test InvalidConfig variant
    let log_err = LogError::InvalidConfig("invalid config".to_string());
    let app_err: AppError = log_err.into();
    assert!(matches!(app_err, AppError::Config(msg) if msg == "invalid config"));

    // Test InvalidLevel variant
    let log_err = LogError::InvalidLevel("invalid level".to_string());
    let app_err: AppError = log_err.into();
    assert!(matches!(app_err, AppError::BadRequest(msg) if msg.contains("无效的日志级别")));

    // Test ReloadFailed variant
    let log_err = LogError::ReloadFailed("reload failed".to_string());
    let app_err: AppError = log_err.into();
    assert!(matches!(app_err, AppError::Internal(msg) if msg.contains("日志重载失败")));
  }

  #[test]
  fn test_database_error_conversion() {
    // Test sqlx::Error conversion via From trait
    let sqlx_err = sqlx::Error::PoolClosed;
    let app_err: AppError = AppError::from(sqlx_err);
    assert!(matches!(app_err, AppError::Database(_)));
  }

  #[test]
  fn test_result_type_alias() {
    // Test that Result<T> works correctly with AppError
    let ok_result: Result<String> = Ok("success".to_string());
    assert!(ok_result.is_ok());
    assert_eq!(ok_result.unwrap(), "success");

    let err_result: Result<String> = Err(AppError::config("config error"));
    assert!(err_result.is_err());
    assert!(matches!(err_result.unwrap_err(), AppError::Config(msg) if msg == "config error"));
  }
}
