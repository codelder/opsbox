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

use crate::api::{AppState, LogConfigResponse, SuccessResponse, UpdateLogLevelRequest, UpdateRetentionRequest};
use crate::config::AgentConfig;
use crate::path::get_available_subdirs;
use crate::search::execute_search;
use axum::Router;
use opsbox_core::error::{AppError, Result};
use std::sync::Arc;

/// 解码 URL 编码的路径
fn decode_path(path: &str) -> String {
  urlencoding::decode(path)
    .map(|s| s.into_owned())
    .unwrap_or_else(|_| path.to_string())
}

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
pub async fn get_log_config(State(state): State<AppState>) -> Result<Json<LogConfigResponse>> {
  let current_level = state.config.current_log_level.lock().await.clone();
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
) -> Result<Json<SuccessResponse<()>>> {
  // 验证日志级别
  let level = LogLevel::from_str(&req.level).map_err(|e| AppError::bad_request(format!("无效的日志级别: {}", e)))?;

  // 动态重载日志级别
  let reload_handle = state
    .config
    .get_reload_handle()
    .ok_or_else(|| AppError::internal("日志系统未初始化".to_string()))?;

  reload_handle
    .update_level(level)
    .map_err(|e| AppError::internal(format!("重载失败: {}", e)))?;

  // 更新保存的当前日志级别
  *state.config.current_log_level.lock().await = req.level.clone();

  info!("日志级别已更新为: {}", level);

  Ok(Json(SuccessResponse::<()>::with_message(format!(
    "日志级别已更新为: {}",
    level
  ))))
}

/// 更新日志保留数量
pub async fn update_log_retention(
  State(_state): State<AppState>,
  Json(req): Json<UpdateRetentionRequest>,
) -> Result<Json<SuccessResponse<()>>> {
  // 验证保留数量
  if req.retention_count == 0 || req.retention_count > 365 {
    return Err(AppError::bad_request("保留数量必须在 1-365 之间".to_string()));
  }

  // 注意：Agent 不持久化配置到数据库，仅在内存中更新
  // 重启后会使用命令行参数指定的值
  info!("日志保留数量已更新为: {} 天（重启后失效）", req.retention_count);

  Ok(Json(SuccessResponse::<()>::with_message(format!(
    "日志保留数量已更新为: {} 天（重启后失效）",
    req.retention_count
  ))))
}

/// 列出目录文件（支持归档浏览）
pub async fn handle_list_files(
  State(state): State<AppState>,
  Query(req): Query<AgentListRequest>,
) -> Result<Json<AgentListResponse>> {
  fn normalize_agent_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("//?/").to_string()
  }

  let path_str = decode_path(&req.path);

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
            path: normalize_agent_path(&abs_path.to_string_lossy()),
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

  // 使用 AgentExplorer 进行列表操作（支持归档）
  use crate::explorer::AgentExplorer;
  let explorer = AgentExplorer::new(state.config.clone());

  // 解码 entry 参数（如果有）
  let entry = req.entry.as_ref().map(|e| decode_path(e));

  match explorer.list(&path_str, entry.as_deref()).await {
    Ok(entries) => {
      let items = entries
        .into_iter()
        .map(|entry| AgentFileItem {
          name: entry.name,
          path: normalize_agent_path(&entry.path),
          is_dir: entry.is_dir,
          is_symlink: entry.is_symlink,
          size: Some(entry.size),
          modified: entry.modified.map(|t| t as i64),
          child_count: entry.child_count.map(|c| c as u32),
          hidden_child_count: entry.hidden_child_count.map(|c| c as u32),
          mime_type: entry.mime_type,
        })
        .collect();
      Ok(Json(AgentListResponse { items }))
    }
    Err(e) => {
      // 访问被拒绝或路径不在允许范围内，统一返回 NotFound 避免泄露信息
      Err(AppError::not_found(format!("Access denied or path not found: {}", e)))
    }
  }
}

use axum::body::Body;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct GetFileRequest {
  pub path: String,
  /// 归档内路径（可选）
  pub entry: Option<String>,
}

