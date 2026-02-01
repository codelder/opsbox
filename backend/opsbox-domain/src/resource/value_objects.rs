//! Resource 上下文值对象
//!
//! 类型安全的资源标识符，替代字符串 ORL。

use std::fmt;
use std::str::FromStr;

use fluent_uri::Uri;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// 资源端点类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndpointType {
    Local,
    Agent,
    S3,
}

impl FromStr for EndpointType {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local" => Ok(EndpointType::Local),
            "agent" => Ok(EndpointType::Agent),
            "s3" => Ok(EndpointType::S3),
            _ => Err(DomainError::InvalidEndpointType(s.to_string())),
        }
    }
}

impl fmt::Display for EndpointType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EndpointType::Local => write!(f, "local"),
            EndpointType::Agent => write!(f, "agent"),
            EndpointType::S3 => write!(f, "s3"),
        }
    }
}

/// 端点引用（值对象）
///
/// 指向特定端点的轻量级引用，包含端点类型和标识符。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EndpointReference {
    /// 端点类型
    pub endpoint_type: EndpointType,
    /// 端点标识符（AgentID / ProfileName / "local"）
    pub id: String,
    /// 可选的服务器地址（仅用于 Agent）
    pub server_addr: Option<String>,
}

impl EndpointReference {
    /// 创建新的端点引用
    pub fn new(endpoint_type: EndpointType, id: String) -> Self {
        Self {
            endpoint_type,
            id,
            server_addr: None,
        }
    }

    /// 创建带服务器地址的端点引用（用于 Agent）
    pub fn with_server(mut self, addr: String) -> Self {
        if addr.is_empty() {
            self
        } else {
            self.server_addr = Some(addr);
            self
        }
    }

    /// 创建本地端点引用
    pub fn local() -> Self {
        Self::new(EndpointType::Local, "localhost".to_string())
    }

    /// 判断是否为本地端点
    pub fn is_local(&self) -> bool {
        self.endpoint_type == EndpointType::Local
    }
}

/// 资源路径（值对象）
///
/// 封装资源路径，提供安全的路径操作。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourcePath(String);

impl ResourcePath {
    /// 创建新的资源路径
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    /// 获取路径字符串
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 连接子路径
    pub fn join(&self, subpath: &str) -> Self {
        let subpath = subpath.trim_start_matches('/');
        if self.0.ends_with('/') {
            Self(format!("{}{}", self.0, subpath))
        } else {
            Self(format!("{}/{}", self.0, subpath))
        }
    }

    /// 获取父目录路径
    pub fn parent(&self) -> Option<Self> {
        if self.0 == "/" || self.0.is_empty() {
            None
        } else {
            // 规范化路径：移除尾部斜杠
            let normalized = if self.0.ends_with('/') && self.0.len() > 1 {
                &self.0[..self.0.len() - 1]
            } else {
                &self.0
            };

            // 查找最后一个斜杠
            if let Some(pos) = normalized.rfind('/') {
                let parent = if pos == 0 { "/" } else { &normalized[..pos] };
                Some(Self(parent.to_string()))
            } else {
                None
            }
        }
    }

    /// 获取文件名
    pub fn file_name(&self) -> Option<&str> {
        // 规范化路径：移除尾部斜杠
        let normalized = if self.0.ends_with('/') && self.0.len() > 1 {
            &self.0[..self.0.len() - 1]
        } else {
            &self.0
        };

        normalized.split('/').next_back().filter(|s| !s.is_empty())
    }
}

impl AsRef<str> for ResourcePath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ResourcePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 归档条目路径（值对象）
///
/// 表示归档文件内部的路径。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArchiveEntryPath(String);

impl ArchiveEntryPath {
    /// 创建新的归档条目路径
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    /// 获取路径字符串
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ArchiveEntryPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ArchiveEntryPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 资源标识符（值对象）
///
/// 类型安全的统一资源标识符，替代字符串 ORL。
///
/// 格式：`orl://[id]@[type][.server_addr]/[path]?entry=[entry_path]`
///
/// # 示例
///
/// ```ignore
/// // 本地文件
/// let id = ResourceIdentifier::local("/var/log/nginx/access.log");
///
/// // Agent 文件
/// let id = ResourceIdentifier::agent("web-01", "/app/logs/error.log", None);
///
/// // S3 归档
/// let id = ResourceIdentifier::s3_archive(
///     "prod",
///     "/bucket/logs/2023/10/data.tar.gz",
///     Some("internal/service.log")
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceIdentifier {
    /// 端点引用
    pub endpoint: EndpointReference,
    /// 资源路径
    pub path: ResourcePath,
    /// 归档条目路径（可选）
    pub archive_entry: Option<ArchiveEntryPath>,
}

impl ResourceIdentifier {
    /// 创建本地资源标识符
    pub fn local(path: impl Into<String>) -> Self {
        Self {
            endpoint: EndpointReference::local(),
            path: ResourcePath::new(path.into()),
            archive_entry: None,
        }
    }

