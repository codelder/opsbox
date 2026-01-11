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

/// Agent 文件列表请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentListRequest {
  /// 请求列举的目录路径
  pub path: String,
}

/// Agent 文件条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFileItem {
  /// 文件名
  pub name: String,
  /// 完整路径
  pub path: String,
  /// 是否为目录
  pub is_dir: bool,
  /// 是否为符号链接
  pub is_symlink: bool,
  /// 文件大小
  pub size: Option<u64>,
  /// 修改时间 (Unix timestamp)
  pub modified: Option<i64>,
  /// 子项目数量 (仅对目录有效)
  pub child_count: Option<u32>,
  /// 隐藏子项目数量 (仅对目录有效)
  pub hidden_child_count: Option<u32>,
  /// MIME 类型
  pub mime_type: Option<String>,
}

/// Agent 文件列表响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentListResponse {
  /// 文件列表
  pub items: Vec<AgentFileItem>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_tag_new() {
        let tag = AgentTag::new("env".to_string(), "prod".to_string());
        assert_eq!(tag.key, "env");
        assert_eq!(tag.value, "prod");
    }

    #[test]
    fn test_agent_tag_from_string() {
        let tag = AgentTag::from_string("env=production").unwrap();
        assert_eq!(tag.key, "env");
        assert_eq!(tag.value, "production");

        let tag = AgentTag::from_string(" key = value ").unwrap();
        assert_eq!(tag.key, "key");
        assert_eq!(tag.value, "value");

        assert!(AgentTag::from_string("invalid").is_none());
    }

    #[test]
    fn test_agent_tag_display() {
        let tag = AgentTag::new("env".to_string(), "prod".to_string());
        assert_eq!(tag.to_string(), "env=prod");
    }

    #[test]
    fn test_agent_status_display() {
        assert_eq!(AgentStatus::Online.to_string(), "Online");
        assert_eq!(AgentStatus::Offline.to_string(), "Offline");
        assert_eq!(AgentStatus::Busy { tasks: 3 }.to_string(), "Busy");
    }

    #[test]
    fn test_agent_status_serialization() {
        let status = AgentStatus::Online;
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("Online"));

        let status = AgentStatus::Busy { tasks: 5 };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("Busy"));
        assert!(json.contains("5"));
    }

    #[test]
    fn test_agent_list_request_serialization() {
        let req = AgentListRequest {
            path: "/var/log".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("/var/log"));

        let deserialized: AgentListRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.path, "/var/log");
    }

    #[test]
    fn test_agent_file_item_serialization() {
        let item = AgentFileItem {
            name: "test.log".to_string(),
            path: "/var/log/test.log".to_string(),
            is_dir: false,
            is_symlink: false,
            size: Some(1024),
            modified: Some(1234567890),
            child_count: None,
            hidden_child_count: None,
            mime_type: Some("text/plain".to_string()),
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("test.log"));
        assert!(json.contains("1024"));

        let deserialized: AgentFileItem = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test.log");
        assert_eq!(deserialized.size, Some(1024));
    }

    #[test]
    fn test_agent_list_response() {
        let response = AgentListResponse {
            items: vec![
                AgentFileItem {
                    name: "file1.log".to_string(),
                    path: "/var/log/file1.log".to_string(),
                    is_dir: false,
                    is_symlink: false,
                    size: Some(100),
                    modified: None,
                    child_count: None,
                    hidden_child_count: None,
                    mime_type: None,
                },
            ],
        };

        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].name, "file1.log");
    }
}
