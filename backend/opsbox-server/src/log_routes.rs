//! 日志配置 API 路由
//!
//! 提供 Server 日志配置的 REST API 端点

use axum::{
  Json, Router,
  extract::State,
  routing::{get, put},
};
use opsbox_core::{
  SqlitePool, UpdateLogLevelRequest, UpdateRetentionRequest,
  error::{AppError, Result},
  logging::{
    LogLevel,
    repository::{LogConfigRepository, LogConfigResponse},
  },
  response::SuccessResponse,
};
use std::str::FromStr;

// 使用 opsbox-core 的 SuccessResponse<T>，T=() 表示无数据
// SuccessResponse 已从 opsbox_core::response 导入

/// 应用状态
#[derive(Clone)]
struct AppState {
  pool: SqlitePool,
  log_dir: std::path::PathBuf,
}

/// 获取日志配置
async fn get_log_config(State(state): State<AppState>) -> Result<Json<LogConfigResponse>> {
  let repo = LogConfigRepository::new(state.pool);

  let response = repo.get_response("server", state.log_dir).await?;

  Ok(Json(response))
}

/// 更新日志级别
async fn update_log_level(
  State(state): State<AppState>,
  Json(req): Json<UpdateLogLevelRequest>,
) -> Result<Json<SuccessResponse<()>>> {
  // 验证日志级别
  let level = LogLevel::from_str(&req.level).map_err(|e| AppError::bad_request(format!("无效的日志级别: {}", e)))?;

  // 更新数据库
  let repo = LogConfigRepository::new(state.pool);
  repo.update_level("server", level).await?;

  // 动态重载日志级别
  let reload_handle =
    crate::server::get_log_reload_handle().ok_or_else(|| AppError::internal("日志系统未初始化".to_string()))?;

  reload_handle
    .update_level(level)
    .map_err(|e| AppError::internal(format!("重载失败: {}", e)))?;

  tracing::info!("日志级别已更新为: {}", level);

  Ok(Json(SuccessResponse::<()>::with_message(format!(
    "日志级别已更新为: {}",
    level
  ))))
}

/// 更新日志保留数量
async fn update_log_retention(
  State(state): State<AppState>,
  Json(req): Json<UpdateRetentionRequest>,
) -> Result<Json<SuccessResponse<()>>> {
  // 验证保留数量
  if req.retention_count == 0 || req.retention_count > 365 {
    return Err(AppError::bad_request("保留数量必须在 1-365 之间".to_string()));
  }

  // 更新数据库
  let repo = LogConfigRepository::new(state.pool);
  repo.update_retention("server", req.retention_count).await?;

  tracing::info!("日志保留数量已更新为: {} 天", req.retention_count);

  Ok(Json(SuccessResponse::<()>::with_message(format!(
    "日志保留数量已更新为: {} 天",
    req.retention_count
  ))))
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

#[cfg(test)]
mod tests {
  use super::*;
  use opsbox_core::logging::LogLevel;

  /// Test LogLevel parsing from string
  ///
  /// 业务场景: API 接收字符串日志级别，需要正确解析为 LogLevel 枚举
  #[test]
  fn test_loglevel_from_str() {
    // Valid levels
    assert_eq!(LogLevel::from_str("error").unwrap(), LogLevel::Error);
    assert_eq!(LogLevel::from_str("warn").unwrap(), LogLevel::Warn);
    assert_eq!(LogLevel::from_str("info").unwrap(), LogLevel::Info);
    assert_eq!(LogLevel::from_str("debug").unwrap(), LogLevel::Debug);
    assert_eq!(LogLevel::from_str("trace").unwrap(), LogLevel::Trace);

    // Case insensitive
    assert_eq!(LogLevel::from_str("ERROR").unwrap(), LogLevel::Error);
    assert_eq!(LogLevel::from_str("Info").unwrap(), LogLevel::Info);
  }

  /// Test LogLevel rejects invalid values
  ///
  /// 业务场景: 防止用户输入无效的日志级别
  #[test]
  fn test_loglevel_invalid() {
    assert!(LogLevel::from_str("").is_err());
    assert!(LogLevel::from_str("invalid").is_err());
    assert!(LogLevel::from_str("warning").is_err()); // common mistake
    assert!(LogLevel::from_str("fatal").is_err());
  }

  /// Test LogLevel Display formatting
  ///
  /// 业务场景: 日志级别需要以字符串形式显示给用户
  #[test]
  fn test_loglevel_display() {
    assert_eq!(format!("{}", LogLevel::Error), "error");
    assert_eq!(format!("{}", LogLevel::Warn), "warn");
    assert_eq!(format!("{}", LogLevel::Info), "info");
    assert_eq!(format!("{}", LogLevel::Debug), "debug");
    assert_eq!(format!("{}", LogLevel::Trace), "trace");
  }

  /// Test retention count validation boundaries
  ///
  /// 业务场景: 日志保留数量必须在 1-365 天之间
  #[test]
  fn test_retention_count_boundaries() {
    // Valid boundaries
    assert!(validate_retention(1).is_ok());
    assert!(validate_retention(365).is_ok());
    assert!(validate_retention(30).is_ok());

    // Invalid boundaries
    assert!(validate_retention(0).is_err());
    assert!(validate_retention(366).is_err());
    assert!(validate_retention(1000).is_err());
  }

  /// Helper function to validate retention count (mirrors the logic in update_log_retention)
  fn validate_retention(count: usize) -> std::result::Result<(), String> {
    if count == 0 || count > 365 {
      Err("保留数量必须在 1-365 之间".to_string())
    } else {
      Ok(())
    }
  }

  /// Test UpdateLogLevelRequest serialization
  ///
  /// 业务场景: API 请求体需要正确序列化/反序列化
  #[test]
  fn test_update_log_level_request() {
    let json = r#"{"level":"info"}"#;
    let req: UpdateLogLevelRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.level, "info");

    let json = r#"{"level":"debug"}"#;
    let req: UpdateLogLevelRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.level, "debug");
  }

  /// Test UpdateRetentionRequest serialization
  #[test]
  fn test_update_retention_request() {
    let json = r#"{"retention_count":30}"#;
    let req: UpdateRetentionRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.retention_count, 30);

    let json = r#"{"retention_count":365}"#;
    let req: UpdateRetentionRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.retention_count, 365);
  }

  /// Test SuccessResponse formatting for log updates
  ///
  /// 业务场景: API 响应需要包含友好的成功消息
  #[test]
  fn test_success_response_formatting() {
    let level = LogLevel::Info;
    let msg = format!("日志级别已更新为: {}", level);
    let response = SuccessResponse::<()>::with_message(msg.clone());

    assert!(response.success);
    assert_eq!(response.message, Some(msg));

    let retention = 30;
    let msg = format!("日志保留数量已更新为: {} 天", retention);
    let response = SuccessResponse::<()>::with_message(msg.clone());

    assert!(response.success);
    assert_eq!(response.message, Some(msg));
  }
}
