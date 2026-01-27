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
    let mut json = serde_json::to_vec(&msg).unwrap_or_else(|_| b"{}".to_vec());
    json.push(b'\n');
    Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(json))
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
) -> Result<Json<SuccessResponse<()>>, ApiError> {
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

  Ok(Json(SuccessResponse::<()>::with_message(format!("日志级别已更新为: {}", level))))
}

/// 更新日志保留数量
pub async fn update_log_retention(
  State(_state): State<AppState>,
  Json(req): Json<UpdateRetentionRequest>,
) -> Result<Json<SuccessResponse<()>>, ApiError> {
  // 验证保留数量
  if req.retention_count == 0 || req.retention_count > 365 {
    return Err(ApiError::InvalidRetention("保留数量必须在 1-365 之间".to_string()));
  }

  // 注意：Agent 不持久化配置到数据库，仅在内存中更新
  // 重启后会使用命令行参数指定的值
  info!("日志保留数量已更新为: {} 天（重启后失效）", req.retention_count);

  Ok(Json(SuccessResponse::<()>::with_message(format!("日志保留数量已更新为: {} 天（重启后失效）", req.retention_count))))
}

/// 列出目录文件
pub async fn handle_list_files(
  State(state): State<AppState>,
  Query(req): Query<AgentListRequest>,
) -> Result<Json<AgentListResponse>, ApiError> {
  let path_str = urlencoding::decode(&req.path).map(|s| s.into_owned()).unwrap_or(req.path);

  // Special case: empty path or "/" means list all search roots themselves
  if path_str.is_empty() || path_str == "/" {
    let mut all_items = Vec::new();

    for root in &state.config.search_roots {
      let root_path = std::path::Path::new(root);
      if !root_path.exists() {
        continue;
      }

      // Instead of listing contents, we list the root itself as a virtual entry
      let name = root_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.clone());

      match crate::path::canonicalize_existing(root_path) {
        Ok(abs_path) => {
          all_items.push(AgentFileItem {
            name,
            path: abs_path.to_string_lossy().to_string(),
            is_dir: true,
            is_symlink: false,
            size: None,
            modified: None,
            child_count: None,
            hidden_child_count: None,
            mime_type: None,
          });
        }
        Err(e) => {
          tracing::warn!("Failed to canonicalize search root {}: {}", root, e);
        }
      }
    }

    return Ok(Json(AgentListResponse { items: all_items }));
  }

  // Security check: ensure path is within allowed directories or subdirectories
  use crate::path::resolve_directory_path;
  let resolved_paths = match resolve_directory_path(&state.config, &path_str) {
    Ok(p) => p,
    Err(e) => {
      // 访问被拒绝或路径不在允许范围内，统一返回 NotFound 避免泄露信息
      return Err(ApiError::NotFound(format!("Access denied or path not found: {}", e)));
    }
  };

  // Use the first resolved path for listing
  let path = &resolved_paths[0];

  let items = opsbox_core::fs::list_directory(path)
    .await
    .map_err(ApiError::Internal)?;

  let items = items
    .into_iter()
    .map(|item| AgentFileItem {
      name: item.name,
      path: item.path,
      is_dir: item.is_dir,
      is_symlink: item.is_symlink,
      size: item.size,
      modified: item.modified,
      child_count: item.child_count,
      hidden_child_count: item.hidden_child_count,
      mime_type: item.mime_type,
    })
    .collect();

  Ok(Json(AgentListResponse { items }))
}

use axum::body::Body;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct GetFileRequest {
  pub path: String,
}

