//! Agent Manager 日志代理集成测试
//!
//! 测试通过 Agent Manager 代理访问 Agent 日志配置的功能：
//! - 代理获取配置
//! - 代理更新级别
//! - 代理更新保留数量
//! - Agent 离线场景
//! - Agent 不存在场景

use agent_manager::manager::AgentManager;
use agent_manager::models::{AgentInfo, AgentStatus, AgentTag};
use agent_manager::routes::create_routes;
use axum::{
  body::Body,
  http::{Request, StatusCode},
};
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt; // for `oneshot`

/// 创建测试用的 Agent Manager
async fn create_test_manager() -> Arc<AgentManager> {
  let pool = sqlx::sqlite::SqlitePool::connect("sqlite::memory:")
    .await
    .expect("创建内存数据库失败");

  Arc::new(AgentManager::new(pool).await.expect("创建 AgentManager 失败"))
}

/// 创建测试用的 Agent 信息
fn create_test_agent(id: &str, host: &str, port: u16) -> AgentInfo {
  AgentInfo {
    id: id.to_string(),
    name: format!("Test Agent {}", id),
    version: "1.0.0".to_string(),
    hostname: "localhost".to_string(),
    tags: vec![
      AgentTag::new("host".to_string(), host.to_string()),
      AgentTag::new("listen_port".to_string(), port.to_string()),
    ],
    search_roots: vec!["/tmp".to_string()],
    last_heartbeat: chrono::Utc::now().timestamp(),
    status: AgentStatus::Online,
  }
}

/// 启动模拟 Agent 服务器
async fn start_mock_agent_server(port: u16) -> tokio::task::JoinHandle<()> {
  use axum::Router;
  use axum::routing::{get, put};

  let app = Router::new()
    .route(
      "/api/v1/log/config",
      get(|| async {
        axum::Json(json!({
            "level": "info",
            "retention_count": 7,
            "log_dir": "/tmp/logs"
        }))
      }),
    )
    .route(
      "/api/v1/log/level",
      put(|axum::Json(payload): axum::Json<serde_json::Value>| async move {
        axum::Json(json!({
            "message": format!("日志级别已更新为: {}", payload["level"])
        }))
      }),
    )
    .route(
      "/api/v1/log/retention",
      put(|axum::Json(payload): axum::Json<serde_json::Value>| async move {
        axum::Json(json!({
            "message": format!("日志保留数量已更新为: {} 天", payload["retention_count"])
        }))
      }),
    );

  tokio::spawn(async move {
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
      .await
      .expect("绑定端口失败");

    axum::serve(listener, app.into_make_service())
      .await
      .expect("启动模拟服务器失败");
  })
}

#[tokio::test]
async fn test_proxy_get_log_config_success() {
  // 启动模拟 Agent 服务器
  let port = 14001;
  let _server = start_mock_agent_server(port).await;

  // 等待服务器启动
  tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

  // 创建 Agent Manager 并注册 Agent
  let manager = create_test_manager().await;
  let agent = create_test_agent("test-agent-1", "127.0.0.1", port);
  manager.register_agent(agent).await.expect("注册 Agent 失败");

  // 创建路由
  let app = create_routes(manager);

  // 发送代理请求
  let request = Request::builder()
    .uri("/test-agent-1/log/config")
    .body(Body::empty())
    .unwrap();

  let response = app.oneshot(request).await.unwrap();

  // 验证响应
  assert_eq!(response.status(), StatusCode::OK);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

  assert_eq!(json["level"], "info");
  assert_eq!(json["retention_count"], 7);
  assert_eq!(json["log_dir"], "/tmp/logs");
}

