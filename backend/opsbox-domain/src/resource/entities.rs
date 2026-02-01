//! Resource 上下文实体
//!
//! Resource 聚合根和相关的实体定义。

use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncRead;

use super::value_objects::{ResourceIdentifier, ResourcePath, DomainError};

/// 资源元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMetadata {
    /// 文件名
    pub name: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 是否为目录
    pub is_dir: bool,
    /// 修改时间（Unix timestamp）
    pub modified: Option<i64>,
    /// MIME 类型
    pub mime_type: Option<String>,
    /// 是否为归档文件
    pub is_archive: bool,
    /// 子项目数量（仅对目录有效）
    pub child_count: Option<u32>,
}

/// 端点连接器 Trait
///
/// 抽象不同端点（Local, S3, Agent）的资源访问能力。
#[async_trait]
pub trait EndpointConnector: Send + Sync {
    /// 获取资源元数据
    async fn metadata(&self, path: &ResourcePath) -> Result<ResourceMetadata, DomainError>;

    /// 列出目录内容
    async fn list(&self, path: &ResourcePath) -> Result<Vec<ResourceMetadata>, DomainError>;

    /// 读取文件内容
    async fn read(&self, path: &ResourcePath) -> Result<Pin<Box<dyn AsyncRead + Send + Unpin>>, DomainError>;

    /// 检查资源是否存在
    async fn exists(&self, path: &ResourcePath) -> Result<bool, DomainError>;
}

/// 端点注册表
///
/// 管理所有已配置的端点连接器。
pub struct EndpointRegistry {
    connectors: std::collections::HashMap<String, Box<dyn EndpointConnector>>,
}

impl EndpointRegistry {
    /// 创建新的端点注册表
    pub fn new() -> Self {
        Self {
            connectors: std::collections::HashMap::new(),
        }
    }

    /// 注册端点连接器
    pub fn register(&mut self, key: String, connector: Box<dyn EndpointConnector>) {
        self.connectors.insert(key, connector);
    }

    /// 获取端点连接器
    pub fn get(&self, key: &str) -> Option<&dyn EndpointConnector> {
        self.connectors.get(key).map(|b| b.as_ref())
    }

    /// 移除端点连接器
    pub fn remove(&mut self, key: &str) -> Option<Box<dyn EndpointConnector>> {
        self.connectors.remove(key)
    }
}

impl Default for EndpointRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 资源聚合根
///
/// 封装资源的访问行为和状态。
pub struct Resource {
    /// 资源标识符
    pub id: ResourceIdentifier,
    /// 资源元数据（延迟加载）
    metadata: Option<ResourceMetadata>,
    /// 端点连接器引用
    connector: Option<std::sync::Arc<dyn EndpointConnector>>,
}

impl Resource {
    /// 创建新的资源实例（延迟加载元数据）
    pub fn new(id: ResourceIdentifier) -> Self {
        Self {
            id,
            metadata: None,
            connector: None,
        }
    }

    /// 设置端点连接器
    pub fn with_connector(mut self, connector: std::sync::Arc<dyn EndpointConnector>) -> Self {
        self.connector = Some(connector);
        self
    }

    /// 从注册表加载资源
    pub async fn load(id: ResourceIdentifier, registry: &EndpointRegistry) -> Result<Self, DomainError> {
        let key = Self::endpoint_key(&id);
        let connector = registry.get(&key)
            .ok_or_else(|| DomainError::ResourceNotFound(format!("Endpoint not found: {}", key)))?;

        let metadata = connector.metadata(&id.path).await?;

        Ok(Self {
            id,
            metadata: Some(metadata),
            connector: None,
        })
    }

    /// 获取资源元数据（延迟加载）
    pub async fn get_metadata(&mut self) -> Result<&ResourceMetadata, DomainError> {
        if self.metadata.is_none() {
            if let Some(connector) = &self.connector {
                let metadata = connector.metadata(&self.id.path).await?;
                self.metadata = Some(metadata);
            } else {
                return Err(DomainError::ResourceNotFound(
                    "No connector available".to_string()
                ));
            }
        }
        Ok(self.metadata.as_ref().unwrap())
    }

    /// 列出子资源
    pub async fn list_children(&self) -> Result<Vec<Resource>, DomainError> {
        if let Some(connector) = &self.connector {
            let items = connector.list(&self.id.path).await?;

            Ok(items.into_iter().map(|meta| {
                let child_id = self.id.join(&meta.name);
                Resource {
                    id: child_id,
                    metadata: Some(meta),
                    connector: self.connector.clone(),
                }
            }).collect())
        } else {
            Err(DomainError::ResourceNotFound(
                "No connector available".to_string()
            ))
        }
    }