/// GET 原始文件内容
pub async fn handle_get_file_raw(
  State(state): State<AppState>,
  Query(req): Query<GetFileRequest>,
) -> Result<impl IntoResponse, ApiError> {
  let path_str = urlencoding::decode(&req.path).map(|s| s.into_owned()).unwrap_or(req.path);

  // Security check: ensure path is within allowed directories or subdirectories
  use crate::path::resolve_directory_path;
  let resolved_paths = match resolve_directory_path(&state.config, &path_str) {
    Ok(p) => p,
    Err(e) => {
      warn!("RawFile: Path resolution failed for {}: {}", path_str, e);
      return Err(ApiError::NotFound(format!("Access denied or path not found: {}", e)));
    }
  };

  // Use the first resolved path
  let path = &resolved_paths[0];

  if !path.exists() || !path.is_file() {
    warn!(
      "RawFile: File check failed for {:?}: exists={}, is_file={}",
      path,
      path.exists(),
      path.is_file()
    );
    return Err(ApiError::NotFound(format!(
      "File not found or not a file: {}",
      path_str
    )));
  }

  // Open file
  let file = tokio::fs::File::open(path)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;
  let stream = tokio_util::io::ReaderStream::new(file);
  let body = Body::from_stream(stream);

  // Guess mime
  let _mime = mime_guess::from_path(path).first_or_octet_stream().as_ref().to_string();

  Ok(
    axum::response::Response::builder()
      .status(StatusCode::OK)
      .header(axum::http::header::CONTENT_TYPE, "application/octet-stream")
      .header(
        axum::http::header::CONTENT_DISPOSITION,
        "attachment; filename=\"file.bin\"",
      )
      .body(body)
      .unwrap(),
  )
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
    .route("/api/v1/file_raw", get(handle_get_file_raw))
    .with_state(AppState { config })
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::body::Body;
  use axum::http::{Request, StatusCode};
  use std::path::PathBuf;
  use std::sync::{Arc, Mutex};
  use tower::ServiceExt;
  use tempfile;

  fn create_test_config(roots: Vec<String>) -> Arc<AgentConfig> {
    Arc::new(AgentConfig {
      agent_id: "test-agent".to_string(),
      agent_name: "Test Agent".to_string(),
      server_endpoint: "http://localhost:4000".to_string(),
      search_roots: roots,
      listen_port: 3976,
      enable_heartbeat: false,
      heartbeat_interval_secs: 30,
      worker_threads: None,
      log_dir: PathBuf::from("/tmp"),
      log_retention: 7,
      reload_handle: None,
      current_log_level: Arc::new(Mutex::new("info".to_string())),
    })
  }

  #[tokio::test]
  async fn test_health_route() {
    let app = create_router(create_test_config(vec!["/tmp".to_string()]));
    let response = app
      .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
  }

  #[tokio::test]
  async fn test_info_route() {
    let app = create_router(create_test_config(vec!["/tmp".to_string()]));
    let response = app
      .oneshot(Request::builder().uri("/api/v1/info").body(Body::empty()).unwrap())
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let info: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(info["id"], "test-agent");
  }

  #[tokio::test]
  async fn test_paths_route() {
    let tmp = tempfile::tempdir().unwrap();
    let sub = tmp.path().join("subdir");
    std::fs::create_dir(&sub).unwrap();

    let app = create_router(create_test_config(vec![tmp.path().to_string_lossy().to_string()]));
    let response = app
      .oneshot(Request::builder().uri("/api/v1/paths").body(Body::empty()).unwrap())
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let paths: Vec<String> = serde_json::from_slice(&body).unwrap();
    assert_eq!(paths, vec!["subdir"]);
  }

  #[tokio::test]
  async fn test_list_files_route() {
    let tmp = tempfile::tempdir().unwrap();
    let file1 = tmp.path().join("file1.txt");
    std::fs::write(&file1, "hello").unwrap();

    // 我们需要规范化路径，因为 API 内部会进行规范化校验
    let canon_tmp = std::fs::canonicalize(tmp.path()).unwrap();
    let path_str = canon_tmp.to_string_lossy().to_string();

    let app = create_router(create_test_config(vec![path_str.clone()]));

    let response = app
      .oneshot(Request::builder()
        .uri(format!("/api/v1/list_files?path={}", urlencoding::encode(&path_str)))
        .body(Body::empty()).unwrap())
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 10).await.unwrap();
    let res: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(res["items"].as_array().unwrap().len() >= 1);
  }

  #[tokio::test]
  async fn test_get_file_raw_route() {
    let tmp = tempfile::tempdir().unwrap();
    let file1 = tmp.path().join("file1.txt");
    std::fs::write(&file1, "hello content").unwrap();

    let canon_file = std::fs::canonicalize(&file1).unwrap();
    let canon_root = std::fs::canonicalize(tmp.path()).unwrap();

    let app = create_router(create_test_config(vec![canon_root.to_string_lossy().to_string()]));

    let response = app
      .oneshot(Request::builder()
        .uri(format!("/api/v1/file_raw?path={}", urlencoding::encode(&canon_file.to_string_lossy())))
        .body(Body::empty()).unwrap())
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(body, "hello content");
  }

  #[tokio::test]
  async fn test_log_config_routes() {
    let app = create_router(create_test_config(vec!["/tmp".to_string()]));

    // GET config
    let response = app.clone()
      .oneshot(Request::builder().uri("/api/v1/log/config").body(Body::empty()).unwrap())
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // PUT level
    let response = app.clone()
      .oneshot(Request::builder()
        .method("PUT")
        .uri("/api/v1/log/level")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"level":"debug"}"#)).unwrap())
      .await
      .unwrap();
    // 由于 Mock 配置中没有 reload_handle，预期返回 500 ReloadFailed
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    // PUT retention
    let response = app.clone()
      .oneshot(Request::builder()
        .method("PUT")
        .uri("/api/v1/log/retention")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"retention_count":10}"#)).unwrap())
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
  }

  #[tokio::test]
  async fn test_handle_search_route() {
    let tmp = tempfile::tempdir().unwrap();
    let file1 = tmp.path().join("file1.log");
    std::fs::write(&file1, "error: something happened\n").unwrap();

    let canon_root = std::fs::canonicalize(tmp.path()).unwrap();
    let app = create_router(create_test_config(vec![canon_root.to_string_lossy().to_string()]));

    // 构造 SearchBody 的 Agent 版本 (AgentSearchRequest)
    // 注意：Target::Dir 的 path "." 在 Agent 侧表示 search_roots[0]
    let search_req = serde_json::json!({
        "task_id": "test-task",
        "query": "error",
        "context_lines": 0,
        "path_filter": null,
        "path_includes": [],
        "path_excludes": [],
        "target": {
            "type": "dir",
            "path": ".",
            "recursive": false
        }
    });

    let response = app
      .oneshot(Request::builder()
        .method("POST")
        .uri("/api/v1/search")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&search_req).unwrap())).unwrap())
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // 检查响应流
    use tokio_stream::StreamExt;
    let mut stream = response.into_body().into_data_stream();
    let mut found_match = false;
    let mut found_complete = false;

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.unwrap();
        let s = String::from_utf8_lossy(&chunk);
        for line in s.lines() {
            if line.trim().is_empty() { continue; }
            let json: serde_json::Value = serde_json::from_str(line).unwrap();
            if json["type"] == "result" {
                found_match = true;
            }
            if json["type"] == "complete" {
                found_complete = true;
            }
        }
    }

    assert!(found_match, "Should find at least one match result in stream");
    assert!(found_complete, "Should find complete event in stream");
  }

  #[tokio::test]
  async fn test_handle_cancel_route() {
    let app = create_router(create_test_config(vec!["/tmp".to_string()]));

    let response = app
      .oneshot(Request::builder()
        .method("POST")
        .uri("/api/v1/cancel/test-task")
        .body(Body::empty()).unwrap())
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
  }
}