#[tokio::test]
async fn test_proxy_update_log_level_success() {
  // 启动模拟 Agent 服务器
  let port = 14002;
  let _server = start_mock_agent_server(port).await;

  // 等待服务器启动
  tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

  // 创建 Agent Manager 并注册 Agent
  let manager = create_test_manager().await;
  let agent = create_test_agent("test-agent-2", "127.0.0.1", port);
  manager.register_agent(agent).await.expect("注册 Agent 失败");

  // 创建路由
  let app = create_routes(manager);

  // 发送代理请求
  let request = Request::builder()
    .method("PUT")
    .uri("/test-agent-2/log/level")
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
async fn test_proxy_update_log_retention_success() {
  // 启动模拟 Agent 服务器
  let port = 14003;
  let _server = start_mock_agent_server(port).await;

  // 等待服务器启动
  tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

  // 创建 Agent Manager 并注册 Agent
  let manager = create_test_manager().await;
  let agent = create_test_agent("test-agent-3", "127.0.0.1", port);
  manager.register_agent(agent).await.expect("注册 Agent 失败");

  // 创建路由
  let app = create_routes(manager);

  // 发送代理请求
  let request = Request::builder()
    .method("PUT")
    .uri("/test-agent-3/log/retention")
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
async fn test_proxy_agent_not_found() {
  // 创建 Agent Manager（不注册任何 Agent）
  let manager = create_test_manager().await;

  // 创建路由
  let app = create_routes(manager);

  // 发送代理请求到不存在的 Agent
  let request = Request::builder()
    .uri("/non-existent-agent/log/config")
    .body(Body::empty())
    .unwrap();

  let response = app.oneshot(request).await.unwrap();

  // 验证返回 404 错误
  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
  let error_text = String::from_utf8(body.to_vec()).unwrap();

  assert!(error_text.contains("不存在"));
}

#[tokio::test]
async fn test_proxy_agent_offline() {
  // 创建 Agent Manager 并注册 Agent（但不启动服务器）
  let manager = create_test_manager().await;
  let agent = create_test_agent("offline-agent", "127.0.0.1", 19999); // 使用未监听的端口
  manager.register_agent(agent).await.expect("注册 Agent 失败");

  // 创建路由
  let app = create_routes(manager);

  // 发送代理请求
  let request = Request::builder()
    .uri("/offline-agent/log/config")
    .body(Body::empty())
    .unwrap();

  let response = app.oneshot(request).await.unwrap();

  // 验证返回 502 Bad Gateway 错误
  assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
  let error_text = String::from_utf8(body.to_vec()).unwrap();

  // 验证错误消息包含连接失败或 Agent 错误的信息
  assert!(
    error_text.contains("无法连接到 Agent")
      || error_text.contains("Connection refused")
      || error_text.contains("Agent 返回错误"),
    "Expected connection or agent error, got: {}",
    error_text
  );
}

#[tokio::test]
async fn test_proxy_agent_missing_host_tag() {
  // 创建 Agent Manager 并注册 Agent（缺少 host 标签）
  let manager = create_test_manager().await;
  let mut agent = create_test_agent("no-host-agent", "127.0.0.1", 14004);
  agent.tags.retain(|t| t.key != "host"); // 移除 host 标签

  manager.register_agent(agent).await.expect("注册 Agent 失败");

  // 创建路由
  let app = create_routes(manager);

  // 发送代理请求
  let request = Request::builder()
    .uri("/no-host-agent/log/config")
    .body(Body::empty())
    .unwrap();

  let response = app.oneshot(request).await.unwrap();

  // 验证返回 500 错误
  assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
  let error_text = String::from_utf8(body.to_vec()).unwrap();

  assert!(error_text.contains("缺少 host 标签"));
}

#[tokio::test]
async fn test_proxy_multiple_agents() {
  // 启动多个模拟 Agent 服务器
  let port1 = 14005;
  let port2 = 14006;
  let _server1 = start_mock_agent_server(port1).await;
  let _server2 = start_mock_agent_server(port2).await;

  // 等待服务器启动
  tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

  // 创建 Agent Manager 并注册多个 Agent
  let manager = create_test_manager().await;
  let agent1 = create_test_agent("agent-1", "127.0.0.1", port1);
  let agent2 = create_test_agent("agent-2", "127.0.0.1", port2);

  manager.register_agent(agent1).await.expect("注册 Agent 1 失败");
  manager.register_agent(agent2).await.expect("注册 Agent 2 失败");

  // 创建路由
  let app = create_routes(manager);

  // 测试访问第一个 Agent
  let request = Request::builder()
    .uri("/agent-1/log/config")
    .body(Body::empty())
    .unwrap();

  let response = app.clone().oneshot(request).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);

  // 测试访问第二个 Agent
  let request = Request::builder()
    .uri("/agent-2/log/config")
    .body(Body::empty())
    .unwrap();

  let response = app.oneshot(request).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_proxy_concurrent_requests() {
  // 启动模拟 Agent 服务器
  let port = 14007;
  let _server = start_mock_agent_server(port).await;

  // 等待服务器启动
  tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

  // 创建 Agent Manager 并注册 Agent
  let manager = create_test_manager().await;
  let agent = create_test_agent("concurrent-agent", "127.0.0.1", port);
  manager.register_agent(agent).await.expect("注册 Agent 失败");

  // 创建路由
  let app = create_routes(manager);

  // 并发发送多个请求
  let mut handles = vec![];

  for i in 0..5 {
    let app_clone = app.clone();
    let level = match i % 3 {
      0 => "debug",
      1 => "info",
      _ => "warn",
    };

    let handle = tokio::spawn(async move {
      let request = Request::builder()
        .method("PUT")
        .uri("/concurrent-agent/log/level")
        .header("content-type", "application/json")
        .body(Body::from(json!({"level": level}).to_string()))
        .unwrap();

      app_clone.oneshot(request).await.unwrap()
    });

    handles.push(handle);
  }

  // 等待所有请求完成
  let results = futures::future::join_all(handles).await;

  // 验证所有请求都成功
  for result in results {
    let response = result.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
  }
}

#[tokio::test]
async fn test_proxy_timeout_scenario() {
  // 创建一个会超时的模拟服务器
  let port = 14008;
  let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
    .await
    .expect("绑定端口失败");

  // 启动一个永远不响应的服务器
  tokio::spawn(async move {
    loop {
      if let Ok((mut _socket, _)) = listener.accept().await {
        // 接受连接但不响应，模拟超时
        tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
      }
    }
  });

  // 等待服务器启动
  tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

  // 创建 Agent Manager 并注册 Agent
  let manager = create_test_manager().await;
  let agent = create_test_agent("timeout-agent", "127.0.0.1", port);
  manager.register_agent(agent).await.expect("注册 Agent 失败");

  // 创建路由
  let app = create_routes(manager);

  // 发送代理请求
  let request = Request::builder()
    .uri("/timeout-agent/log/config")
    .body(Body::empty())
    .unwrap();

  let response = app.oneshot(request).await.unwrap();

  // 验证返回 502 Bad Gateway 错误（超时）
  assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
  let error_text = String::from_utf8(body.to_vec()).unwrap();

  assert!(error_text.contains("无法连接到 Agent"));
}

#[tokio::test]
async fn test_proxy_all_log_levels() {
  // 启动模拟 Agent 服务器
  let port = 14009;
  let _server = start_mock_agent_server(port).await;

  // 等待服务器启动
  tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

  // 创建 Agent Manager 并注册 Agent
  let manager = create_test_manager().await;
  let agent = create_test_agent("levels-agent", "127.0.0.1", port);
  manager.register_agent(agent).await.expect("注册 Agent 失败");

  // 创建路由
  let app = create_routes(manager);

  // 测试所有日志级别
  let levels = vec!["error", "warn", "info", "debug", "trace"];

  for level in levels {
    let request = Request::builder()
      .method("PUT")
      .uri("/levels-agent/log/level")
      .header("content-type", "application/json")
      .body(Body::from(json!({"level": level}).to_string()))
      .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "代理更新日志级别 {} 失败", level);
  }
}

#[tokio::test]
async fn test_proxy_retention_boundary_values() {
  // 启动模拟 Agent 服务器
  let port = 14010;
  let _server = start_mock_agent_server(port).await;

  // 等待服务器启动
  tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

  // 创建 Agent Manager 并注册 Agent
  let manager = create_test_manager().await;
  let agent = create_test_agent("retention-agent", "127.0.0.1", port);
  manager.register_agent(agent).await.expect("注册 Agent 失败");

  // 创建路由
  let app = create_routes(manager);

  // 测试边界值：最小值 1
  let request = Request::builder()
    .method("PUT")
    .uri("/retention-agent/log/retention")
    .header("content-type", "application/json")
    .body(Body::from(json!({"retention_count": 1}).to_string()))
    .unwrap();

  let response = app.clone().oneshot(request).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);

  // 测试边界值：最大值 365
  let request = Request::builder()
    .method("PUT")
    .uri("/retention-agent/log/retention")
    .header("content-type", "application/json")
    .body(Body::from(json!({"retention_count": 365}).to_string()))
    .unwrap();

  let response = app.oneshot(request).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
}
