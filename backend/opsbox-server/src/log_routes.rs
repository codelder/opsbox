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
