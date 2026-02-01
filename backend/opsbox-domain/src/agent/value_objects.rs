//! Agent 上下文值对象
//!
//! Agent 相关的值对象定义。

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Agent 标签（key=value 形式）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

/// Agent 连接信息（值对象）
///
/// 封装 Agent 的网络连接状态和超时配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentConnection {
    /// 基础 URL
    pub base_url: String,
    /// 监听端口
    pub listen_port: u16,
    /// 最后心跳时间
    pub last_heartbeat: i64,
    /// 心跳超时时间（秒）
    pub heartbeat_timeout_secs: i64,
}

impl AgentConnection {
    /// 创建新的连接信息
    pub fn new(
        base_url: String,
        listen_port: u16,
        heartbeat_timeout_secs: i64,
    ) -> Self {
        Self {
            base_url,
            listen_port,
            last_heartbeat: chrono::Utc::now().timestamp(),
            heartbeat_timeout_secs,
        }
    }

    /// 创建带自定义心跳时间的连接信息
    pub fn with_heartbeat(
        base_url: String,
        listen_port: u16,
        heartbeat_timeout_secs: i64,
        last_heartbeat: i64,
    ) -> Self {
        Self {
            base_url,
            listen_port,
            last_heartbeat,
            heartbeat_timeout_secs,
        }
    }

    /// 检查是否在线
    pub fn is_online(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now - self.last_heartbeat < self.heartbeat_timeout_secs
    }

    /// 更新心跳时间
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = chrono::Utc::now().timestamp();
    }

    /// 获取完整的 API 地址
    pub fn api_url(&self) -> String {
        format!("{}:{}", self.base_url, self.listen_port)
    }

    /// 获取心跳延迟（秒）
    pub fn heartbeat_age(&self) -> i64 {
        chrono::Utc::now().timestamp() - self.last_heartbeat
    }
}

/// Agent 能力（值对象）
///
/// 描述 Agent 支持的功能和操作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilities {
    /// 支持的文件系统操作
    pub fs_operations: FsCapabilities,
    /// 支持的压缩格式
    pub archive_formats: HashSet<String>,
    /// 最大文件大小限制（字节，None 表示无限制）
    pub max_file_size: Option<u64>,
    /// 是否支持实时日志流
    pub supports_streaming: bool,
    /// 支持的编码格式
    pub supported_encodings: HashSet<String>,
}

impl AgentCapabilities {
    /// 创建新的能力描述
    pub fn new() -> Self {
        Self {
            fs_operations: FsCapabilities::default(),
            archive_formats: HashSet::new(),
            max_file_size: None,
            supports_streaming: false,
            supported_encodings: HashSet::new(),
        }
    }

    /// 添加支持的归档格式
    pub fn add_archive_format(mut self, format: String) -> Self {
        self.archive_formats.insert(format);
        self
    }

    /// 添加支持的编码
    pub fn add_encoding(mut self, encoding: String) -> Self {
        self.supported_encodings.insert(encoding);
        self
    }

    /// 设置最大文件大小
    pub fn with_max_file_size(mut self, size: u64) -> Self {
        self.max_file_size = Some(size);
        self
    }

    /// 启用流式传输
    pub fn with_streaming(mut self) -> Self {
        self.supports_streaming = true;
        self
    }

    /// 检查是否支持特定归档格式
    pub fn supports_archive_format(&self, format: &str) -> bool {
        self.archive_formats.contains(format)
    }

    /// 检查是否支持特定编码
    pub fn supports_encoding(&self, encoding: &str) -> bool {
        self.supported_encodings.is_empty() || self.supported_encodings.contains(encoding)
    }
}

impl Default for AgentCapabilities {
    fn default() -> Self {
        Self::new()
    }
}

/// 文件系统操作能力
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsCapabilities {
    /// 是否支持读取
    pub can_read: bool,
    /// 是否支持列出目录
    pub can_list: bool,
    /// 是否支持获取元数据
    pub can_metadata: bool,
    /// 是否支持归档导航
    pub can_archive_navigate: bool,
}

impl FsCapabilities {
    /// 创建完整的文件系统能力
    pub fn full() -> Self {
        Self {
            can_read: true,
            can_list: true,
            can_metadata: true,
            can_archive_navigate: true,
        }
    }

    /// 创建只读能力
    pub fn read_only() -> Self {
        Self {
            can_read: true,
            can_list: true,
            can_metadata: true,
            can_archive_navigate: false,
        }
    }
}

impl Default for FsCapabilities {
    fn default() -> Self {
        Self::full()
    }
}

/// Agent 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    /// 在线
    Online,
    /// 忙碌（正在执行任务）
    Busy { active_tasks: usize },
    /// 离线
    Offline,
}

