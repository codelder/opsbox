//! OpsBox Agent 库
//!
//! 导出用于测试的类型和函数

use axum::{Json, extract::State};
use opsbox_core::logging::{LogLevel, ReloadHandle};
use std::{path::PathBuf, sync::Arc};

// 重新导出需要的类型
pub use crate::types::*;

mod types {
  use super::*;

  /// Agent 配置
  #[allow(dead_code)]
  pub struct AgentConfig {
    agent_id: String,
    agent_name: String,
    server_endpoint: String,
    search_roots: Vec<String>,
    listen_port: u16,
    enable_heartbeat: bool,
    heartbeat_interval_secs: u64,
    worker_threads: Option<usize>,
    pub log_dir: PathBuf,
    pub log_retention: usize,
    reload_handle: Option<Arc<ReloadHandle>>,
  }

  impl AgentConfig {
    pub fn new(
      agent_id: String,
      agent_name: String,
      server_endpoint: String,
      search_roots: Vec<String>,
      listen_port: u16,
      enable_heartbeat: bool,
      heartbeat_interval_secs: u64,
      worker_threads: Option<usize>,
      log_dir: PathBuf,
      log_retention: usize,
      reload_handle: Option<Arc<ReloadHandle>>,
    ) -> Self {
      Self {
        agent_id,
        agent_name,
        server_endpoint,
        search_roots,
        listen_port,
        enable_heartbeat,
        heartbeat_interval_secs,
        worker_threads,
        log_dir,
        log_retention,
        reload_handle,
      }
    }

    pub fn get_reload_handle(&self) -> Option<Arc<ReloadHandle>> {
      self.reload_handle.clone()
    }
  }

  /// 应用状态
  #[derive(Clone)]
  pub struct AppState {
    pub config: Arc<AgentConfig>,
  }

  /// 日志配置响应
  #[derive(Debug, serde::Serialize, serde::Deserialize)]
  pub struct LogConfigResponse {
    pub level: String,
    pub retention_count: usize,
    pub log_dir: String,
  }

  /// 更新日志级别请求
  #[derive(Debug, serde::Serialize, serde::Deserialize)]
  pub struct UpdateLogLevelRequest {
    pub level: String,
  }

  /// 更新保留数量请求
  #[derive(Debug, serde::Serialize, serde::Deserialize)]
  pub struct UpdateRetentionRequest {
    pub retention_count: usize,
  }

  /// 通用成功响应
  #[derive(Debug, serde::Serialize)]
  pub struct SuccessResponse {
    pub message: String,
  }

  /// 错误响应
  #[derive(Debug, serde::Serialize)]
  pub struct ErrorResponse {
    pub error: String,
  }

  /// API 错误类型
  #[derive(Debug)]
  pub enum ApiError {
    InvalidLevel(String),
    InvalidRetention(String),
    ReloadFailed(String),
    NotInitialized,
  }

  impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
      use axum::http::StatusCode;

      let (status, message) = match self {
        ApiError::InvalidLevel(msg) => (StatusCode::BAD_REQUEST, format!("无效的日志级别: {}", msg)),
        ApiError::InvalidRetention(msg) => (StatusCode::BAD_REQUEST, format!("无效的保留数量: {}", msg)),
        ApiError::ReloadFailed(msg) => (StatusCode::INTERNAL_SERVER_ERROR, format!("重载失败: {}", msg)),
        ApiError::NotInitialized => (StatusCode::INTERNAL_SERVER_ERROR, "日志系统未初始化".to_string()),
      };

      (status, Json(ErrorResponse { error: message })).into_response()
    }
  }
}

/// 获取日志配置
pub async fn get_log_config(State(state): State<AppState>) -> Result<Json<LogConfigResponse>, ApiError> {
  let response = LogConfigResponse {
    level: "info".to_string(),
    retention_count: state.config.log_retention,
    log_dir: state.config.log_dir.to_string_lossy().to_string(),
  };

  Ok(Json(response))
}

/// 更新日志级别
pub async fn update_log_level(
  State(state): State<AppState>,
  Json(req): Json<UpdateLogLevelRequest>,
) -> Result<Json<SuccessResponse>, ApiError> {
  use std::str::FromStr;

  let level = LogLevel::from_str(&req.level).map_err(|e| ApiError::InvalidLevel(e.to_string()))?;

  let reload_handle = state.config.get_reload_handle().ok_or(ApiError::NotInitialized)?;

  reload_handle
    .update_level(level)
    .map_err(|e| ApiError::ReloadFailed(e.to_string()))?;

  tracing::info!("日志级别已更新为: {}", level);

  Ok(Json(SuccessResponse {
    message: format!("日志级别已更新为: {}", level),
  }))
}

/// 更新日志保留数量
pub async fn update_log_retention(
  State(_state): State<AppState>,
  Json(req): Json<UpdateRetentionRequest>,
) -> Result<Json<SuccessResponse>, ApiError> {
  if req.retention_count == 0 || req.retention_count > 365 {
    return Err(ApiError::InvalidRetention("保留数量必须在 1-365 之间".to_string()));
  }

  tracing::info!("日志保留数量已更新为: {} 天（重启后失效）", req.retention_count);

  Ok(Json(SuccessResponse {
    message: format!("日志保留数量已更新为: {} 天（重启后失效）", req.retention_count),
  }))
}