    /// 创建 Agent 资源标识符
    pub fn agent(agent_id: impl Into<String>, path: impl Into<String>, server_addr: Option<String>) -> Self {
        Self {
            endpoint: EndpointReference::new(EndpointType::Agent, agent_id.into())
                .with_server(server_addr.unwrap_or_default()),
            path: ResourcePath::new(path.into()),
            archive_entry: None,
        }
    }

    /// 创建 S3 资源标识符
    pub fn s3(profile: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            endpoint: EndpointReference::new(EndpointType::S3, profile.into()),
            path: ResourcePath::new(path.into()),
            archive_entry: None,
        }
    }

    /// 创建归档资源标识符
    pub fn archive(mut self, entry: impl Into<String>) -> Self {
        self.archive_entry = Some(ArchiveEntryPath::new(entry.into()));
        self
    }

    /// 检查是否为归档资源
    pub fn is_archive(&self) -> bool {
        self.archive_entry.is_some() ||
            self.path.as_str().ends_with(".tar") ||
            self.path.as_str().ends_with(".tar.gz") ||
            self.path.as_str().ends_with(".tgz") ||
            self.path.as_str().ends_with(".zip")
    }

    /// 获取显示名称
    pub fn display_name(&self) -> String {
        if let Some(entry) = &self.archive_entry {
            entry.as_str().split('/').next_back()
                .unwrap_or(entry.as_str()).to_string()
        } else {
            self.path.file_name()
                .unwrap_or(self.path.as_str()).to_string()
        }
    }

    /// 连接子路径
    pub fn join(&self, subpath: &str) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            path: self.path.join(subpath),
            archive_entry: self.archive_entry.clone(),
        }
    }
}

impl fmt::Display for ResourceIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "orl://{}@{}", self.endpoint.id, self.endpoint.endpoint_type)?;
        if let Some(addr) = &self.endpoint.server_addr {
            write!(f, ".{}", addr)?;
        }
        write!(f, "{}", self.path)?;
        if let Some(entry) = &self.archive_entry {
            write!(f, "?entry={}", entry.as_str())?;
        }
        Ok(())
    }
}

impl FromStr for ResourceIdentifier {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // 解析 URI
        let uri = Uri::parse(s)
            .map_err(|e| DomainError::InvalidResourceIdentifier(format!("Invalid URI: {}", e)))?;

        // 验证 scheme
        if uri.scheme().as_str() != "orl" {
            return Err(DomainError::InvalidResourceIdentifier(
                format!("Unsupported scheme: {}", uri.scheme().as_str())
            ));
        }

        // 获取 authority
        let authority = uri.authority()
            .ok_or_else(|| DomainError::InvalidResourceIdentifier("Missing authority".to_string()))?;

        // 解析端点类型和 ID
        let host = authority.host();
        let type_str = host.split('.').next().unwrap_or(host);
        let endpoint_type = EndpointType::from_str(type_str)?;
        let id = authority.userinfo()
            .map(|u| u.as_str().to_string())
            .unwrap_or_else(|| match endpoint_type {
                EndpointType::Agent => "root".to_string(),
                _ => "localhost".to_string(),
            });

        // 解析服务器地址（仅用于 Agent）
        // 格式: type.addr 或 type.addr.port
        let server_addr = if endpoint_type == EndpointType::Agent {
            host.split('.').skip(1).collect::<Vec<_>>().join(".").parse().ok()
        } else {
            None
        };

        let endpoint = EndpointReference::new(endpoint_type, id)
            .with_server(server_addr.unwrap_or_default());

        // 解析路径
        let path = ResourcePath::new(uri.path().as_str());

        // 解析归档条目
        let archive_entry = uri.query()
            .and_then(|q| {
                q.as_str().split('&')
                    .find_map(|pair| {
                        let mut parts = pair.splitn(2, '=');
                        if parts.next() == Some("entry") {
                            parts.next()
                        } else {
                            None
                        }
                    })
            })
            .map(ArchiveEntryPath::new);

        Ok(Self { endpoint, path, archive_entry })
    }
}

impl Serialize for ResourceIdentifier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ResourceIdentifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// 领域错误
#[derive(Debug, Error, PartialEq)]
pub enum DomainError {
    #[error("Invalid endpoint type: {0}")]
    InvalidEndpointType(String),

