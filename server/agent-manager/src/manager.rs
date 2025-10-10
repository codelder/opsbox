//! Agent 管理器
//!
//! 负责 Agent 的注册、注销、查询和状态管理

use crate::models::{AgentInfo, AgentStatus};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Agent 管理器
pub struct AgentManager {
  /// 已注册的 Agent（内存存储）
  agents: Arc<RwLock<HashMap<String, AgentInfo>>>,

  /// 心跳超时时间（秒）
  heartbeat_timeout: i64,
}

impl AgentManager {
  /// 创建新的 Agent 管理器
  pub fn new() -> Self {
    Self {
      agents: Arc::new(RwLock::new(HashMap::new())),
      heartbeat_timeout: 90, // 90秒未心跳则视为离线
    }
  }

  /// 注册 Agent
  pub async fn register_agent(&self, mut info: AgentInfo) -> Result<(), String> {
    // 更新心跳时间
    info.update_heartbeat();

    log::info!(
      "注册 Agent: id={}, name={}, hostname={}",
      info.id,
      info.name,
      info.hostname
    );

    self.agents.write().await.insert(info.id.clone(), info);

    Ok(())
  }

  /// 注销 Agent
  pub async fn unregister_agent(&self, agent_id: &str) -> Result<(), String> {
    self.agents.write().await.remove(agent_id);

    log::info!("注销 Agent: id={}", agent_id);

    Ok(())
  }

  /// 更新 Agent 心跳
  pub async fn heartbeat(&self, agent_id: &str) -> Result<(), String> {
    let mut agents = self.agents.write().await;

    if let Some(agent) = agents.get_mut(agent_id) {
      agent.update_heartbeat();
      agent.status = AgentStatus::Online;

      log::debug!("Agent 心跳: id={}", agent_id);

      Ok(())
    } else {
      Err(format!("Agent {} 不存在", agent_id))
    }
  }

  /// 获取指定 Agent
  pub async fn get_agent(&self, agent_id: &str) -> Option<AgentInfo> {
    self.agents.read().await.get(agent_id).cloned()
  }

  /// 获取所有 Agent
  pub async fn list_agents(&self) -> Vec<AgentInfo> {
    self.agents.read().await.values().cloned().collect()
  }

  /// 获取所有在线 Agent
  pub async fn list_online_agents(&self) -> Vec<AgentInfo> {
    let agents = self.agents.read().await;

    agents
      .values()
      .filter(|agent| agent.is_online(self.heartbeat_timeout))
      .cloned()
      .collect()
  }

  /// 获取 Agent 数量
  pub async fn count(&self) -> usize {
    self.agents.read().await.len()
  }

  /// 清理离线 Agent
  pub async fn cleanup_offline_agents(&self) -> usize {
    let mut agents = self.agents.write().await;
    let timeout = self.heartbeat_timeout;

    let before_count = agents.len();

    agents.retain(|id, agent| {
      let is_online = agent.is_online(timeout);
      if !is_online {
        log::info!("清理离线 Agent: id={}", id);
      }
      is_online
    });

    before_count - agents.len()
  }
}

impl Default for AgentManager {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_register_and_get_agent() {
    let manager = AgentManager::new();

    let info = AgentInfo {
      id: "test-1".to_string(),
      name: "Test Agent".to_string(),
      version: "1.0.0".to_string(),
      hostname: "localhost".to_string(),
      tags: vec!["test".to_string()],
      search_roots: vec!["/var/log".to_string()],
      last_heartbeat: 0,
      status: AgentStatus::Online,
    };

    manager.register_agent(info.clone()).await.unwrap();

    let retrieved = manager.get_agent("test-1").await.unwrap();
    assert_eq!(retrieved.id, "test-1");
    assert_eq!(retrieved.name, "Test Agent");
  }

  #[tokio::test]
  async fn test_heartbeat() {
    let manager = AgentManager::new();

    let info = AgentInfo {
      id: "test-1".to_string(),
      name: "Test Agent".to_string(),
      version: "1.0.0".to_string(),
      hostname: "localhost".to_string(),
      tags: vec![],
      search_roots: vec![],
      last_heartbeat: 0,
      status: AgentStatus::Online,
    };

    manager.register_agent(info).await.unwrap();

    // 心跳
    manager.heartbeat("test-1").await.unwrap();

    let agent = manager.get_agent("test-1").await.unwrap();
    assert!(agent.last_heartbeat > 0);
  }

  #[tokio::test]
  async fn test_list_agents() {
    let manager = AgentManager::new();

    for i in 1..=3 {
      let info = AgentInfo {
        id: format!("agent-{}", i),
        name: format!("Agent {}", i),
        version: "1.0.0".to_string(),
        hostname: "localhost".to_string(),
        tags: vec![],
        search_roots: vec![],
        last_heartbeat: chrono::Utc::now().timestamp(),
        status: AgentStatus::Online,
      };

      manager.register_agent(info).await.unwrap();
    }

    let agents = manager.list_agents().await;
    assert_eq!(agents.len(), 3);
  }
}
