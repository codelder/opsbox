//! Agent Mock服务器
//!
//! 提供模拟Agent服务器的工具，用于集成测试

use axum::{Router, routing::{get, put}, Json};
use serde_json::{json, Value};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use crate::TestError;

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
            );

        let listener = TcpListener::bind(&address).await
            .map_err(|e| TestError::Network(format!("绑定端口失败: {}", e)))?;

        let bound_address = listener.local_addr()
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

    for port in start..=end {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Some(port);
        }
    }

    None
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