    #[error("Invalid resource identifier: {0}")]
    InvalidResourceIdentifier(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Access denied: {0}")]
    AccessDenied(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_type_from_str() {
        assert_eq!(EndpointType::from_str("local"), Ok(EndpointType::Local));
        assert_eq!(EndpointType::from_str("agent"), Ok(EndpointType::Agent));
        assert_eq!(EndpointType::from_str("s3"), Ok(EndpointType::S3));
        assert!(EndpointType::from_str("invalid").is_err());
    }

    #[test]
    fn test_resource_identifier_local() {
        let id = ResourceIdentifier::local("/var/log/app.log");
        assert!(id.endpoint.is_local());
        assert_eq!(id.endpoint.id, "localhost");
        assert_eq!(id.path.as_str(), "/var/log/app.log");
        assert!(id.archive_entry.is_none());
    }

    #[test]
    fn test_resource_identifier_agent() {
        let id = ResourceIdentifier::agent("web-01", "/app/logs/error.log", Some("192.168.1.100".to_string()));
        assert_eq!(id.endpoint.endpoint_type, EndpointType::Agent);
        assert_eq!(id.endpoint.id, "web-01");
        assert_eq!(id.endpoint.server_addr, Some("192.168.1.100".to_string()));
    }

    #[test]
    fn test_resource_identifier_s3() {
        let id = ResourceIdentifier::s3("prod", "/bucket/logs/data.tar.gz");
        assert_eq!(id.endpoint.endpoint_type, EndpointType::S3);
        assert_eq!(id.endpoint.id, "prod");
        assert_eq!(id.path.as_str(), "/bucket/logs/data.tar.gz");
    }

    #[test]
    fn test_resource_identifier_archive() {
        let id = ResourceIdentifier::s3("prod", "/bucket/logs/data.tar.gz")
            .archive("internal/service.log");
        assert!(id.is_archive());
        assert_eq!(id.archive_entry.as_ref().map(|e| e.as_str()), Some("internal/service.log"));
    }

    #[test]
    fn test_resource_identifier_display_name() {
        let id = ResourceIdentifier::local("/var/log/app.log");
        assert_eq!(id.display_name(), "app.log");

        let id = ResourceIdentifier::local("/archive.tar.gz")
            .archive("inner/file.log");
        assert_eq!(id.display_name(), "file.log");
    }

    #[test]
    fn test_resource_identifier_join() {
        let id = ResourceIdentifier::local("/var/log");
        let joined = id.join("app.log");
        assert_eq!(joined.path.as_str(), "/var/log/app.log");
    }

    #[test]
    fn test_resource_identifier_from_str() {
        let id: ResourceIdentifier = "orl://local/var/log/app.log".parse().unwrap();
        assert!(id.endpoint.is_local());
        assert_eq!(id.path.as_str(), "/var/log/app.log");

        let id: ResourceIdentifier = "orl://web-01@agent.192.168.1.100/app/log?entry=inner.log".parse().unwrap();
        assert_eq!(id.endpoint.endpoint_type, EndpointType::Agent);
        assert_eq!(id.endpoint.id, "web-01");
        assert_eq!(id.endpoint.server_addr, Some("192.168.1.100".to_string()));
        assert_eq!(id.archive_entry.as_ref().map(|e| e.as_str()), Some("inner.log"));
    }

    #[test]
    fn test_resource_path_join() {
        let path = ResourcePath::new("/var/log");
        let joined = path.join("app.log");
        assert_eq!(joined.as_str(), "/var/log/app.log");

        let path = ResourcePath::new("/var/log/");
        let joined = path.join("/app.log");
        assert_eq!(joined.as_str(), "/var/log/app.log");
    }

    #[test]
    fn test_resource_path_parent() {
        let path = ResourcePath::new("/var/log/app.log");
        assert_eq!(path.parent().map(|p| p.to_string()), Some("/var/log".to_string()));

        let path = ResourcePath::new("/var/log/");
        assert_eq!(path.parent().map(|p| p.to_string()), Some("/var".to_string()));
    }

    #[test]
    fn test_resource_path_file_name() {
        let path = ResourcePath::new("/var/log/app.log");
        assert_eq!(path.file_name(), Some("app.log"));

        let path = ResourcePath::new("/var/log/");
        assert_eq!(path.file_name(), Some("log"));
    }

    #[test]
    fn test_resource_identifier_serialization() {
        let id = ResourceIdentifier::local("/var/log/app.log");
        let json = serde_json::to_string(&id).unwrap();
        assert!(json.contains("orl://"));

        let deserialized: ResourceIdentifier = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, id);
    }
}