/// GET 原始文件内容（支持归档内文件下载）
pub async fn handle_get_file_raw(
  State(state): State<AppState>,
  Query(req): Query<GetFileRequest>,
) -> Result<impl IntoResponse> {
  let path_str = decode_path(&req.path);

  // 使用 AgentExplorer 进行下载操作（支持归档）
  use crate::explorer::AgentExplorer;
  let explorer = AgentExplorer::new(state.config.clone());

  // 解码 entry 参数（如果有）
  let entry = req.entry.as_ref().map(|e| decode_path(e));

  match explorer.download(&path_str, entry.as_deref()).await {
    Ok((name, size, reader)) => {
      // 获取 MIME 类型
      let mime = AgentExplorer::guess_mime_type(&name).unwrap_or_else(|| "application/octet-stream".to_string());

      // 创建流式响应
      use tokio_util::io::ReaderStream;
      let stream = ReaderStream::new(reader);
      let body = Body::from_stream(stream);

      // URL 编码文件名用于 Content-Disposition
      let encoded_name = urlencoding::encode(&name);

      Ok(
        axum::response::Response::builder()
          .status(StatusCode::OK)
          .header(axum::http::header::CONTENT_TYPE, mime)
          .header(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"; filename*=UTF-8''{}", name, encoded_name),
          )
          .header(axum::http::header::CONTENT_LENGTH, size.unwrap_or(0))
          .body(body)
          .unwrap(),
      )
    }
    Err(e) => {
      warn!("RawFile: Download failed for {}: {}", path_str, e);
      Err(AppError::not_found(format!("Access denied or file not found: {}", e)))
    }
  }
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
  use axum::response::Response;
  use std::path::PathBuf;
  use std::sync::Arc;
  use tokio::sync::Mutex;
  use tower::ServiceExt;

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

  async fn collect_ndjson_events(response: Response) -> Vec<serde_json::Value> {
    use tokio_stream::StreamExt;

    let mut events = Vec::new();
    let mut stream = response.into_body().into_data_stream();
    while let Some(chunk_res) = stream.next().await {
      let chunk = chunk_res.unwrap();
      let text = String::from_utf8_lossy(&chunk);
      for line in text.lines() {
        if line.trim().is_empty() {
          continue;
        }
        let json: serde_json::Value = serde_json::from_str(line).unwrap();
        events.push(json);
      }
    }
    events
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
      .oneshot(
        Request::builder()
          .uri(format!("/api/v1/list_files?path={}", urlencoding::encode(&path_str)))
          .body(Body::empty())
          .unwrap(),
      )
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 10).await.unwrap();
    let res: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(!res["items"].as_array().unwrap().is_empty());
  }

  #[cfg(not(windows))]
  #[tokio::test]
  async fn test_list_files_route_rejects_invalid_absolute_parent() {
    let tmp = tempfile::tempdir().unwrap();
    let outer_parent = tmp.path().join("outer_parent");
    let allowed_root = outer_parent.join("allowed_root");
    std::fs::create_dir_all(&allowed_root).unwrap();

    let normalized_parent = outer_parent
      .strip_prefix(std::path::Path::new("/"))
      .expect("temp dir should be absolute on unix");
    let trap_dir = allowed_root.join(normalized_parent).join("codelder");
    std::fs::create_dir_all(&trap_dir).unwrap();

    let app = create_router(create_test_config(vec![allowed_root.to_string_lossy().to_string()]));

    let response = app
      .oneshot(
        Request::builder()
          .uri(format!(
            "/api/v1/list_files?path={}",
            urlencoding::encode(&outer_parent.to_string_lossy())
          ))
          .body(Body::empty())
          .unwrap(),
      )
      .await
      .unwrap();

    assert_eq!(
      response.status(),
      StatusCode::NOT_FOUND,
      "absolute parent outside search_roots should fail instead of listing trap dir {:?}",
      trap_dir
    );
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
      .oneshot(
        Request::builder()
          .uri(format!(
            "/api/v1/file_raw?path={}",
            urlencoding::encode(&canon_file.to_string_lossy())
          ))
          .body(Body::empty())
          .unwrap(),
      )
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
    let response = app
      .clone()
      .oneshot(
        Request::builder()
          .uri("/api/v1/log/config")
          .body(Body::empty())
          .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // PUT level
    let response = app
      .clone()
      .oneshot(
        Request::builder()
          .method("PUT")
          .uri("/api/v1/log/level")
          .header("content-type", "application/json")
          .body(Body::from(r#"{"level":"debug"}"#))
          .unwrap(),
      )
      .await
      .unwrap();
    // 由于 Mock 配置中没有 reload_handle，预期返回 500 ReloadFailed
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    // PUT retention
    let response = app
      .clone()
      .oneshot(
        Request::builder()
          .method("PUT")
          .uri("/api/v1/log/retention")
          .header("content-type", "application/json")
          .body(Body::from(r#"{"retention_count":10}"#))
          .unwrap(),
      )
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
      .oneshot(
        Request::builder()
          .method("POST")
          .uri("/api/v1/search")
          .header("content-type", "application/json")
          .body(Body::from(serde_json::to_string(&search_req).unwrap()))
          .unwrap(),
      )
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let events = collect_ndjson_events(response).await;
    let found_match = events.iter().any(|e| e["type"] == "result");
    let found_complete = events.iter().any(|e| e["type"] == "complete");

    assert!(found_match, "Should find at least one match result in stream");
    assert!(found_complete, "Should find complete event in stream");
  }

  /// 测试 Target::Dir { path: "subdir" } 的端到端搜索行为
  /// 验证 resolve_target_paths 返回的完整路径不会被错误地再次拼接
  /// 这是针对 P1 回归的集成测试
  #[tokio::test]
  async fn test_handle_search_dir_with_subdir() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    // 创建目录结构: root/app/logs/error.log
    let app_dir = root.join("app");
    let logs_dir = app_dir.join("logs");
    std::fs::create_dir_all(&logs_dir).unwrap();

    // 在 subdir 中创建包含关键字的文件
    let error_log = logs_dir.join("error.log");
    std::fs::write(&error_log, "2025-01-01 ERROR: critical failure in app\n").unwrap();

    // 同时在 root 创建一个文件（用于验证没有搜索到错误的位置）
    let root_log = root.join("root.log");
    std::fs::write(&root_log, "INFO: root level log\n").unwrap();

    let canon_root = std::fs::canonicalize(root).unwrap();
    let app = create_router(create_test_config(vec![canon_root.to_string_lossy().to_string()]));

    // 关键测试：使用 Target::Dir { path: "app/logs" }
    // resolve_target_paths 应该返回 /path/to/root/app/logs
    // execute_search 不应该再把 "app/logs" 拼接到这个路径上
    let search_req = serde_json::json!({
        "task_id": "test-subdir-task",
        "query": "ERROR",
        "context_lines": 0,
        "path_filter": null,
        "path_includes": [],
        "path_excludes": [],
        "target": {
            "type": "dir",
            "path": "app/logs",  // 关键：使用嵌套子目录
            "recursive": false
        }
    });

    let response = app
      .oneshot(
        Request::builder()
          .method("POST")
          .uri("/api/v1/search")
          .header("content-type", "application/json")
          .body(Body::from(serde_json::to_string(&search_req).unwrap()))
          .unwrap(),
      )
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let events = collect_ndjson_events(response).await;
    let mut found_match = false;
    let mut found_complete = false;

    for event in events {
      if event["type"] == "result" {
        found_match = true;
        // SearchResult 在 data 字段中
        let path = event["data"]["path"].as_str().unwrap_or("");
        // 验证结果来自正确的文件（在 app/logs 中）
        // 关键断言：路径包含 app/logs/error.log，而不是 app/logs/app/logs/error.log
        assert!(
          path.contains("error.log") && path.contains("logs"),
          "Result should be from app/logs/error.log, got path: {}",
          path
        );
        // 额外验证：确保没有重复拼接（例如 app/logs/app/logs）
        let path_str = path.to_string();
        assert!(
          !path_str.contains("app/logs/app") && !path_str.contains("logs/logs"),
          "Path should not contain duplicated path segments (indicating incorrect path concatenation): {}",
          path
        );
      }
      if event["type"] == "complete" {
        found_complete = true;
      }
      if event["type"] == "error" {
        panic!("Unexpected error in search: {:?}", event);
      }
    }

    // 关键断言：应该找到匹配结果（说明路径解析正确）
    assert!(
      found_match,
      "Should find match in app/logs/error.log - if this fails, path may be incorrectly concatenated"
    );
    assert!(found_complete, "Should find complete event in stream");
  }

  /// 测试 Target::Files 的端到端搜索行为
  /// 验证相对路径和多文件场景，断言结果不重复、都命中
  #[tokio::test]
  async fn test_handle_search_files_with_relative_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    // 创建多个文件
    let file1 = root.join("error1.log");
    let file2 = root.join("error2.log");
    let file3 = root.join("info.log"); // 不包含关键字的文件
    std::fs::write(&file1, "2025-01-01 ERROR: first error\n").unwrap();
    std::fs::write(&file2, "2025-01-01 ERROR: second error\n").unwrap();
    std::fs::write(&file3, "2025-01-01 INFO: info message\n").unwrap();

    let canon_root = std::fs::canonicalize(root).unwrap();
    let app = create_router(create_test_config(vec![canon_root.to_string_lossy().to_string()]));

    // 使用相对路径的多文件搜索
    let search_req = serde_json::json!({
        "task_id": "test-files-task",
        "query": "ERROR",
        "context_lines": 0,
        "path_filter": null,
        "path_includes": [],
        "path_excludes": [],
        "target": {
            "type": "files",
            "paths": ["error1.log", "error2.log", "info.log"]  // 相对路径
        }
    });

    let response = app
      .oneshot(
        Request::builder()
          .method("POST")
          .uri("/api/v1/search")
          .header("content-type", "application/json")
          .body(Body::from(serde_json::to_string(&search_req).unwrap()))
          .unwrap(),
      )
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    use std::collections::HashSet;
    let events = collect_ndjson_events(response).await;
    let mut found_files: HashSet<String> = HashSet::new();
    let mut result_count = 0usize;
    let mut found_complete = false;

    for event in events {
      if event["type"] == "result" {
        result_count += 1;
        let path = event["data"]["path"].as_str().unwrap_or("");
        if let Some(name) = std::path::Path::new(path).file_name().and_then(|n| n.to_str()) {
          found_files.insert(name.to_string());
        }
      }
      if event["type"] == "complete" {
        found_complete = true;
      }
      if event["type"] == "error" {
        panic!("Unexpected error in search: {:?}", event);
      }
    }

    // 断言：应该正好产生 2 条结果，且精确命中两个 error 文件
    assert_eq!(result_count, 2, "Should produce exactly 2 result events");
    let expected = HashSet::from(["error1.log".to_string(), "error2.log".to_string()]);
    assert_eq!(
      found_files, expected,
      "Results should exactly come from error1.log and error2.log"
    );
    assert!(found_complete, "Should find complete event");
  }

  /// 测试非法 query 返回 Error 事件
  /// 验证非法查询表达式返回 type=error 且 source=agent-query-parse
  #[tokio::test]
  async fn test_handle_search_invalid_query_returns_error() {
    let tmp = tempfile::tempdir().unwrap();
    let file1 = tmp.path().join("test.log");
    std::fs::write(&file1, "some content\n").unwrap();

    let canon_root = std::fs::canonicalize(tmp.path()).unwrap();
    let app = create_router(create_test_config(vec![canon_root.to_string_lossy().to_string()]));

    // 使用非法的查询表达式（无效的 glob 模式会导致 parse_github_like 失败）
    // path: 限定符会验证 glob 模式，[invalid 是无效的 glob 字符类
    let search_req = serde_json::json!({
        "task_id": "test-invalid-query",
        "query": "test path:[invalid",  // 无效的 glob 模式
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
      .oneshot(
        Request::builder()
          .method("POST")
          .uri("/api/v1/search")
          .header("content-type", "application/json")
          .body(Body::from(serde_json::to_string(&search_req).unwrap()))
          .unwrap(),
      )
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let events = collect_ndjson_events(response).await;
    let mut found_query_parse_error = false;
    let mut found_complete = false;
    let mut error_details: Option<String> = None;

    for event in events {
      if event["type"] == "error" {
        // source 在 data 对象里面
        let source = event["data"]["source"].as_str().unwrap_or("");
        error_details = Some(format!("{:?}", event));
        // 关键断言：错误来源应该是 agent-query-parse
        if source == "agent-query-parse" {
          found_query_parse_error = true;
        }
      }
      if event["type"] == "complete" {
        found_complete = true;
      }
    }

    // 关键断言：应该收到 agent-query-parse 错误事件
    assert!(
      found_query_parse_error,
      "Should receive error event with source='agent-query-parse' for invalid query. Got error: {:?}",
      error_details
    );
    // 非法查询应该立即终止，不应该有 complete 事件
    assert!(
      !found_complete,
      "Should NOT receive complete event for invalid query (should fail early)"
    );
  }

  /// 测试 Dir 的 recursive 语义
  /// 验证 recursive=true 递归扫描，recursive=false 只扫一层
  #[tokio::test]
  async fn test_handle_search_dir_recursive_semantics() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    // 创建目录结构:
    // root/level1.log (第一层)
    // root/subdir/level2.log (第二层)
    // root/subdir/nested/level3.log (第三层)
    let subdir = root.join("subdir");
    let nested = subdir.join("nested");
    std::fs::create_dir_all(&nested).unwrap();

    std::fs::write(root.join("level1.log"), "UNIQUE_KEYWORD at level1\n").unwrap();
    std::fs::write(subdir.join("level2.log"), "UNIQUE_KEYWORD at level2\n").unwrap();
    std::fs::write(nested.join("level3.log"), "UNIQUE_KEYWORD at level3\n").unwrap();

    let canon_root = std::fs::canonicalize(root).unwrap();
    let app = create_router(create_test_config(vec![canon_root.to_string_lossy().to_string()]));

    // 测试 1: recursive=false，应该只扫描目标目录（不进入子目录）
    let search_req_non_recursive = serde_json::json!({
        "task_id": "test-non-recursive",
        "query": "UNIQUE_KEYWORD",
        "context_lines": 0,
        "path_filter": null,
        "path_includes": [],
        "path_excludes": [],
        "target": {
            "type": "dir",
            "path": "subdir",  // 指定子目录
            "recursive": false  // 只扫 subdir 这一层
        }
    });

    let response = app
      .clone()
      .oneshot(
        Request::builder()
          .method("POST")
          .uri("/api/v1/search")
          .header("content-type", "application/json")
          .body(Body::from(serde_json::to_string(&search_req_non_recursive).unwrap()))
          .unwrap(),
      )
      .await
      .unwrap();

    use std::collections::HashSet;
    let events = collect_ndjson_events(response).await;
    let mut non_recursive_paths: HashSet<String> = HashSet::new();

    for event in events {
      if event["type"] == "result" {
        let path = event["data"]["path"].as_str().unwrap_or("").to_string();
        non_recursive_paths.insert(path);
      }
    }

    // recursive=false 应该只找到 subdir 下的 level2.log（不进入 nested）
    assert_eq!(
      non_recursive_paths.len(),
      1,
      "recursive=false should only find 1 result in subdir (not nested). Found: {:?}",
      non_recursive_paths
    );
    assert!(
      non_recursive_paths.iter().any(|p| p.contains("level2.log")),
      "Result should be level2.log, got: {:?}",
      non_recursive_paths
    );

    // 测试 2: recursive=true，应该递归扫描所有层级
    let search_req_recursive = serde_json::json!({
        "task_id": "test-recursive",
        "query": "UNIQUE_KEYWORD",
        "context_lines": 0,
        "path_filter": null,
        "path_includes": [],
        "path_excludes": [],
        "target": {
            "type": "dir",
            "path": "subdir",
            "recursive": true  // 递归扫描
        }
    });

    let response = app
      .oneshot(
        Request::builder()
          .method("POST")
          .uri("/api/v1/search")
          .header("content-type", "application/json")
          .body(Body::from(serde_json::to_string(&search_req_recursive).unwrap()))
          .unwrap(),
      )
      .await
      .unwrap();

    let events = collect_ndjson_events(response).await;
    let mut recursive_paths: HashSet<String> = HashSet::new();

    for event in events {
      if event["type"] == "result" {
        let path = event["data"]["path"].as_str().unwrap_or("").to_string();
        recursive_paths.insert(path);
      }
    }

    // recursive=true 应该找到 2 个结果（level2.log 和 level3.log）
    assert_eq!(
      recursive_paths.len(),
      2,
      "recursive=true should find 2 results in subdir and nested. Found: {:?}",
      recursive_paths
    );
    assert!(
      recursive_paths.iter().any(|p| p.contains("level2.log")),
      "Should find level2.log"
    );
    assert!(
      recursive_paths.iter().any(|p| p.contains("level3.log")),
      "Should find level3.log (in nested)"
    );
  }

  #[tokio::test]
  async fn test_handle_cancel_route() {
    let app = create_router(create_test_config(vec!["/tmp".to_string()]));

    let response = app
      .oneshot(
        Request::builder()
          .method("POST")
          .uri("/api/v1/cancel/test-task")
          .body(Body::empty())
          .unwrap(),
      )
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
  }
}
