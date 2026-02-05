//! Resource 模块 - 完整资源概念
//!
//! 定义了 Resource，它是访问 DFS 资源的完整描述

use super::{archive::ArchiveContext, endpoint::Endpoint, path::ResourcePath};

/// 完整的资源描述
///
/// Resource 是访问 DFS 资源的完整描述，组合了 Endpoint、Path 和可选的 ArchiveContext
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    /// 存储端点
    pub endpoint: Endpoint,
    /// 主路径
    pub primary_path: ResourcePath,
    /// 归档上下文（可选）
    pub archive_context: Option<ArchiveContext>,
}

impl Resource {
    /// 创建新的资源
    pub fn new(
        endpoint: Endpoint,
        primary_path: ResourcePath,
        archive_context: Option<ArchiveContext>,
    ) -> Self {
        Self {
            endpoint,
            primary_path,
            archive_context,
        }
    }

    /// 创建本地文件资源
    pub fn local(path: &str) -> Self {
        Self {
            endpoint: Endpoint::local_fs(),
            primary_path: ResourcePath::from_str(path),
            archive_context: None,
        }
    }

    /// 创建本地归档内文件资源
    pub fn local_archive(archive_path: &str, inner_path: &str, archive_type: Option<super::archive::ArchiveType>) -> Self {
        Self {
            endpoint: Endpoint::local_fs(),
            primary_path: ResourcePath::from_str(archive_path),
            archive_context: Some(ArchiveContext::from_path_str(inner_path, archive_type)),
        }
    }

    /// 创建 S3 对象资源
    pub fn s3(profile: String, path: &str) -> Self {
        Self {
            endpoint: Endpoint::s3(profile),
            primary_path: ResourcePath::from_str(path),
            archive_context: None,
        }
    }

    /// 创建 Agent 代理资源
    pub fn agent(host: String, port: u16, agent_name: String, path: &str) -> Self {
        Self {
            endpoint: Endpoint::agent(host, port, agent_name),
            primary_path: ResourcePath::from_str(path),
            archive_context: None,
        }
    }

    /// 创建 Agent 归档内文件资源
    pub fn agent_archive(
        host: String,
        port: u16,
        agent_name: String,
        archive_path: &str,
        inner_path: &str,
        archive_type: Option<super::archive::ArchiveType>,
    ) -> Self {
        Self {
            endpoint: Endpoint::agent(host, port, agent_name),
            primary_path: ResourcePath::from_str(archive_path),
            archive_context: Some(ArchiveContext::from_path_str(inner_path, archive_type)),
        }
    }

    /// 判断是否为归档资源
    pub fn is_archive(&self) -> bool {
        self.archive_context.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::super::archive::ArchiveType;
    use super::*;

    #[test]
    fn test_resource_local() {
        let resource = Resource::local("/var/log/app.log");
        assert!(matches!(resource.endpoint.location, super::super::endpoint::Location::Local));
        assert_eq!(resource.primary_path.to_string(), "/var/log/app.log");
        assert!(!resource.is_archive());
    }

    #[test]
    fn test_resource_local_archive() {
        let resource = Resource::local_archive("/data/archive.tar", "inner/file.txt", Some(ArchiveType::Tar));
        assert!(matches!(resource.endpoint.location, super::super::endpoint::Location::Local));
        assert_eq!(resource.primary_path.to_string(), "/data/archive.tar");
        assert!(resource.is_archive());
        assert_eq!(resource.archive_context.as_ref().unwrap().inner_path.to_string(), "inner/file.txt");
    }

    #[test]
    fn test_resource_s3() {
        let resource = Resource::s3("backup".to_string(), "bucket/2024/data.txt");
        assert!(matches!(resource.endpoint.location, super::super::endpoint::Location::Cloud));
        assert_eq!(resource.primary_path.to_string(), "bucket/2024/data.txt");
        assert!(!resource.is_archive());
    }

    #[test]
    fn test_resource_agent() {
        let resource = Resource::agent("192.168.1.100".to_string(), 4001, "web-01".to_string(), "/logs/backup.tar.gz");
        assert!(matches!(resource.endpoint.location, super::super::endpoint::Location::Remote { .. }));
        assert_eq!(resource.primary_path.to_string(), "/logs/backup.tar.gz");
        assert!(!resource.is_archive());
    }

    #[test]
    fn test_resource_agent_archive() {
        let resource = Resource::agent_archive(
            "192.168.1.100".to_string(),
            4001,
            "web-01".to_string(),
            "/logs/backup.tar.gz",
            "2024/01/app.log",
            Some(ArchiveType::TarGz),
        );
        assert!(matches!(resource.endpoint.location, super::super::endpoint::Location::Remote { .. }));
        assert_eq!(resource.primary_path.to_string(), "/logs/backup.tar.gz");
        assert!(resource.is_archive());
        assert_eq!(resource.archive_context.as_ref().unwrap().inner_path.to_string(), "2024/01/app.log");
    }

    #[test]
    fn test_resource_is_archive() {
        let normal = Resource::local("/var/log/app.log");
        assert!(!normal.is_archive());

        let archive = Resource::local_archive("/data/archive.tar", "file.txt", Some(ArchiveType::Tar));
        assert!(archive.is_archive());
    }
}
