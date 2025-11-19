//! Agent 日志 API 集成测试
//!
//! 测试日志配置 API 的完整功能：
//! - 获取日志配置
//! - 更新日志级别
//! - 更新日志保留数量
//! - 参数验证

use axum::{
  body::Body,
  http::{Request, StatusCode},
};
use futures::future;
use opsbox_core::logging::LogLevel;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt; // for `oneshot`

/// 全局 reload handle（用于测试）
static RELOAD_HANDLE: std::sync::OnceLock<Arc<opsbox_core::logging::ReloadHandle>> = std::sync::OnceLock::new();

/// 初始化测试日志系统（只执行一次）
fn init_test_logging_once() -> Arc<opsbox_core::logging::ReloadHandle> {
  RELOAD_HANDLE
    .get_or_init(|| {
      let temp_dir = TempDir::new().expect("创建临时日志目录失败");
      let log_dir = temp_dir.path().join("logs");
      std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

      let log_config = opsbox_core::logging::LogConfig {
        level: LogLevel::Info,
        log_dir: log_dir.clone(),
        retention_count: 7,
        enable_console: true,
        enable_file: false,
        file_prefix: "test-agent".to_string(),
      };
      let reload_handle = Arc::new(opsbox_core::logging::init(log_config).expect("初始化日志系统失败"));

      // 防止 temp_dir 被 drop
      std::mem::forget(temp_dir);

      reload_handle
    })
    .clone()
}

/// 创建测试用的 AgentConfig
fn create_test_config(log_dir: std::path::PathBuf, log_retention: usize) -> Arc<opsbox_agent::AgentConfig> {
  // 初始化日志系统（只执行一次）并获取 reload handle
  let reload_handle = init_test_logging_once();

  use std::sync::Arc as StdArc;

  Arc::new(opsbox_agent::AgentConfig {
    agent_id: "test-agent".to_string(),
    agent_name: "Test Agent".to_string(),
    server_endpoint: "http://localhost:4000".to_string(),
    search_roots: vec!["/tmp".to_string()],
    listen_port: 4001,
    enable_heartbeat: true,
    heartbeat_interval_secs: 30,
    worker_threads: None,
    log_dir: log_dir.clone(),
    log_retention,
    reload_handle: Some(reload_handle.clone()),
    current_log_level: StdArc::new(std::sync::Mutex::new("info".to_string())),
  })
}

/// 创建测试路由
fn create_test_router(config: Arc<opsbox_agent::AgentConfig>) -> axum::Router {
  use axum::routing::{get, put};

  axum::Router::new()
    .route("/api/v1/log/config", get(opsbox_agent::routes::get_log_config))
    .route("/api/v1/log/level", put(opsbox_agent::routes::update_log_level))
    .route("/api/v1/log/retention", put(opsbox_agent::routes::update_log_retention))
    .with_state(opsbox_agent::AppState { config })
}

#[tokio::test]
async fn test_get_log_config_success() {
  // 创建测试环境
  let temp_dir = TempDir::new().expect("创建临时目录失败");
  let log_dir = temp_dir.path().join("logs");
  std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

  let config = create_test_config(log_dir.clone(), 7);
  let app = create_test_router(config);

  // 发送请求
  let request = Request::builder()
    .uri("/api/v1/log/config")
    .body(Body::empty())
    .unwrap();

  let response = app.oneshot(request).await.unwrap();

  // 验证响应
  assert_eq!(response.status(), StatusCode::OK);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

  assert_eq!(json["level"], "info");
  assert_eq!(json["retention_count"], 7);
  assert_eq!(json["log_dir"], log_dir.to_str().unwrap());
}

#[tokio::test]
async fn test_update_log_level_success() {
  // 创建测试环境
  let temp_dir = TempDir::new().expect("创建临时目录失败");
  let log_dir = temp_dir.path().join("logs");
  std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

  let config = create_test_config(log_dir, 7);
  let app = create_test_router(config);

  // 测试更新为 DEBUG 级别
  let request = Request::builder()
    .method("PUT")
    .uri("/api/v1/log/level")
    .header("content-type", "application/json")
    .body(Body::from(json!({"level": "debug"}).to_string()))
    .unwrap();

  let response = app.oneshot(request).await.unwrap();

  // 验证响应
  assert_eq!(response.status(), StatusCode::OK);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

  assert!(json["message"].as_str().unwrap().contains("debug"));
}

#[tokio::test]
async fn test_update_log_level_all_levels() {
  // 创建测试环境
  let temp_dir = TempDir::new().expect("创建临时目录失败");
  let log_dir = temp_dir.path().join("logs");
  std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

  let config = create_test_config(log_dir, 7);
  let app = create_test_router(config);

  // 测试所有日志级别
  let levels = vec!["error", "warn", "info", "debug", "trace"];

  for level in levels {
    let request = Request::builder()
      .method("PUT")
      .uri("/api/v1/log/level")
      .header("content-type", "application/json")
      .body(Body::from(json!({"level": level}).to_string()))
      .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "更新日志级别 {} 失败", level);
  }
}

#[tokio::test]
async fn test_update_log_level_invalid() {
  // 创建测试环境
  let temp_dir = TempDir::new().expect("创建临时目录失败");
  let log_dir = temp_dir.path().join("logs");
  std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

  let config = create_test_config(log_dir, 7);
  let app = create_test_router(config);

  // 测试无效的日志级别
  let request = Request::builder()
    .method("PUT")
    .uri("/api/v1/log/level")
    .header("content-type", "application/json")
    .body(Body::from(json!({"level": "invalid"}).to_string()))
    .unwrap();

  let response = app.oneshot(request).await.unwrap();

  // 验证返回 400 错误
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

  assert!(json["error"].as_str().unwrap().contains("无效的日志级别"));
}

