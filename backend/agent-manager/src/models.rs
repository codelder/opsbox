//! Agent 数据模型

use serde::{Deserialize, Serialize};

/// Agent 标签（key=value 形式）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct AgentTag {
  /// 标签键
  pub key: String,
  /// 标签值
  pub value: String,
}

impl AgentTag {
  /// 创建新标签
  pub fn new(key: String, value: String) -> Self {
    Self { key, value }
  }

  /// 从字符串解析标签（key=value 格式）
  pub fn from_string(s: &str) -> Option<Self> {
    if let Some((key, value)) = s.split_once('=') {
      Some(Self {
        key: key.trim().to_string(),
        value: value.trim().to_string(),
      })
    } else {
      None
    }
  }
}

impl std::fmt::Display for AgentTag {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}={}", self.key, self.value)
  }
}

/// Agent 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
  /// Agent 唯一标识
  pub id: String,

  /// Agent 显示名称
  pub name: String,

  /// Agent 版本
  pub version: String,

  /// 主机名
  pub hostname: String,

  /// 标签（key=value 形式）
  pub tags: Vec<AgentTag>,

  /// 可搜索的根目录
  pub search_roots: Vec<String>,

  /// 最后心跳时间戳（Unix timestamp）
  pub last_heartbeat: i64,

  /// Agent 状态
  pub status: AgentStatus,
}

/// Agent 状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum AgentStatus {
  /// 在线
  Online,

  /// 忙碌（正在执行任务）
  Busy { tasks: usize },

  /// 离线
  Offline,
}

impl std::fmt::Display for AgentStatus {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      AgentStatus::Online => write!(f, "Online"),
      AgentStatus::Busy { .. } => write!(f, "Busy"),
      AgentStatus::Offline => write!(f, "Offline"),
    }
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

impl AgentInfo {
  /// 检查 Agent 是否在线（根据最后心跳时间）
  pub fn is_online(&self, timeout_secs: i64) -> bool {
    let now = chrono::Utc::now().timestamp();
    now - self.last_heartbeat < timeout_secs
  }

  /// 更新心跳时间
  pub fn update_heartbeat(&mut self) {
    self.last_heartbeat = chrono::Utc::now().timestamp();
  }

  /// 检查是否包含指定标签
  pub fn has_tag(&self, key: &str, value: &str) -> bool {
    self.tags.iter().any(|tag| tag.key == key && tag.value == value)
  }

  /// 检查是否包含指定键的标签（不关心值）
  pub fn has_tag_key(&self, key: &str) -> bool {
    self.tags.iter().any(|tag| tag.key == key)
  }

  /// 获取指定键的标签值
  pub fn get_tag_value(&self, key: &str) -> Option<&str> {
    self
      .tags
      .iter()
      .find(|tag| tag.key == key)
      .map(|tag| tag.value.as_str())
  }

  /// 添加标签
  pub fn add_tag(&mut self, key: String, value: String) {
    let tag = AgentTag::new(key, value);
    if !self.tags.contains(&tag) {
      self.tags.push(tag);
    }
  }

  /// 移除标签
  pub fn remove_tag(&mut self, key: &str, value: &str) {
    self.tags.retain(|tag| !(tag.key == key && tag.value == value));
  }

  /// 移除指定键的所有标签
  pub fn remove_tag_key(&mut self, key: &str) {
    self.tags.retain(|tag| tag.key != key);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_agent_info_is_online() {
    let mut agent = AgentInfo {
      id: "test".to_string(),
      name: "Test Agent".to_string(),
      version: "1.0.0".to_string(),
      hostname: "localhost".to_string(),
      tags: vec![],
      search_roots: vec![],
      last_heartbeat: chrono::Utc::now().timestamp(),
      status: AgentStatus::Online,
    };

    // 刚更新心跳，应该在线
    assert!(agent.is_online(60));

    // 设置为1小时前，应该离线
    agent.last_heartbeat = chrono::Utc::now().timestamp() - 3600;
    assert!(!agent.is_online(60));
  }

  #[test]
  fn test_agent_tag_parsing() {
    // 测试正常解析
    let tag = AgentTag::from_string("env=production").unwrap();
    assert_eq!(tag.key, "env");
    assert_eq!(tag.value, "production");

    // 测试带空格的解析
    let tag = AgentTag::from_string(" service = web ").unwrap();
    assert_eq!(tag.key, "service");
    assert_eq!(tag.value, "web");

    // 测试无效格式
    assert!(AgentTag::from_string("invalid").is_none());
    assert!(AgentTag::from_string("").is_none());
  }

  #[test]
  fn test_agent_info_tags() {
    let mut agent = AgentInfo {
      id: "test".to_string(),
      name: "Test Agent".to_string(),
      version: "1.0.0".to_string(),
      hostname: "localhost".to_string(),
      tags: vec![
        AgentTag::new("env".to_string(), "production".to_string()),
        AgentTag::new("service".to_string(), "web".to_string()),
      ],
      search_roots: vec![],
      last_heartbeat: 0,
      status: AgentStatus::Online,
    };

    // 测试标签检查
    assert!(agent.has_tag("env", "production"));
    assert!(agent.has_tag("service", "web"));
    assert!(!agent.has_tag("env", "development"));

    // 测试键检查
    assert!(agent.has_tag_key("env"));
    assert!(agent.has_tag_key("service"));
    assert!(!agent.has_tag_key("region"));

    // 测试获取标签值
    assert_eq!(agent.get_tag_value("env"), Some("production"));
    assert_eq!(agent.get_tag_value("service"), Some("web"));
    assert_eq!(agent.get_tag_value("region"), None);

    // 测试添加标签
    agent.add_tag("region".to_string(), "us-west".to_string());
    assert!(agent.has_tag("region", "us-west"));

    // 测试移除标签
    agent.remove_tag("service", "web");
    assert!(!agent.has_tag("service", "web"));
    assert!(agent.has_tag_key("env")); // env 标签还在
  }
}
