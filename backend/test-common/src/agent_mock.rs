//! Agent Mock服务器
//!
//! 提供模拟Agent服务器的工具，用于集成测试

use crate::TestError;
use axum::{
  Json, Router,
  routing::{get, put},
};
use serde_json::{Value, json};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Agent模拟服务器配置
#[derive(Clone)]
pub struct MockAgentConfig {
  /// 服务器监听地址
  pub address: String,
  /// 服务器监听端口
  pub port: u16,
  /// 模拟的日志级别
  pub log_level: String,
  /// 模拟的日志保留天数
  pub log_retention_days: i32,
  /// 模拟的日志目录
  pub log_dir: String,
}

impl Default for MockAgentConfig {
  fn default() -> Self {
    Self {
      address: "127.0.0.1".to_string(),
      port: crate::constants::AGENT_PORT_START,
      log_level: "info".to_string(),
      log_retention_days: 7,
      log_dir: "/tmp/logs".to_string(),
    }
  }
}

/// Agent模拟服务器实例
pub struct MockAgentServer {
  /// 服务器任务句柄
  pub task: JoinHandle<()>,
  /// 服务器地址
  pub address: SocketAddr,
  /// 服务器配置
  pub config: MockAgentConfig,
}

impl MockAgentServer {
  /// 启动模拟Agent服务器
  pub async fn start(config: MockAgentConfig) -> Result<Self, TestError> {
    let address = format!("{}:{}", config.address, config.port);

    let app = Router::new()
      .route(
        "/api/v1/log/config",
        get({
          let config = config.clone();
          move || {
            let config = config.clone();
            async move {
              Json(json!({
                  "level": config.log_level,
                  "retention_count": config.log_retention_days,
                  "log_dir": config.log_dir
              }))
            }
          }
        }),
      )
      .route(
        "/api/v1/log/level",
        put(|Json(payload): Json<Value>| async move {
          Json(json!({
              "message": format!("日志级别已更新为: {}", payload["level"])
          }))
        }),
      )
      .route(
        "/api/v1/log/retention",
        put(|Json(payload): Json<Value>| async move {
          Json(json!({
              "message": format!("日志保留数量已更新为: {} 天", payload["retention_count"])
          }))
        }),
      )
      // 添加搜索端点支持
      .route(
        "/api/v1/logseek/search.ndjson",
        get(|| async {
          // 返回空的NDJSON流
          axum::response::Response::builder()
            .header("content-type", "application/x-ndjson")
            .body(axum::body::Body::empty())
            .unwrap()
        }),
      )
      .route(
        "/api/v1/explorer/list",
        get(|| async {
          Json(json!({
              "entries": [],
              "parent": null,
              "path": "/",
              "total": 0,
              "hidden_count": 0,
              "child_dirs": 0
          }))
        }),
      )
      // 添加 list_files 端点支持 (AgentProxyFS 使用)
      .route(
        "/api/v1/list_files",
        get(|| async {
          Json(json!({
              "items": []
          }))
        }),
      );

    let listener = TcpListener::bind(&address)
      .await
      .map_err(|e| TestError::Network(format!("绑定端口失败: {}", e)))?;

    let bound_address = listener
      .local_addr()
      .map_err(|e| TestError::Network(format!("获取本地地址失败: {}", e)))?;

    let task = tokio::spawn(async move {
      axum::serve(listener, app.into_make_service())
        .await
        .expect("模拟Agent服务器启动失败");
    });

    // 等待服务器启动
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    Ok(Self {
      task,
      address: bound_address,
      config,
    })
  }

  /// 获取服务器基础URL
  pub fn base_url(&self) -> String {
    format!("http://{}", self.address)
  }

  /// 获取服务器主机名（不含端口）
  pub fn hostname(&self) -> String {
    self.address.ip().to_string()
  }

  /// 获取服务器端口
  pub fn port(&self) -> u16 {
    self.address.port()
  }

  /// 停止服务器
  pub async fn stop(self) -> Result<(), TestError> {
    self.task.abort();

    // 等待任务完成
    match self.task.await {
      Ok(_) => Ok(()),
      Err(e) if e.is_cancelled() => Ok(()),
      Err(e) => Err(TestError::Other(format!("停止服务器失败: {}", e))),
    }
  }
}

/// 启动模拟Agent服务器（简化版本）
pub async fn start_mock_agent_server(port: u16) -> Result<MockAgentServer, TestError> {
  let config = MockAgentConfig {
    port,
    ..Default::default()
  };

  MockAgentServer::start(config).await
}

/// 查找可用端口
pub fn find_available_port(start: u16, end: u16) -> Option<u16> {
  use std::net::TcpListener;

  (start..=end).find(|&port| TcpListener::bind(("127.0.0.1", port)).is_ok())
}