#[tokio::test]
async fn test_update_log_retention_success() {
  // 创建测试环境
  let temp_dir = TempDir::new().expect("创建临时目录失败");
  let log_dir = temp_dir.path().join("logs");
  std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

  let config = create_test_config(log_dir, 7);
  let app = create_test_router(config);

  // 测试更新保留数量
  let request = Request::builder()
    .method("PUT")
    .uri("/api/v1/log/retention")
    .header("content-type", "application/json")
    .body(Body::from(json!({"retention_count": 30}).to_string()))
    .unwrap();

  let response = app.oneshot(request).await.unwrap();

  // 验证响应
  assert_eq!(response.status(), StatusCode::OK);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

  assert!(json["message"].as_str().unwrap().contains("30"));
}

#[tokio::test]
async fn test_update_log_retention_boundary_values() {
  // 创建测试环境
  let temp_dir = TempDir::new().expect("创建临时目录失败");
  let log_dir = temp_dir.path().join("logs");
  std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

  let config = create_test_config(log_dir, 7);
  let app = create_test_router(config);

  // 测试边界值：最小值 1
  let request = Request::builder()
    .method("PUT")
    .uri("/api/v1/log/retention")
    .header("content-type", "application/json")
    .body(Body::from(json!({"retention_count": 1}).to_string()))
    .unwrap();

  let response = app.clone().oneshot(request).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);

  // 测试边界值：最大值 365
  let request = Request::builder()
    .method("PUT")
    .uri("/api/v1/log/retention")
    .header("content-type", "application/json")
    .body(Body::from(json!({"retention_count": 365}).to_string()))
    .unwrap();

  let response = app.oneshot(request).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_update_log_retention_invalid_zero() {
  // 创建测试环境
  let temp_dir = TempDir::new().expect("创建临时目录失败");
  let log_dir = temp_dir.path().join("logs");
  std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

  let config = create_test_config(log_dir, 7);
  let app = create_test_router(config);

  // 测试无效值：0
  let request = Request::builder()
    .method("PUT")
    .uri("/api/v1/log/retention")
    .header("content-type", "application/json")
    .body(Body::from(json!({"retention_count": 0}).to_string()))
    .unwrap();

  let response = app.oneshot(request).await.unwrap();

  // 验证返回 400 错误
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

  assert!(json["error"].as_str().unwrap().contains("保留数量必须在 1-365 之间"));
}

#[tokio::test]
async fn test_update_log_retention_invalid_too_large() {
  // 创建测试环境
  let temp_dir = TempDir::new().expect("创建临时目录失败");
  let log_dir = temp_dir.path().join("logs");
  std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

  let config = create_test_config(log_dir, 7);
  let app = create_test_router(config);

  // 测试无效值：超过 365
  let request = Request::builder()
    .method("PUT")
    .uri("/api/v1/log/retention")
    .header("content-type", "application/json")
    .body(Body::from(json!({"retention_count": 366}).to_string()))
    .unwrap();

  let response = app.oneshot(request).await.unwrap();

  // 验证返回 400 错误
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

  assert!(json["error"].as_str().unwrap().contains("保留数量必须在 1-365 之间"));
}

#[tokio::test]
async fn test_concurrent_updates() {
  // 创建测试环境
  let temp_dir = TempDir::new().expect("创建临时目录失败");
  let log_dir = temp_dir.path().join("logs");
  std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

  let config = create_test_config(log_dir, 7);
  let app = create_test_router(config);

  // 并发更新日志级别
  let mut handles = vec![];
  let levels = vec!["debug", "info", "warn", "error", "trace"];

  for level in levels {
    let app_clone = app.clone();
    let level_str = level.to_string();

    let handle = tokio::spawn(async move {
      let request = Request::builder()
        .method("PUT")
        .uri("/api/v1/log/level")
        .header("content-type", "application/json")
        .body(Body::from(json!({"level": level_str}).to_string()))
        .unwrap();

      app_clone.oneshot(request).await.unwrap()
    });

    handles.push(handle);
  }

  // 等待所有请求完成
  let results = future::join_all(handles).await;

  // 验证所有请求都成功
  for result in results {
    let response = result.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
  }
}

#[tokio::test]
async fn test_malformed_json_requests() {
  // 创建测试环境
  let temp_dir = TempDir::new().expect("创建临时目录失败");
  let log_dir = temp_dir.path().join("logs");
  std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

  let config = create_test_config(log_dir, 7);
  let app = create_test_router(config);

  // 测试格式错误的 JSON
  let request = Request::builder()
    .method("PUT")
    .uri("/api/v1/log/level")
    .header("content-type", "application/json")
    .body(Body::from("{invalid json"))
    .unwrap();

  let response = app.clone().oneshot(request).await.unwrap();

  // 验证返回错误（400 或 422）
  assert!(response.status() == StatusCode::BAD_REQUEST || response.status() == StatusCode::UNPROCESSABLE_ENTITY);

  // 测试缺少必需字段
  let request = Request::builder()
    .method("PUT")
    .uri("/api/v1/log/level")
    .header("content-type", "application/json")
    .body(Body::from(json!({"wrong_field": "debug"}).to_string()))
    .unwrap();

  let response = app.oneshot(request).await.unwrap();

  // 验证返回错误
  assert!(response.status() == StatusCode::BAD_REQUEST || response.status() == StatusCode::UNPROCESSABLE_ENTITY);
}