impl AgentStatus {
    /// 检查是否可用（在线且不忙）
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Online)
    }

    /// 获取活跃任务数
    pub fn active_tasks(&self) -> Option<usize> {
        match self {
            Self::Busy { active_tasks } => Some(*active_tasks),
            _ => None,
        }
    }
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Online => write!(f, "Online"),
            Self::Busy { active_tasks } => write!(f, "Busy({} tasks)", active_tasks),
            Self::Offline => write!(f, "Offline"),
        }
    }
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
    fn test_agent_connection_new() {
        let conn = AgentConnection::new(
            "http://192.168.1.100".to_string(),
            3976,
            60,
        );

        assert_eq!(conn.base_url, "http://192.168.1.100");
        assert_eq!(conn.listen_port, 3976);
        assert_eq!(conn.heartbeat_timeout_secs, 60);
        assert!(conn.is_online());
    }

    #[test]
    fn test_agent_connection_api_url() {
        let conn = AgentConnection::new(
            "http://192.168.1.100".to_string(),
            8080,
            60,
        );

        assert_eq!(conn.api_url(), "http://192.168.1.100:8080");
    }

    #[test]
    fn test_agent_connection_update_heartbeat() {
        let mut conn = AgentConnection::new(
            "http://localhost".to_string(),
            3976,
            60,
        );

        // 使用旧的心跳时间来测试
        let old_heartbeat = conn.last_heartbeat;
        // 手动设置一个旧的心跳时间（1秒前）
        conn.last_heartbeat = chrono::Utc::now().timestamp() - 1;

        conn.update_heartbeat();
        assert!(conn.last_heartbeat >= old_heartbeat);
        assert!(conn.last_heartbeat > old_heartbeat - 1); // 确保更新了
    }

    #[test]
    fn test_agent_capabilities_new() {
        let caps = AgentCapabilities::new();
        assert!(caps.archive_formats.is_empty());
        assert!(!caps.supports_streaming);
    }

    #[test]
    fn test_agent_capabilities_builder() {
        let caps = AgentCapabilities::new()
            .add_archive_format("tar".to_string())
            .add_archive_format("zip".to_string())
            .add_encoding("utf-8".to_string())
            .with_max_file_size(1024 * 1024 * 100)
            .with_streaming();

        assert!(caps.supports_archive_format("tar"));
        assert!(caps.supports_archive_format("zip"));
        assert!(!caps.supports_archive_format("rar"));
        assert!(caps.supports_encoding("utf-8"));
        assert_eq!(caps.max_file_size, Some(1024 * 1024 * 100));
        assert!(caps.supports_streaming);
    }

    #[test]
    fn test_fs_capabilities_full() {
        let caps = FsCapabilities::full();
        assert!(caps.can_read);
        assert!(caps.can_list);
        assert!(caps.can_metadata);
        assert!(caps.can_archive_navigate);
    }

    #[test]
    fn test_fs_capabilities_read_only() {
        let caps = FsCapabilities::read_only();
        assert!(caps.can_read);
        assert!(caps.can_list);
        assert!(caps.can_metadata);
        assert!(!caps.can_archive_navigate);
    }

    #[test]
    fn test_agent_status_is_available() {
        assert!(AgentStatus::Online.is_available());
        assert!(!AgentStatus::Offline.is_available());
        assert!(!AgentStatus::Busy { active_tasks: 1 }.is_available());
    }

    #[test]
    fn test_agent_status_active_tasks() {
        assert_eq!(AgentStatus::Online.active_tasks(), None);
        assert_eq!(AgentStatus::Offline.active_tasks(), None);
        assert_eq!(AgentStatus::Busy { active_tasks: 3 }.active_tasks(), Some(3));
    }

    #[test]
    fn test_agent_status_display() {
        assert_eq!(AgentStatus::Online.to_string(), "Online");
        assert_eq!(AgentStatus::Offline.to_string(), "Offline");
        assert_eq!(AgentStatus::Busy { active_tasks: 5 }.to_string(), "Busy(5 tasks)");
    }

    #[test]
    fn test_agent_tag_equality() {
        let tag1 = AgentTag::new("env".to_string(), "prod".to_string());
        let tag2 = AgentTag::new("env".to_string(), "prod".to_string());
        let tag3 = AgentTag::new("env".to_string(), "dev".to_string());

        assert_eq!(tag1, tag2);
        assert_ne!(tag1, tag3);
    }

    #[test]
    fn test_agent_tag_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();

        set.insert(AgentTag::new("env".to_string(), "prod".to_string()));
        set.insert(AgentTag::new("env".to_string(), "prod".to_string())); // 重复
        set.insert(AgentTag::new("region".to_string(), "us".to_string()));

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_agent_connection_with_heartbeat() {
        let conn = AgentConnection::with_heartbeat(
            "http://localhost".to_string(),
            3976,
            60,
            1000000, // 很久以前的时间戳
        );

        assert!(!conn.is_online());
        assert!(conn.heartbeat_age() > 0);
    }
}
