//! Agent 管理器
//!
//! 负责 Agent 的注册、注销、查询和状态管理

use crate::models::{AgentInfo, AgentStatus, AgentTag};
use crate::repository::AgentRepository;
use sqlx::sqlite::SqlitePool;
use std::sync::Arc;
use tracing;

/// Agent 管理器
pub struct AgentManager {
  /// 数据库操作层
  repository: Arc<AgentRepository>,

  /// 心跳超时时间（秒）
  heartbeat_timeout: i64,

  /// 缓存的重用 HTTP 客户端（避免重复构建）
  http_client: reqwest::Client,
}

impl AgentManager {
  /// 创建新的 Agent 管理器（使用外部传入的连接池）
  pub async fn new(pool: SqlitePool) -> Result<Self, String> {
    let repository = AgentRepository::new(pool);

    // 初始化数据库表结构
    repository
      .init_schema()
      .await
      .map_err(|e| format!("数据库表初始化失败: {}", e))?;

    // 构建可重用的 HTTP 客户端（禁用代理，避免本地请求被劫持）
    let http_client = reqwest::Client::builder()
      .no_proxy()
      .build()
      .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    Ok(Self {
      repository: Arc::new(repository),
      heartbeat_timeout: 90, // 90秒未心跳则视为离线
      http_client,
    })
  }

  /// 获取可重用的 HTTP 客户端
  pub fn http_client(&self) -> &reqwest::Client {
    &self.http_client
  }

  /// 注册 Agent
  pub async fn register_agent(&self, mut info: AgentInfo) -> Result<(), String> {
    // 更新心跳时间
    info.update_heartbeat();

    tracing::info!(
      "注册 Agent: id={}, name={}, hostname={}, tags={:?}, last_heartbeat={}",
      info.id,
      info.name,
      info.hostname,
      info.tags.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
      info.last_heartbeat
    );

    self
      .repository
      .register_agent(&info)
      .await
      .map_err(|e| format!("注册 Agent 失败: {}", e))?;

    Ok(())
  }

  /// 设置 Agent 标签（用户手动设置）
  pub async fn set_agent_tags(&self, agent_id: &str, tags: Vec<AgentTag>) -> Result<(), String> {
    // 检查 Agent 是否存在
    if self
      .repository
      .get_agent(agent_id)
      .await
      .map_err(|e| format!("查询 Agent 失败: {}", e))?
      .is_none()
    {
      return Err(format!("Agent {} 不存在", agent_id));
    }

    self
      .repository
      .save_agent_tags(agent_id, &tags)
      .await
      .map_err(|e| format!("设置标签失败: {}", e))?;

    tracing::info!("设置 Agent {} 的标签: {:?}", agent_id, tags);
    Ok(())
  }

  /// 添加 Agent 标签
  pub async fn add_agent_tag(&self, agent_id: &str, tag: AgentTag) -> Result<(), String> {
    // 获取现有标签
    let mut tags = self
      .repository
      .get_agent_tags(agent_id)
      .await
      .map_err(|e| format!("获取标签失败: {}", e))?;

    // 检查是否已存在
    if !tags.contains(&tag) {
      tags.push(tag.clone());
      self
        .repository
        .save_agent_tags(agent_id, &tags)
        .await
        .map_err(|e| format!("添加标签失败: {}", e))?;
      tracing::info!("为 Agent {} 添加标签: {}", agent_id, tag);
    }
    Ok(())
  }

  /// 移除 Agent 标签
  pub async fn remove_agent_tag(&self, agent_id: &str, key: &str, value: &str) -> Result<(), String> {
    self
      .repository
      .delete_agent_tag(agent_id, key, value)
      .await
      .map_err(|e| format!("移除标签失败: {}", e))?;

    tracing::info!("从 Agent {} 移除标签: {}={}", agent_id, key, value);
    Ok(())
  }

  /// 移除 Agent 的所有标签
  pub async fn clear_agent_tags(&self, agent_id: &str) -> Result<(), String> {
    self
      .repository
      .save_agent_tags(agent_id, &[])
      .await
      .map_err(|e| format!("清空标签失败: {}", e))?;

    tracing::info!("清空 Agent {} 的所有标签", agent_id);
    Ok(())
  }

  /// 注销 Agent
  pub async fn unregister_agent(&self, agent_id: &str) -> Result<(), String> {
    self
      .repository
      .unregister_agent(agent_id)
      .await
      .map_err(|e| format!("注销 Agent 失败: {}", e))?;

    tracing::info!("注销 Agent: {}", agent_id);
    Ok(())
  }

