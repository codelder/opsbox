//! S3 文件系统连接器
//!
//! TODO: 实现适配到 S3OpsFS

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::AsyncRead;

use opsbox_domain::resource::{
    EndpointConnector, ResourceMetadata, ResourcePath, DomainError,
};

/// S3 端点连接器（占位实现）
pub struct S3EndpointConnector {
    _private: (),
}

impl S3EndpointConnector {
    /// 占位构造函数
    pub fn new() -> Self {
        Self { _private: () }
    }
}

#[async_trait]
impl EndpointConnector for S3EndpointConnector {
    async fn metadata(&self, _path: &ResourcePath) -> Result<ResourceMetadata, DomainError> {
        Err(DomainError::ResourceNotFound("S3 connector not yet implemented".to_string()))
    }

    async fn list(&self, _path: &ResourcePath) -> Result<Vec<ResourceMetadata>, DomainError> {
        Err(DomainError::ResourceNotFound("S3 connector not yet implemented".to_string()))
    }

    async fn read(&self, _path: &ResourcePath) -> Result<Pin<Box<dyn AsyncRead + Send + Unpin>>, DomainError> {
        Err(DomainError::ResourceNotFound("S3 connector not yet implemented".to_string()))
    }

    async fn exists(&self, _path: &ResourcePath) -> Result<bool, DomainError> {
        Err(DomainError::ResourceNotFound("S3 connector not yet implemented".to_string()))
    }
}
