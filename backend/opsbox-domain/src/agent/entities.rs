//! Agent 上下文实体
//!
//! Agent 聚合根和相关实体定义。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::value_objects::{
    AgentTag, AgentConnection, AgentCapabilities, AgentStatus,
};

/// Agent ID（值对象）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(String);

impl AgentId {
    /// 创建新的 Agent ID
    pub fn new(id: String) -> Self {
        Self(id)
    }

    /// 从字符串创建
    pub fn from_string(s: String) -> Self {
        Self(s)
    }

    /// 获取内部字符串
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 生成随机 UUID 作为 ID
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for AgentId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for AgentId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Agent 聚合根
///
/// 封装 Agent 的完整行为和状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Agent 唯一标识
    pub id: AgentId,
    /// Agent 显示名称
    pub name: String,
    /// Agent 版本
    pub version: String,
    /// 主机名
    pub hostname: String,
    /// 标签集合
    tags: Vec<AgentTag>,
    /// 可搜索的根目录
    pub search_roots: Vec<String>,
    /// 连接信息
    connection: AgentConnection,
    /// Agent 能力
    pub capabilities: AgentCapabilities,
    /// 当前状态
    status: AgentStatus,
}

impl Agent {
    /// 创建新的 Agent
    pub fn new(
        id: AgentId,
        name: String,
        version: String,
        hostname: String,
        connection: AgentConnection,
    ) -> Self {
        Self {
            id,
            name,
            version,
            hostname,
            tags: Vec::new(),
            search_roots: Vec::new(),
            connection,
            capabilities: AgentCapabilities::default(),
            status: AgentStatus::Online,
        }
    }

    /// 添加标签
    pub fn add_tag(&mut self, tag: AgentTag) {
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
    }

    /// 添加键值对标签
    pub fn add_tag_kv(&mut self, key: String, value: String) {
        self.add_tag(AgentTag::new(key, value));
    }

    /// 移除标签
    pub fn remove_tag(&mut self, key: &str, value: &str) {
        self.tags.retain(|tag| !(tag.key == key && tag.value == value));
    }

    /// 移除指定键的所有标签
    pub fn remove_tag_key(&mut self, key: &str) {
        self.tags.retain(|tag| tag.key != key);
    }

    /// 清空所有标签
    pub fn clear_tags(&mut self) {
        self.tags.clear();
    }

    /// 获取指定键的标签值
    pub fn get_tag(&self, key: &str) -> Option<&str> {
        self.tags.iter()
            .find(|tag| tag.key == key)
            .map(|tag| tag.value.as_str())
    }

    /// 检查是否包含指定标签
    pub fn has_tag(&self, key: &str, value: &str) -> bool {
        self.tags.iter().any(|tag| tag.key == key && tag.value == value)
    }

    /// 检查是否包含指定键的标签（不关心值）
    pub fn has_tag_key(&self, key: &str) -> bool {
        self.tags.iter().any(|tag| tag.key == key)
    }

    /// 获取所有标签
    pub fn tags(&self) -> &[AgentTag] {
        &self.tags
    }

    /// 添加搜索根目录
    pub fn add_search_root(&mut self, root: String) {
        if !self.search_roots.contains(&root) {
            self.search_roots.push(root);
        }
    }

    /// 获取搜索根目录
    pub fn search_roots(&self) -> &[String] {
        &self.search_roots
    }

    /// 更新连接信息
    pub fn update_connection(&mut self, connection: AgentConnection) {
        self.connection = connection;
    }

    /// 更新心跳
    pub fn update_heartbeat(&mut self) {
        self.connection.update_heartbeat();
        if self.status == AgentStatus::Offline {
            self.status = AgentStatus::Online;
        }
    }

    /// 检查是否在线
    pub fn is_online(&self) -> bool {
        !matches!(self.status, AgentStatus::Offline) && self.connection.is_online()
    }

