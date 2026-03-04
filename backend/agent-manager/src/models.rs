//! Agent 数据模型
//!
//! 本模块重新导出 opsbox-core 中的 Agent 相关类型，
//! 并定义 agent-manager 特有的类型。

use serde::{Deserialize, Serialize};

// 从 opsbox-core 重新导出共享的 Agent 类型
pub use opsbox_core::agent::models::{AgentInfo, AgentStatus, AgentTag};

/// Agent 列表响应
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentListResponse {
  pub agents: Vec<AgentInfo>,
  pub total: usize,
}

/// Agent 注册请求（扩展：支持上报监听端口）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegisterRequest {
  #[serde(flatten)]
  pub info: AgentInfo,
  /// Agent 本地监听端口（用于服务端结合远端地址推断访问端点）
  pub listen_port: Option<u16>,
}

/// Agent 心跳响应
#[derive(Debug, Serialize, Deserialize)]
pub struct HeartbeatResponse {
  pub success: bool,
  pub message: String,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_agent_get_base_url() {
    // Default case
    let agent = AgentInfo {
      id: "test".into(),
      name: "Test".into(),
      version: "1.0".into(),
      hostname: "localhost".into(),
      tags: vec![],
      search_roots: vec![],
      last_heartbeat: 0,
      status: AgentStatus::Online,
    };
    assert_eq!(agent.get_base_url(), "http://localhost:3976");

    // Custom host via tag
    let mut agent_host = agent.clone();
    agent_host.add_tag("host".into(), "192.168.1.100".into());
    assert_eq!(agent_host.get_base_url(), "http://192.168.1.100:3976");

    // Custom port via tag
    let mut agent_port = agent.clone();
    agent_port.add_tag("listen_port".into(), "8080".into());
    assert_eq!(agent_port.get_base_url(), "http://localhost:8080");

    // Custom host and port
    let mut agent_full = agent.clone();
    agent_full.add_tag("host".into(), "10.0.0.1".into());
    agent_full.add_tag("listen_port".into(), "9090".into());
    assert_eq!(agent_full.get_base_url(), "http://10.0.0.1:9090");
  }
}
