//! Endpoint 模块 - 存储端点概念
//!
//! 定义了三个基础维度（Location、StorageBackend、AccessMethod）和它们的组合（Endpoint）

/// 资源位置
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Location {
    /// 本机 - 直接访问本地硬件
    Local,
    /// 远程主机 - 通过网络访问
    Remote { host: String, port: u16 },
    /// 云服务 - 通过互联网访问
    Cloud,
}

/// 存储后端类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageBackend {
    /// 目录型存储 - 支持真实目录层级的文件系统
    Directory,
    /// 对象存储 - 扁平键空间的存储系统
    ObjectStorage,
}

/// 访问方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessMethod {
    /// 直接访问 - 使用原生 SDK 或系统调用
    Direct,
    /// 代理访问 - 通过中间代理转发
    Proxy,
}

/// 存储端点
///
/// Endpoint 是三个基础维度的组合，表示一个具体的存储端点
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// 资源位置
    pub location: Location,
    /// 存储后端
    pub backend: StorageBackend,
    /// 访问方式
    pub access_method: AccessMethod,
    /// 端点标识符
    pub identity: String,
    /// S3 bucket 名称（仅用于 S3 端点）
    pub bucket: Option<String>,
}

impl Endpoint {
    /// 本地文件系统
    pub fn local_fs() -> Self {
        Endpoint {
            location: Location::Local,
            backend: StorageBackend::Directory,
            access_method: AccessMethod::Direct,
            identity: "localhost".to_string(),
            bucket: None,
        }
    }

    /// Agent 代理（远程文件系统）
    pub fn agent(host: String, port: u16, agent_name: String) -> Self {
        Endpoint {
            location: Location::Remote { host, port },
            backend: StorageBackend::Directory,
            access_method: AccessMethod::Proxy,
            identity: agent_name,
            bucket: None,
        }
    }

    /// S3 对象存储
    pub fn s3(profile: String) -> Self {
        Endpoint {
            location: Location::Cloud,
            backend: StorageBackend::ObjectStorage,
            access_method: AccessMethod::Direct,
            identity: profile,
            bucket: None,
        }
    }

    /// S3 对象存储（带 bucket）
    pub fn s3_with_bucket(profile: String, bucket: String) -> Self {
        Endpoint {
            location: Location::Cloud,
            backend: StorageBackend::ObjectStorage,
            access_method: AccessMethod::Direct,
            identity: profile,
            bucket: Some(bucket),
        }
    }

    /// Agent discovery - 用于列出所有在线 agent 的虚拟端点
    pub fn agent_discovery() -> Self {
        Endpoint {
            location: Location::Local,
            backend: StorageBackend::Directory,
            access_method: AccessMethod::Direct,
            identity: "agent.root".to_string(),
            bucket: None,
        }
    }

    /// S3 discovery - 用于列出所有 S3 profile 的虚拟端点
    pub fn s3_discovery() -> Self {
        Endpoint {
            location: Location::Local,
            backend: StorageBackend::Directory,
            access_method: AccessMethod::Direct,
            identity: "s3.root".to_string(),
            bucket: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_fs_endpoint() {
        let endpoint = Endpoint::local_fs();
        assert_eq!(endpoint.location, Location::Local);
        assert_eq!(endpoint.backend, StorageBackend::Directory);
        assert_eq!(endpoint.access_method, AccessMethod::Direct);
        assert_eq!(endpoint.identity, "localhost");
    }

    #[test]
    fn test_agent_endpoint() {
        let endpoint = Endpoint::agent("192.168.1.100".to_string(), 4001, "web-01".to_string());
        assert!(matches!(endpoint.location, Location::Remote { .. }));
        assert_eq!(endpoint.backend, StorageBackend::Directory);
        assert_eq!(endpoint.access_method, AccessMethod::Proxy);
        assert_eq!(endpoint.identity, "web-01");
    }

    #[test]
    fn test_s3_endpoint() {
        let endpoint = Endpoint::s3("backup".to_string());
        assert_eq!(endpoint.location, Location::Cloud);
        assert_eq!(endpoint.backend, StorageBackend::ObjectStorage);
        assert_eq!(endpoint.access_method, AccessMethod::Direct);
        assert_eq!(endpoint.identity, "backup");
        assert!(endpoint.bucket.is_none());
    }

    #[test]
    fn test_s3_endpoint_with_bucket() {
        let endpoint = Endpoint::s3_with_bucket("backup".to_string(), "my-bucket".to_string());
        assert_eq!(endpoint.location, Location::Cloud);
        assert_eq!(endpoint.backend, StorageBackend::ObjectStorage);
        assert_eq!(endpoint.access_method, AccessMethod::Direct);
        assert_eq!(endpoint.identity, "backup");
        assert_eq!(endpoint.bucket, Some("my-bucket".to_string()));
    }
}