    /// 获取连接信息
    pub fn connection(&self) -> &AgentConnection {
        &self.connection
    }

    /// 获取 API 地址
    pub fn api_url(&self) -> String {
        self.connection.api_url()
    }

    /// 设置能力
    pub fn set_capabilities(&mut self, capabilities: AgentCapabilities) {
        self.capabilities = capabilities;
    }

    /// 检查是否可以接受任务
    pub fn can_accept_task(&self) -> bool {
        self.connection.is_online() && self.status.is_available()
    }

    /// 设置状态
    pub fn set_status(&mut self, status: AgentStatus) {
        self.status = status;
    }

    /// 获取状态
    pub fn status(&self) -> AgentStatus {
        self.status
    }

    /// 设置为忙碌状态
    pub fn set_busy(&mut self, active_tasks: usize) {
        self.status = AgentStatus::Busy { active_tasks };
    }

    /// 设置为离线状态
    pub fn set_offline(&mut self) {
        self.status = AgentStatus::Offline;
    }

    /// 设置为在线状态
    pub fn set_online(&mut self) {
        self.status = AgentStatus::Online;
    }
}

/// Agent 注册表（领域服务）
///
/// 管理 Agent 的注册、查找和生命周期。
pub struct AgentRegistry {
    /// Agent 存储映射
    agents: HashMap<AgentId, Arc<RwLock<Agent>>>,
    /// 心跳超时时间（秒）
    heartbeat_timeout_secs: i64,
}

impl AgentRegistry {
    /// 创建新的注册表
    pub fn new(heartbeat_timeout_secs: i64) -> Self {
        Self {
            agents: HashMap::new(),
            heartbeat_timeout_secs,
        }
    }

    /// 注册 Agent
    pub fn register(&mut self, agent: Agent) {
        let id = agent.id.clone();
        self.agents.insert(id, Arc::new(RwLock::new(agent)));
    }

    /// 注销 Agent
    pub fn unregister(&mut self, id: &AgentId) -> Option<Agent> {
        self.agents.remove(id)
            .and_then(|arc| Arc::try_unwrap(arc).ok())
            .map(|rw| RwLock::into_inner(rw).ok())
            .flatten()
    }

    /// 获取 Agent
    pub fn get(&self, id: &AgentId) -> Option<Arc<RwLock<Agent>>> {
        self.agents.get(id).cloned()
    }

    /// 检查 Agent 是否存在
    pub fn contains(&self, id: &AgentId) -> bool {
        self.agents.contains_key(id)
    }

    /// 获取所有 Agent ID
    pub fn all_agent_ids(&self) -> Vec<AgentId> {
        self.agents.keys().cloned().collect()
    }

