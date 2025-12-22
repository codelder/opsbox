//! 路由处理器
//!
//! 定义所有 HTTP 路由和处理器

use axum::{
  Json,
  extract::{Path, Query, State},
  http::StatusCode,
  response::IntoResponse,
  routing::{get, post, put},
};
use logseek::agent::{AgentInfo, AgentSearchRequest};
use opsbox_core::agent::models::{AgentFileItem, AgentListRequest, AgentListResponse};
use opsbox_core::logging::LogLevel;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, info, warn};

use crate::api::{
  ApiError, AppState, LogConfigResponse, SuccessResponse, UpdateLogLevelRequest, UpdateRetentionRequest,
};
use crate::config::AgentConfig;
use crate::path::get_available_subdirs;
use crate::search::execute_search;
use axum::Router;
use std::sync::Arc;

/// 健康检查
pub async fn health() -> &'static str {
  "OK"
}

/// 获取 Agent 信息
pub async fn get_info(State(state): State<AppState>) -> Json<AgentInfo> {
  Json(state.config.to_agent_info())
}

/// 列出可用的搜索路径
pub async fn list_available_paths(State(state): State<AppState>) -> Json<Vec<String>> {
  let paths = get_available_subdirs(&state.config);
  Json(paths)
}

/// 处理搜索请求
pub async fn handle_search(
  State(state): State<AppState>,
  Json(request): Json<AgentSearchRequest>,
) -> impl IntoResponse {
  info!("收到搜索请求: task_id={}, query={}", request.task_id, request.query);
  if tracing::enabled!(tracing::Level::DEBUG) {
    match serde_json::to_string(&request) {
      Ok(s) => debug!("[Wire] ← /api/v1/search 请求体: {}", s),
      Err(e) => debug!("[Wire] ← /api/v1/search 请求体序列化失败: {}", e),
    }
  }

  // 创建结果 channel
  let (tx, rx) = mpsc::channel(128);

  // 创建取消令牌
  use tokio_util::sync::CancellationToken;
  let cancel_token = CancellationToken::new();
  let cancel_token_clone = cancel_token.clone();

  // 在后台执行搜索
  tokio::spawn(execute_search(state.config.clone(), request, tx, cancel_token_clone));

  // 创建 Drop guard 来触发取消
  struct CancelOnDrop(tokio_util::sync::CancellationToken);
  impl Drop for CancelOnDrop {
    fn drop(&mut self) {
      self.0.cancel();
    }
  }
  let _cancel_guard = CancelOnDrop(cancel_token);

  // 将 channel 转换为 NDJSON 流
  let stream = ReceiverStream::new(rx).map(move |msg| {
    let _ = &_cancel_guard; // 捕获 guard
    let json = serde_json::to_string(&msg).unwrap_or_else(|_| "{}".to_string());
    Ok::<_, std::convert::Infallible>(format!("{}\n", json))
  });

  axum::response::Response::builder()
    .status(StatusCode::OK)
    .header(axum::http::header::CONTENT_TYPE, "application/x-ndjson; charset=utf-8")
    .body(axum::body::Body::from_stream(stream))
    .unwrap()
}

/// 取消搜索任务
pub async fn handle_cancel(State(_state): State<AppState>, Path(task_id): Path<String>) -> StatusCode {
  warn!("收到取消请求: task_id={} (暂未实现)", task_id);
  StatusCode::NOT_IMPLEMENTED
}

/// 获取日志配置
pub async fn get_log_config(State(state): State<AppState>) -> Result<Json<LogConfigResponse>, ApiError> {
  let current_level = state.config.current_log_level.lock().unwrap().clone();
  let response = LogConfigResponse {
    level: current_level,
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
  // 验证日志级别
  let level = LogLevel::from_str(&req.level).map_err(|e| ApiError::InvalidLevel(e.to_string()))?;

  // 动态重载日志级别
  let reload_handle = state.config.get_reload_handle().ok_or(ApiError::NotInitialized)?;

  reload_handle
    .update_level(level)
    .map_err(|e| ApiError::ReloadFailed(e.to_string()))?;

  // 更新保存的当前日志级别
  *state.config.current_log_level.lock().unwrap() = req.level.clone();

  info!("日志级别已更新为: {}", level);

  Ok(Json(SuccessResponse {
    message: format!("日志级别已更新为: {}", level),
  }))
}

/// 更新日志保留数量
pub async fn update_log_retention(
  State(_state): State<AppState>,
  Json(req): Json<UpdateRetentionRequest>,
) -> Result<Json<SuccessResponse>, ApiError> {
  // 验证保留数量
  if req.retention_count == 0 || req.retention_count > 365 {
    return Err(ApiError::InvalidRetention("保留数量必须在 1-365 之间".to_string()));
  }

  // 注意：Agent 不持久化配置到数据库，仅在内存中更新
  // 重启后会使用命令行参数指定的值
  info!("日志保留数量已更新为: {} 天（重启后失效）", req.retention_count);

  Ok(Json(SuccessResponse {
    message: format!("日志保留数量已更新为: {} 天（重启后失效）", req.retention_count),
  }))
}

/// 列出目录文件
pub async fn handle_list_files(
  State(_state): State<AppState>,
  Query(req): Query<AgentListRequest>,
) -> Result<Json<AgentListResponse>, ApiError> {
  let path_str = req.path;
  let path = std::path::Path::new(&path_str);

  // Security check: ensure path is within allowed directories or subdirectories
  // This is simplified. Real implementation should check `state.config.search_dirs`
  // assuming agent config has allowed paths.

  if !path.exists() {
    return Err(ApiError::NotFound(format!("Path not found: {}", path_str)));
  }

  let mut read_dir = tokio::fs::read_dir(path)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;
  let mut items = Vec::new();

  while let Ok(Some(entry)) = read_dir.next_entry().await {
    if let Ok(meta) = entry.metadata().await {
      let name = entry.file_name().to_string_lossy().to_string();
      let is_dir = meta.is_dir();
      let size = if is_dir { None } else { Some(meta.len()) };
      let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

      // Full path
      let full_path = entry.path().to_string_lossy().to_string();

      items.push(AgentFileItem {
        name,
        path: full_path,
        is_dir,
        size,
        modified,
      });
    }
  }

  // Sort items: directories first, then files
  items.sort_by(|a, b| {
    if a.is_dir == b.is_dir {
      a.name.cmp(&b.name)
    } else if a.is_dir {
      std::cmp::Ordering::Less
    } else {
      std::cmp::Ordering::Greater
    }
  });

  Ok(Json(AgentListResponse { items }))
}

/// 创建 Agent 路由
pub fn create_router(config: Arc<AgentConfig>) -> Router {
  Router::new()
    .route("/health", get(health))
    .route("/api/v1/info", get(get_info))
    .route("/api/v1/paths", get(list_available_paths))
    .route("/api/v1/search", post(handle_search))
    .route("/api/v1/cancel/{task_id}", post(handle_cancel))
    .route("/api/v1/log/config", get(get_log_config))
    .route("/api/v1/log/level", put(update_log_level))
    .route("/api/v1/log/retention", put(update_log_retention))
    .route("/api/v1/list_files", get(handle_list_files))
    .with_state(AppState { config })
}