  /// 更新 Agent 心跳
  pub async fn heartbeat(&self, agent_id: &str) -> Result<(), String> {
    self
      .repository
      .update_heartbeat(agent_id)
      .await
      .map_err(|e| format!("更新心跳失败: {}", e))?;

    tracing::debug!("Agent {} 心跳更新", agent_id);
    Ok(())
  }

  /// 内部：根据最后心跳动态计算并修正状态
  fn apply_dynamic_status(&self, mut agent: AgentInfo) -> AgentInfo {
    // 超时则一律视为离线（不记录详细日志）
    if !agent.check_online_status(self.heartbeat_timeout, false) {
      agent.status = AgentStatus::Offline;
      return agent;
    }
    // 在线窗口内：保留原有状态（Online 或 Busy）
    if let AgentStatus::Offline = agent.status {
      agent.status = AgentStatus::Online;
    }
    agent
  }

  /// 获取 Agent 信息（动态状态）
  pub async fn get_agent(&self, agent_id: &str) -> Option<AgentInfo> {
    self
      .repository
      .get_agent(agent_id)
      .await
      .map_err(|e| tracing::error!("获取 Agent 失败: {}", e))
      .ok()
      .flatten()
      .map(|a| self.apply_dynamic_status(a))
  }

  /// 获取所有 Agent（动态状态）
  pub async fn list_agents(&self) -> Vec<AgentInfo> {
    let list = self
      .repository
      .list_agents()
      .await
      .map_err(|e| tracing::error!("获取 Agent 列表失败: {}", e))
      .unwrap_or_default();
    list.into_iter().map(|a| self.apply_dynamic_status(a)).collect()
  }

  /// 获取在线 Agent（动态过滤）
  pub async fn list_online_agents(&self) -> Vec<AgentInfo> {
    let list = self.list_agents().await;
    tracing::info!(
      "AgentManager::list_online_agents: total {} agents, heartbeat_timeout={}",
      list.len(),
      self.heartbeat_timeout
    );

    let online_agents: Vec<AgentInfo> = list
      .into_iter()
      .filter(|a| a.check_online_status(self.heartbeat_timeout, true))
      .collect();

    tracing::info!(
      "AgentManager::list_online_agents: returning {} online agents",
      online_agents.len()
    );
    for agent in &online_agents {
      tracing::info!("  Online agent: id={}, name={}", agent.id, agent.name);
    }

    online_agents
  }

  /// 按标签筛选 Agent（动态状态）
  pub async fn list_agents_by_tags(&self, tag_filters: &[AgentTag]) -> Vec<AgentInfo> {
    let list = self
      .repository
      .list_agents_by_tags(tag_filters)
      .await
      .map_err(|e| tracing::error!("按标签筛选 Agent 失败: {}", e))
      .unwrap_or_default();
    list.into_iter().map(|a| self.apply_dynamic_status(a)).collect()
  }

  /// 按标签筛选在线 Agent（动态过滤）
  pub async fn list_online_agents_by_tags(&self, tag_filters: &[AgentTag]) -> Vec<AgentInfo> {
    let list = self.list_agents_by_tags(tag_filters).await;
    list
      .into_iter()
      .filter(|a| a.is_online(self.heartbeat_timeout))
      .collect()
  }

  /// 获取所有可用的标签键
  pub async fn get_all_tag_keys(&self) -> Vec<String> {
    self
      .repository
      .get_all_tag_keys()
      .await
      .map_err(|e| tracing::error!("获取标签键失败: {}", e))
      .unwrap_or_default()
  }

  /// 获取指定键的所有标签值
  pub async fn get_tag_values_by_key(&self, key: &str) -> Vec<String> {
    self
      .repository
      .get_tag_values_by_key(key)
      .await
      .map_err(|e| tracing::error!("获取标签值失败: {}", e))
      .unwrap_or_default()
  }

