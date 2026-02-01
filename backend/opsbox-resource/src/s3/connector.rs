//! S3 文件系统连接器
//!
//! 将 EndpointConnector 适配到 S3OpsFS。

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use aws_sdk_s3::Client;
use tokio::io::AsyncRead;

use opsbox_core::odfs::{providers::s3::S3OpsFS, OpsFileSystem, OpsPath};
use opsbox_domain::resource::{
    EndpointConnector, ResourceMetadata, ResourcePath, DomainError,
};

/// S3 端点连接器
///
/// 委托给 S3OpsFS 实现。
pub struct S3EndpointConnector {
    inner: Arc<S3OpsFS>,
}

impl S3EndpointConnector {
    /// 从 S3 配置创建新的连接器
    pub fn new(client: Client, bucket: String) -> Self {
        Self {
            inner: Arc::new(S3OpsFS::new(client, bucket)),
        }
    }

    /// 从现有的 S3OpsFS 创建
    pub fn from_opsfs(fs: Arc<S3OpsFS>) -> Self {
        Self {
            inner: fs,
        }
    }

    /// 获取内部文件系统引用
    pub fn inner(&self) -> &S3OpsFS {
        &self.inner
    }
}

#[async_trait]
impl EndpointConnector for S3EndpointConnector {
    /// 获取资源元数据
    async fn metadata(&self, path: &ResourcePath) -> Result<ResourceMetadata, DomainError> {
        let ops_path = OpsPath::new(path.as_str());
        let ops_meta = self.inner.as_ref().metadata(&ops_path).await
            .map_err(|e| DomainError::ResourceNotFound(format!("Failed to get metadata: {}", e)))?;

        Ok(convert_metadata(ops_meta))
    }

    /// 列出目录内容
    async fn list(&self, path: &ResourcePath) -> Result<Vec<ResourceMetadata>, DomainError> {
        let ops_path = OpsPath::new(path.as_str());
        let entries = self.inner.as_ref().read_dir(&ops_path).await
            .map_err(|e| DomainError::ResourceNotFound(format!("Failed to list directory: {}", e)))?;

        entries.into_iter()
            .map(|entry| Ok(convert_metadata(entry.metadata)))
            .collect()
    }

    /// 读取文件内容
    async fn read(&self, path: &ResourcePath) -> Result<Pin<Box<dyn AsyncRead + Send + Unpin>>, DomainError> {
        let ops_path = OpsPath::new(path.as_str());
        let ops_read = self.inner.as_ref().open_read(&ops_path).await
            .map_err(|e| DomainError::ResourceNotFound(format!("Failed to open file: {}", e)))?;

        // OpsRead 已经是 Pin<Box<dyn AsyncRead + Send + Unpin>>，可以直接返回
        Ok(ops_read)
    }

    /// 检查资源是否存在
    async fn exists(&self, path: &ResourcePath) -> Result<bool, DomainError> {
        let ops_path = OpsPath::new(path.as_str());
        match self.inner.as_ref().metadata(&ops_path).await {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(DomainError::ResourceNotFound(format!("Failed to check existence: {}", e))),
        }
    }
}

/// 转换 OpsMetadata 到 ResourceMetadata
fn convert_metadata(ops_meta: opsbox_core::odfs::types::OpsMetadata) -> ResourceMetadata {
    use opsbox_core::odfs::types::OpsFileType;

    let is_dir = matches!(ops_meta.file_type, OpsFileType::Directory);
    ResourceMetadata {
        name: ops_meta.name.clone(),
        size: ops_meta.size,
        is_dir,
        modified: ops_meta.modified.map(|t| t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64).unwrap_or(0)),
        mime_type: ops_meta.mime_type.clone(),
        is_archive: ops_meta.is_archive,
        child_count: None,  // S3 不直接提供子目录数量
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_s3_connector_new() {
        let config = aws_sdk_s3::Config::builder()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .build();
        let client = Client::from_conf(config);
        let connector = S3EndpointConnector::new(client, "test-bucket".to_string());
        // 验证创建成功
        assert_eq!(connector.inner().name(), "S3OpsFS");
    }
}