    /// 读取资源内容
    pub async fn read(&self) -> Result<Pin<Box<dyn AsyncRead + Send + Unpin>>, DomainError> {
        if let Some(connector) = &self.connector {
            connector.read(&self.id.path).await
        } else {
            Err(DomainError::ResourceNotFound(
                "No connector available".to_string()
            ))
        }
    }

    /// 检查资源是否存在
    pub async fn exists(&self) -> Result<bool, DomainError> {
        if let Some(connector) = &self.connector {
            connector.exists(&self.id.path).await
        } else {
            Err(DomainError::ResourceNotFound(
                "No connector available".to_string()
            ))
        }
    }

    /// 获取端点键（用于从注册表查找连接器）
    fn endpoint_key(id: &ResourceIdentifier) -> String {
        format!("{}:{}", id.endpoint.endpoint_type, id.endpoint.id)
    }

    /// 获取显示名称
    pub fn display_name(&self) -> String {
        self.id.display_name()
    }

    /// 检查是否为归档
    pub fn is_archive(&self) -> bool {
        self.id.is_archive()
    }

    /// 检查是否为目录
    pub fn is_dir(&self) -> bool {
        self.metadata.as_ref()
            .map(|m| m.is_dir)
            .unwrap_or(false)
    }

    /// 获取文件大小
    pub fn size(&self) -> Option<u64> {
        self.metadata.as_ref().map(|m| m.size)
    }
}

impl Clone for Resource {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            metadata: self.metadata.clone(),
            connector: self.connector.clone(),
        }
    }
}

/// 资源流
///
/// 用于流式处理大量资源。
pub struct ResourceStream {
    resources: Vec<Resource>,
    index: usize,
}

impl ResourceStream {
    /// 创建新的资源流
    pub fn new(resources: Vec<Resource>) -> Self {
        Self {
            resources,
            index: 0,
        }
    }

    /// 获取下一个资源
    pub fn next(&mut self) -> Option<Resource> {
        if self.index < self.resources.len() {
            let resource = self.resources[self.index].clone();
            self.index += 1;
            Some(resource)
        } else {
            None
        }
    }

    /// 检查是否还有更多资源
    pub fn has_more(&self) -> bool {
        self.index < self.resources.len()
    }

    /// 获取剩余资源数量
    pub fn remaining(&self) -> usize {
        self.resources.len().saturating_sub(self.index)
    }
}

impl Stream for ResourceStream {
    type Item = Resource;

    fn poll_next(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        Poll::Ready(this.next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_new() {
        let id = ResourceIdentifier::local("/var/log/app.log");
        let resource = Resource::new(id.clone());

        assert_eq!(resource.id, id);
        assert!(resource.metadata.is_none());
        assert!(resource.connector.is_none());
    }

    #[test]
    fn test_resource_display_name() {
        let id = ResourceIdentifier::local("/var/log/app.log");
        let resource = Resource::new(id);
        assert_eq!(resource.display_name(), "app.log");
    }

    #[test]
    fn test_resource_is_archive() {
        let id = ResourceIdentifier::local("/archive.tar.gz");
        let resource = Resource::new(id);
        assert!(resource.is_archive());
    }

    #[test]
    fn test_endpoint_registry() {
        let registry = EndpointRegistry::new();

        // 模拟连接器（这里只是类型检查，实际使用需要实现 trait）
        // registry.register("local:localhost".to_string(), Box::new(...));

        assert!(registry.get("local:localhost").is_none());
    }

    #[test]
    fn test_resource_stream() {
        let resources = vec![
            Resource::new(ResourceIdentifier::local("/file1.log")),
            Resource::new(ResourceIdentifier::local("/file2.log")),
        ];

        let mut stream = ResourceStream::new(resources);
        assert_eq!(stream.remaining(), 2);
        assert!(stream.has_more());

        assert!(stream.next().is_some());
        assert_eq!(stream.remaining(), 1);

        assert!(stream.next().is_some());
        assert_eq!(stream.remaining(), 0);
        assert!(!stream.has_more());

        assert!(stream.next().is_none());
    }

    #[test]
    fn test_resource_identifier_endpoint_key() {
        let id = ResourceIdentifier::local("/var/log");
        assert_eq!(Resource::endpoint_key(&id), "local:localhost");

        let id = ResourceIdentifier::agent("web-01", "/app/log", None);
        assert_eq!(Resource::endpoint_key(&id), "agent:web-01");

        let id = ResourceIdentifier::s3("prod", "/bucket/path");
        assert_eq!(Resource::endpoint_key(&id), "s3:prod");
    }
}