  /// 获取所有可用的标签
  pub async fn get_all_tags(&self) -> Vec<AgentTag> {
    self
      .repository
      .get_all_tags()
      .await
      .map_err(|e| tracing::error!("获取所有标签失败: {}", e))
      .unwrap_or_default()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::models::AgentStatus;

  async fn create_test_manager() -> AgentManager {
    let pool = sqlx::sqlite::SqlitePool::connect("sqlite::memory:").await.unwrap();

    AgentManager::new(pool).await.unwrap()
  }

  #[tokio::test]
  async fn test_register_and_get_agent() {
    let manager = create_test_manager().await;

    let info = AgentInfo {
      id: "test-1".to_string(),
      name: "Test Agent".to_string(),
      version: "1.0.0".to_string(),
      hostname: "localhost".to_string(),
      tags: vec![AgentTag::new("test".to_string(), "true".to_string())],
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
    let manager = create_test_manager().await;

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
    let manager = create_test_manager().await;

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

  #[tokio::test]
  async fn test_list_agents_by_tags() {
    let manager = create_test_manager().await;

    // 注册带有不同标签的 Agent
    let agents_data = vec![
      ("agent-1", vec![("env", "production"), ("service", "web")]),
      ("agent-2", vec![("env", "production"), ("service", "api")]),
      ("agent-3", vec![("env", "development"), ("service", "web")]),
      ("agent-4", vec![("env", "development"), ("service", "api")]),
    ];

    for (id, tags) in agents_data {
      let mut info = AgentInfo {
        id: id.to_string(),
        name: format!("Agent {}", id),
        version: "1.0.0".to_string(),
        hostname: "localhost".to_string(),
        tags: vec![],
        search_roots: vec![],
        last_heartbeat: chrono::Utc::now().timestamp(),
        status: AgentStatus::Online,
      };

      for (key, value) in tags {
        info.add_tag(key.to_string(), value.to_string());
      }

      manager.register_agent(info).await.unwrap();
    }

    // 测试按单个标签筛选
    let production_agents = manager
      .list_agents_by_tags(&[AgentTag::new("env".to_string(), "production".to_string())])
      .await;
    assert_eq!(production_agents.len(), 2);
    assert!(production_agents.iter().all(|a| a.has_tag("env", "production")));

    // 测试按多个标签筛选
    let web_agents = manager
      .list_agents_by_tags(&[AgentTag::new("service".to_string(), "web".to_string())])
      .await;
    assert_eq!(web_agents.len(), 2);
    assert!(web_agents.iter().all(|a| a.has_tag("service", "web")));

    // 测试按复合标签筛选
    let production_web_agents = manager
      .list_agents_by_tags(&[
        AgentTag::new("env".to_string(), "production".to_string()),
        AgentTag::new("service".to_string(), "web".to_string()),
      ])
      .await;
    assert_eq!(production_web_agents.len(), 1);
    assert!(production_web_agents[0].has_tag("env", "production"));
    assert!(production_web_agents[0].has_tag("service", "web"));

    // 测试按不存在的标签筛选
    let non_existent = manager
      .list_agents_by_tags(&[AgentTag::new("region".to_string(), "us-west".to_string())])
      .await;
    assert_eq!(non_existent.len(), 0);

    // 测试空标签列表（应该返回所有 Agent）
    let all_agents = manager.list_agents_by_tags(&[]).await;
    assert_eq!(all_agents.len(), 4);
  }

  #[tokio::test]
  async fn test_get_all_tags() {
    let manager = create_test_manager().await;

    // 注册带有不同标签的 Agent
    let agents_data = vec![
      ("agent-1", vec![("env", "production"), ("service", "web")]),
      ("agent-2", vec![("env", "production"), ("service", "api")]),
      ("agent-3", vec![("env", "development"), ("service", "web")]),
    ];

    for (id, tags) in agents_data {
      let mut info = AgentInfo {
        id: id.to_string(),
        name: format!("Agent {}", id),
        version: "1.0.0".to_string(),
        hostname: "localhost".to_string(),
        tags: vec![],
        search_roots: vec![],
        last_heartbeat: chrono::Utc::now().timestamp(),
        status: AgentStatus::Online,
      };

      for (key, value) in tags {
        info.add_tag(key.to_string(), value.to_string());
      }

      manager.register_agent(info).await.unwrap();
    }

    let all_tags = manager.get_all_tags().await;
    assert_eq!(all_tags.len(), 4); // env=production, env=development, service=web, service=api

    let tag_strings: Vec<String> = all_tags.iter().map(|t| t.to_string()).collect();
    assert!(tag_strings.contains(&"env=production".to_string()));
    assert!(tag_strings.contains(&"env=development".to_string()));
    assert!(tag_strings.contains(&"service=web".to_string()));
    assert!(tag_strings.contains(&"service=api".to_string()));

    // 测试获取标签键
    let tag_keys = manager.get_all_tag_keys().await;
    assert_eq!(tag_keys.len(), 2);
    assert!(tag_keys.contains(&"env".to_string()));
    assert!(tag_keys.contains(&"service".to_string()));

    // 测试获取指定键的标签值
    let env_values = manager.get_tag_values_by_key("env").await;
    assert_eq!(env_values.len(), 2);
    assert!(env_values.contains(&"production".to_string()));
    assert!(env_values.contains(&"development".to_string()));
  }
}
