//! Agent 数据模型
//!
//! 本模块重新导出 opsbox-core 中的 Agent 相关类型，
//! 并定义 agent-manager 特有的类型。
//!
//! 同时提供与 opsbox-domain Agent 类型之间的转换。

use serde::{Deserialize, Serialize};

// 从 opsbox-core 重新导出共享的 Agent 类型（保持向后兼容）
pub use opsbox_core::agent::models::{AgentInfo, AgentStatus, AgentTag};

// 从 opsbox-domain 导入 DDD Agent 类型（使用 pub use 重新导出）
pub use opsbox_domain::agent::{
    Agent as DomainAgent,
    AgentId,
    AgentConnection,
    AgentCapabilities,
    AgentTag as DomainAgentTag,
    AgentStatus as DomainAgentStatus,
};

/// Agent 类型转换工具
pub struct AgentConverter;

impl AgentConverter {
    /// 将 AgentInfo 转换为 DomainAgent
    pub fn to_domain_agent(info: &AgentInfo) -> DomainAgent {
        use opsbox_domain::agent::{Agent, AgentConnection};

        let listen_port = info
            .get_tag_value("listen_port")
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(3976);

        let connection = AgentConnection::new(info.get_base_url(), listen_port, 90);

        let mut agent = Agent::new(
            AgentId::new(info.id.clone()),
            info.name.clone(),
            info.version.clone(),
            info.hostname.clone(),
            connection,
        );

        // 转换标签
        for tag in &info.tags {
            agent.add_tag_kv(tag.key.clone(), tag.value.clone());
        }

        // 添加搜索根目录
        for root in &info.search_roots {
            agent.add_search_root(root.clone());
        }

        // 设置状态
        agent.set_status(match info.status {
            AgentStatus::Online => DomainAgentStatus::Online,
            AgentStatus::Offline => DomainAgentStatus::Offline,
            AgentStatus::Busy { .. } => DomainAgentStatus::Busy { active_tasks: 1 },
        });

        agent
    }

    /// 将 DomainAgent 转换为 AgentInfo
    pub fn to_agent_info(agent: &DomainAgent) -> AgentInfo {
        let tags = agent
            .tags()
            .iter()
            .map(|tag| AgentTag {
                key: tag.key.clone(),
                value: tag.value.clone(),
            })
            .collect();

        let status = match agent.status() {
            DomainAgentStatus::Online => AgentStatus::Online,
            DomainAgentStatus::Offline => AgentStatus::Offline,
            DomainAgentStatus::Busy { active_tasks } => AgentStatus::Busy {
                tasks: active_tasks,
            },
        };

        AgentInfo {
            id: agent.id.as_str().to_string(),
            name: agent.name.clone(),
            version: agent.version.clone(),
            hostname: agent.hostname.clone(),
            tags,
            search_roots: agent.search_roots().to_vec(),
            last_heartbeat: agent.connection().last_heartbeat,
            status,
        }
    }
}

#[cfg(test)]
mod conversion_tests {
    use super::*;

    #[test]
    fn test_agent_info_to_domain_agent() {
        let info = AgentInfo {
            id: "test-agent".to_string(),
            name: "Test Agent".to_string(),
            version: "1.0.0".to_string(),
            hostname: "localhost".to_string(),
            tags: vec![
                AgentTag::new("env".to_string(), "prod".to_string()),
                AgentTag::new("listen_port".to_string(), "8080".to_string()),
            ],
            search_roots: vec!["/var/log".to_string()],
            last_heartbeat: 1234567890,
            status: AgentStatus::Online,
        };

        let domain_agent = AgentConverter::to_domain_agent(&info);

        assert_eq!(domain_agent.id.as_str(), "test-agent");
        assert_eq!(domain_agent.name, "Test Agent");
        assert!(domain_agent.has_tag("env", "prod"));
        assert_eq!(domain_agent.search_roots().len(), 1);
    }

    #[test]
    fn test_domain_agent_to_agent_info() {
        use opsbox_domain::agent::{Agent, AgentConnection};

        let id = AgentId::new("test-agent".to_string());
        let connection = AgentConnection::new("http://localhost".to_string(), 8080, 90);

        let mut domain_agent = Agent::new(
            id.clone(),
            "Test Agent".to_string(),
            "1.0.0".to_string(),
            "localhost".to_string(),
            connection,
        );

        domain_agent.add_tag_kv("env".to_string(), "prod".to_string());
        domain_agent.add_search_root("/var/log".to_string());

        let info = AgentConverter::to_agent_info(&domain_agent);

        assert_eq!(info.id, "test-agent");
        assert_eq!(info.name, "Test Agent");
        assert!(info.has_tag("env", "prod"));
        assert_eq!(info.search_roots.len(), 1);
    }

    #[test]
    fn test_round_trip_conversion() {
        let original = AgentInfo {
            id: "test-agent".to_string(),
            name: "Test Agent".to_string(),
            version: "1.0.0".to_string(),
            hostname: "192.168.1.100".to_string(),
            tags: vec![
                AgentTag::new("host".to_string(), "192.168.1.100".to_string()),
                AgentTag::new("listen_port".to_string(), "9000".to_string()),
            ],
            search_roots: vec!["/var/log".to_string(), "/app/logs".to_string()],
            last_heartbeat: 1234567890,
            status: AgentStatus::Online,
        };

        let domain_agent = AgentConverter::to_domain_agent(&original);
        let converted = AgentConverter::to_agent_info(&domain_agent);

        assert_eq!(converted.id, original.id);
        assert_eq!(converted.name, original.name);
        assert_eq!(converted.tags.len(), original.tags.len());
        assert_eq!(converted.search_roots, original.search_roots);
    }
}

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