    /// 获取在线 Agent
    pub fn online_agents(&self) -> Vec<AgentId> {
        self.agents.iter()
            .filter(|(_, agent)| {
                agent.read().ok().map(|a| a.is_online()).unwrap_or(false)
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// 获取可用 Agent（在线且不忙）
    pub fn available_agents(&self) -> Vec<AgentId> {
        self.agents.iter()
            .filter(|(_, agent)| {
                agent.read().ok().map(|a| a.can_accept_task()).unwrap_or(false)
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// 根据标签查找 Agent
    pub fn find_by_tag(&self, key: &str, value: &str) -> Vec<AgentId> {
        self.agents.iter()
            .filter(|(_, agent)| {
                agent.read().ok().map(|a| a.has_tag(key, value)).unwrap_or(false)
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// 根据标签键查找 Agent（不关心值）
    pub fn find_by_tag_key(&self, key: &str) -> Vec<AgentId> {
        self.agents.iter()
            .filter(|(_, agent)| {
                agent.read().ok().map(|a| a.has_tag_key(key)).unwrap_or(false)
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// 更新 Agent 心跳
    pub fn update_heartbeat(&self, id: &AgentId) -> bool {
        if let Some(agent) = self.get(id) {
            if let Ok(mut agent) = agent.write() {
                agent.update_heartbeat();
                return true;
            }
        }
        false
    }

    /// 清理离线 Agent（使用注册表级别的超时设置）
    pub fn cleanup_offline(&mut self) -> usize {
        let before = self.agents.len();
        let now = chrono::Utc::now().timestamp();
        self.agents.retain(|_, agent| {
            agent.read().ok().map(|a| {
                // 使用注册表级别的超时检查
                now - a.connection().last_heartbeat < self.heartbeat_timeout_secs
            }).unwrap_or(false)
        });
        before - self.agents.len()
    }

    /// 获取 Agent 统计
    pub fn stats(&self) -> AgentRegistryStats {
        let total = self.agents.len();
        let online = self.online_agents().len();
        let available = self.available_agents().len();
        let offline = total - online;

        AgentRegistryStats {
            total,
            online,
            offline,
            available,
        }
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new(60) // 默认 60 秒超时
    }
}

/// Agent 注册表统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistryStats {
    /// 总数
    pub total: usize,
    /// 在线数
    pub online: usize,
    /// 离线数
    pub offline: usize,
    /// 可用数（在线且不忙）
    pub available: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_id_new() {
        let id = AgentId::new("test-agent".to_string());
        assert_eq!(id.as_str(), "test-agent");
    }

    #[test]
    fn test_agent_id_from_string() {
        let id = AgentId::from("test-agent");
        assert_eq!(id.as_str(), "test-agent");
    }

    #[test]
    fn test_agent_id_generate() {
        let id1 = AgentId::generate();
        let id2 = AgentId::generate();
        assert_ne!(id1, id2);
        assert!(uuid::Uuid::parse_str(id1.as_str()).is_ok());
    }

    #[test]
    fn test_agent_new() {
        let id = AgentId::new("agent1".to_string());
        let connection = AgentConnection::new(
            "http://192.168.1.100".to_string(),
            3976,
            60,
        );

        let agent = Agent::new(
            id.clone(),
            "Web Server 01".to_string(),
            "1.0.0".to_string(),
            "web-01".to_string(),
            connection,
        );

        assert_eq!(agent.id, id);
        assert_eq!(agent.name, "Web Server 01");
        assert!(agent.tags.is_empty());
        assert!(agent.search_roots.is_empty());
        assert!(agent.is_online());
    }

    #[test]
    fn test_agent_tags() {
        let id = AgentId::new("agent1".to_string());
        let connection = AgentConnection::new(
            "http://localhost".to_string(),
            3976,
            60,
        );

        let mut agent = Agent::new(
            id,
            "Test Agent".to_string(),
            "1.0".to_string(),
            "localhost".to_string(),
            connection,
        );

        agent.add_tag_kv("env".to_string(), "prod".to_string());
        agent.add_tag_kv("region".to_string(), "us".to_string());

        assert_eq!(agent.tags().len(), 2);
        assert!(agent.has_tag("env", "prod"));
        assert_eq!(agent.get_tag("env"), Some("prod"));

        agent.remove_tag("env", "prod");
        assert_eq!(agent.tags().len(), 1);
        assert!(!agent.has_tag("env", "prod"));

        agent.clear_tags();
        assert!(agent.tags().is_empty());
    }

    #[test]
    fn test_agent_search_roots() {
        let id = AgentId::new("agent1".to_string());
        let connection = AgentConnection::new(
            "http://localhost".to_string(),
            3976,
            60,
        );

        let mut agent = Agent::new(
            id,
            "Test Agent".to_string(),
            "1.0".to_string(),
            "localhost".to_string(),
            connection,
        );

        agent.add_search_root("/var/log".to_string());
        agent.add_search_root("/app/logs".to_string());
        agent.add_search_root("/var/log".to_string()); // 重复

        assert_eq!(agent.search_roots().len(), 2);
    }

    #[test]
    fn test_agent_status() {
        let id = AgentId::new("agent1".to_string());
        let connection = AgentConnection::new(
            "http://localhost".to_string(),
            3976,
            60,
        );

        let mut agent = Agent::new(
            id,
            "Test Agent".to_string(),
            "1.0".to_string(),
            "localhost".to_string(),
            connection,
        );

        assert!(agent.can_accept_task());

        agent.set_busy(3);
        assert!(!agent.can_accept_task());
        assert_eq!(agent.status().active_tasks(), Some(3));

        agent.set_offline();
        assert!(!agent.is_online());
        assert!(!agent.can_accept_task());

        agent.set_online();
        assert!(agent.is_online());
    }

    #[test]
    fn test_agent_registry() {
        let mut registry = AgentRegistry::new(60);

        let id = AgentId::new("agent1".to_string());
        let connection = AgentConnection::new(
            "http://192.168.1.100".to_string(),
            3976,
            60,
        );

        let agent = Agent::new(
            id.clone(),
            "Test Agent".to_string(),
            "1.0".to_string(),
            "web-01".to_string(),
            connection,
        );

        registry.register(agent);

        assert!(registry.contains(&id));
        assert_eq!(registry.all_agent_ids().len(), 1);
        assert_eq!(registry.online_agents().len(), 1);
        assert_eq!(registry.available_agents().len(), 1);
    }

    #[test]
    fn test_agent_registry_find_by_tag() {
        let mut registry = AgentRegistry::new(60);

        let agent1 = {
            let id = AgentId::new("agent1".to_string());
            let conn = AgentConnection::new("http://localhost".to_string(), 3976, 60);
            let mut agent = Agent::new(id, "Agent 1".to_string(), "1.0".to_string(), "host1".to_string(), conn);
            agent.add_tag_kv("env".to_string(), "prod".to_string());
            agent
        };

        let agent2 = {
            let id = AgentId::new("agent2".to_string());
            let conn = AgentConnection::new("http://localhost".to_string(), 3976, 60);
            let mut agent = Agent::new(id, "Agent 2".to_string(), "1.0".to_string(), "host2".to_string(), conn);
            agent.add_tag_kv("env".to_string(), "dev".to_string());
            agent.add_tag_kv("region".to_string(), "us".to_string());
            agent
        };

        registry.register(agent1);
        registry.register(agent2);

        let prod_agents = registry.find_by_tag("env", "prod");
        assert_eq!(prod_agents.len(), 1);

        let us_agents = registry.find_by_tag("region", "us");
        assert_eq!(us_agents.len(), 1);
    }

    #[test]
    fn test_agent_registry_stats() {
        let mut registry = AgentRegistry::new(60);

        // 添加在线 Agent
        let agent1 = {
            let id = AgentId::new("agent1".to_string());
            let conn = AgentConnection::new("http://localhost".to_string(), 3976, 60);
            Agent::new(id, "Agent 1".to_string(), "1.0".to_string(), "host1".to_string(), conn)
        };

        // 添加忙碌的 Agent
        let agent2 = {
            let id = AgentId::new("agent2".to_string());
            let conn = AgentConnection::new("http://localhost".to_string(), 3976, 60);
            let mut agent = Agent::new(id, "Agent 2".to_string(), "1.0".to_string(), "host2".to_string(), conn);
            agent.set_busy(2);
            agent
        };

        registry.register(agent1);
        registry.register(agent2);

        let stats = registry.stats();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.online, 2);
        assert_eq!(stats.available, 1);
        assert_eq!(stats.offline, 0);
    }

    #[test]
    fn test_agent_api_url() {
        let id = AgentId::new("agent1".to_string());
        let connection = AgentConnection::new(
            "http://192.168.1.100".to_string(),
            8080,
            60,
        );

        let agent = Agent::new(
            id,
            "Test Agent".to_string(),
            "1.0".to_string(),
            "localhost".to_string(),
            connection,
        );

        assert_eq!(agent.api_url(), "http://192.168.1.100:8080");
    }
}