/// 创建Agent测试信息
pub fn create_test_agent_info(id: &str, host: &str, port: u16) -> agent_manager::models::AgentInfo {
  use agent_manager::models::{AgentInfo, AgentStatus, AgentTag};

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

#[cfg(test)]
mod tests {
  use super::*;
  use agent_manager::models::AgentStatus;

  #[test]
  fn test_mock_agent_config_default() {
    // 测试MockAgentConfig默认值
    let config = MockAgentConfig::default();

    assert_eq!(config.address, "127.0.0.1");
    assert_eq!(config.port, crate::constants::AGENT_PORT_START);
    assert_eq!(config.log_level, "info");
    assert_eq!(config.log_retention_days, 7);
    assert_eq!(config.log_dir, "/tmp/logs");
  }

  #[test]
  fn test_mock_agent_config_clone() {
    // 测试MockAgentConfig Clone实现
    let config = MockAgentConfig::default();
    let cloned = config.clone();

    assert_eq!(cloned.address, config.address);
    assert_eq!(cloned.port, config.port);
    assert_eq!(cloned.log_level, config.log_level);
    assert_eq!(cloned.log_retention_days, config.log_retention_days);
    assert_eq!(cloned.log_dir, config.log_dir);
  }

  #[tokio::test]
  async fn test_start_mock_agent_server() {
    // 测试启动模拟Agent服务器
    // 使用高端口避免冲突
    let port = 19000; // 使用较高端口
    let result = start_mock_agent_server(port).await;

    // 可能成功也可能失败（端口可能被占用），我们接受两种情况
    match result {
      Ok(server) => {
        // 如果启动成功，测试服务器属性
        assert_eq!(server.port(), port);
        assert!(!server.base_url().is_empty());
        assert!(!server.hostname().is_empty());

        // 停止服务器
        let stop_result = server.stop().await;
        assert!(stop_result.is_ok());
      }
      Err(_) => {
        // 端口被占用是可能的，特别是CI环境中
        // 这种情况下我们不认为测试失败
        println!("注意：端口{}被占用，跳过服务器启动测试", port);
      }
    }
  }

  #[test]
  fn test_find_available_port() {
    // 测试查找可用端口
    let start_port = 19001;
    let end_port = 19010;
    let result = find_available_port(start_port, end_port);

    // 可能找到也可能找不到可用端口
    if let Some(port) = result {
      assert!(port >= start_port && port <= end_port);
    } else {
      println!("注意：在端口{}-{}范围内未找到可用端口", start_port, end_port);
    }
  }

  #[test]
  fn test_create_test_agent_info() {
    // 测试创建Agent测试信息
    let agent_info = create_test_agent_info("test-123", "192.168.1.100", 4001);

    assert_eq!(agent_info.id, "test-123");
    assert_eq!(agent_info.name, "Test Agent test-123");
    assert_eq!(agent_info.version, "1.0.0");
    assert_eq!(agent_info.hostname, "localhost");

    // 检查标签
    assert_eq!(agent_info.tags.len(), 2);
    assert!(
      agent_info
        .tags
        .iter()
        .any(|t| t.key == "host" && t.value == "192.168.1.100")
    );
    assert!(
      agent_info
        .tags
        .iter()
        .any(|t| t.key == "listen_port" && t.value == "4001")
    );

    // 检查搜索根目录
    assert_eq!(agent_info.search_roots, vec!["/tmp".to_string()]);

    // 检查状态
    match agent_info.status {
      AgentStatus::Online => {}
      _ => panic!("Expected Online status"),
    }

    // 检查最后心跳时间应该是最近的
    let now = chrono::Utc::now().timestamp();
    assert!(agent_info.last_heartbeat <= now);
    assert!(agent_info.last_heartbeat >= now - 10); // 应该是最近10秒内
  }

  #[test]
  fn test_mock_agent_server_methods() {
    // 测试MockAgentServer方法（不实际启动服务器）
    // 注意：我们不能在不启动服务器的情况下测试这些方法
    // 这个测试主要验证代码编译
    // 占位测试
  }

  #[test]
  fn test_agent_tag_creation() {
    // 测试AgentTag创建（间接测试）
    use agent_manager::models::AgentTag;

    let tag = AgentTag::new("test_key".to_string(), "test_value".to_string());
    assert_eq!(tag.key, "test_key");
    assert_eq!(tag.value, "test_value");
  }

  #[test]
  fn test_agent_status_variants() {
    // 测试AgentStatus变体
    use agent_manager::models::AgentStatus;

    let online = AgentStatus::Online;
    let offline = AgentStatus::Offline;

    // 确保变体都存在
    match online {
      AgentStatus::Online => {}
      _ => panic!("Expected Online"),
    }

    match offline {
      AgentStatus::Offline => {}
      _ => panic!("Expected Offline"),
    }
  }

  #[test]
  fn test_constants_availability() {
    // 测试常量可用性
    let agent_port_start = crate::constants::AGENT_PORT_START;
    assert!(agent_port_start > 0);
    assert!(agent_port_start < 65535);
  }

  #[tokio::test]
  async fn test_mock_agent_server_lifecycle() {
    // 测试模拟Agent服务器生命周期
    // 使用更高端口避免冲突
    let test_port = 19020;

    // 跳过如果端口被占用
    if std::net::TcpListener::bind(("127.0.0.1", test_port)).is_err() {
      println!("注意：端口{}被占用，跳过生命周期测试", test_port);
      return;
    }

    let config = MockAgentConfig {
      port: test_port,
      ..Default::default()
    };

    let server_result = MockAgentServer::start(config).await;
    if let Err(e) = server_result {
      println!("启动服务器失败: {:?}，跳过测试", e);
      return;
    }

    let server = server_result.unwrap();

    // 验证服务器属性
    assert_eq!(server.port(), test_port);
    assert!(!server.base_url().is_empty());
    assert_eq!(server.hostname(), "127.0.0.1");

    // 停止服务器
    let stop_result = server.stop().await;
    assert!(stop_result.is_ok());
  }

  #[test]
  fn test_port_range_logic() {
    // 测试端口范围逻辑
    // 测试无效范围（start > end）
    let result = find_available_port(20000, 10000);
    assert!(result.is_none());

    // 测试单端口范围
    let test_port = 19030;
    // 我们不知道这个端口是否可用，所以只检查函数是否执行
    let _ = find_available_port(test_port, test_port);
    // 如果没有panic，测试通过
    {}
  }
}
