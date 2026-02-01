//! 本地文件系统连接器
//!
//! 将 EndpointConnector 适配到 LocalOpsFS。

use std::pin::Pin;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::AsyncRead;

use opsbox_core::odfs::{providers::local::LocalOpsFS, OpsFileSystem};
use opsbox_domain::resource::{
    EndpointConnector, ResourceMetadata, ResourcePath, DomainError,
};

/// 本地端点连接器
///
/// 委托给 LocalOpsFS 实现。
pub struct LocalEndpointConnector {
    inner: Arc<LocalOpsFS>,
    root: PathBuf,
}

impl LocalEndpointConnector {
    /// 创建新的本地连接器
    pub fn new(root: String) -> Self {
        let root_path = PathBuf::from(&root);
        Self {
            inner: Arc::new(LocalOpsFS::new(Some(root_path.clone()))),
            root: root_path,
        }
    }

    /// 从现有的 LocalOpsFS 创建
    pub fn from_opsfs(fs: Arc<LocalOpsFS>) -> Self {
        // 注意：无法从 LocalOpsFS 获取 root，因为它是私有的
        // 这个方法主要用于测试，使用默认 root
        Self {
            inner: fs,
            root: PathBuf::from("/"),
        }
    }

    /// 获取内部文件系统引用
    pub fn inner(&self) -> &LocalOpsFS {
        &self.inner
    }

    /// 获取 root 路径
    pub fn root(&self) -> &PathBuf {
        &self.root
    }
}

#[async_trait]
impl EndpointConnector for LocalEndpointConnector {
    /// 获取资源元数据
    async fn metadata(&self, path: &ResourcePath) -> Result<ResourceMetadata, DomainError> {
        use opsbox_core::odfs::OpsPath;

        let ops_path = OpsPath::new(path.as_str());
        let ops_meta = self.inner.as_ref().metadata(&ops_path).await
            .map_err(|e| DomainError::ResourceNotFound(format!("Failed to get metadata: {}", e)))?;

        Ok(convert_metadata(ops_meta))
    }

    /// 列出目录内容
    async fn list(&self, path: &ResourcePath) -> Result<Vec<ResourceMetadata>, DomainError> {
        use opsbox_core::odfs::{OpsFileSystem, OpsPath};

        let ops_path = OpsPath::new(path.as_str());
        let entries = self.inner.as_ref().read_dir(&ops_path).await
            .map_err(|e| DomainError::ResourceNotFound(format!("Failed to list directory: {}", e)))?;

        entries.into_iter()
            .map(|entry| Ok(convert_metadata(entry.metadata)))
            .collect()
    }

    /// 读取文件内容
    async fn read(&self, path: &ResourcePath) -> Result<Pin<Box<dyn AsyncRead + Send + Unpin>>, DomainError> {
        use opsbox_core::odfs::{OpsFileSystem, OpsPath};

        let ops_path = OpsPath::new(path.as_str());
        let ops_read = self.inner.as_ref().open_read(&ops_path).await
            .map_err(|e| DomainError::ResourceNotFound(format!("Failed to open file: {}", e)))?;

        // OpsRead 已经是 Pin<Box<dyn AsyncRead + Send + Unpin>>，可以直接返回
        Ok(ops_read)
    }

    /// 检查资源是否存在
    async fn exists(&self, path: &ResourcePath) -> Result<bool, DomainError> {
        use opsbox_core::odfs::{OpsFileSystem, OpsPath};

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
    let is_dir = ops_meta.is_dir();
    ResourceMetadata {
        name: ops_meta.name.clone(),
        size: ops_meta.size,
        is_dir,
        modified: ops_meta.modified.map(|t| t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64).unwrap_or(0)),
        mime_type: ops_meta.mime_type.clone(),
        is_archive: ops_meta.is_archive,
        child_count: None,  // OpsMetadata 没有 child_count 字段
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opsbox_core::odfs::OpsFileSystem;
    use tokio::io::AsyncReadExt;

    #[test]
    fn test_local_connector_new() {
        let connector = LocalEndpointConnector::new("/tmp".to_string());
        // 验证创建成功
        assert_eq!(connector.inner().name(), "LocalOpsFS");
    }

    #[tokio::test]
    async fn test_local_connector_metadata() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        // 创建测试文件和目录
        let sub_dir = root.join("test_dir");
        tokio::fs::create_dir(&sub_dir).await.unwrap();
        tokio::fs::write(sub_dir.join("file.txt"), "hello world").await.unwrap();
        tokio::fs::write(root.join("root.txt"), "root content").await.unwrap();

        let connector = LocalEndpointConnector::new(root.to_str().unwrap().to_string());

        // 测试目录元数据
        let meta = connector.metadata(&ResourcePath::new("test_dir")).await.unwrap();
        assert!(meta.is_dir);
        assert_eq!(meta.name, "test_dir");

        // 测试文件元数据
        let meta = connector.metadata(&ResourcePath::new("root.txt")).await.unwrap();
        assert!(!meta.is_dir);
        assert_eq!(meta.name, "root.txt");
        assert_eq!(meta.size, 12); // "root content" = 12 bytes
    }

    #[tokio::test]
    async fn test_local_connector_list() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        // 创建测试结构
        let sub_dir = root.join("dir1");
        tokio::fs::create_dir(&sub_dir).await.unwrap();
        tokio::fs::write(sub_dir.join("file1.txt"), "content1").await.unwrap();
        tokio::fs::write(sub_dir.join("file2.txt"), "content2").await.unwrap();
        tokio::fs::write(root.join("root_file.txt"), "root content").await.unwrap();

        let connector = LocalEndpointConnector::new(root.to_str().unwrap().to_string());

        // 列出根目录
        let items = connector.list(&ResourcePath::new("/")).await.unwrap();
        assert_eq!(items.len(), 2); // dir1 和 root_file.txt

        // 列出子目录
        let items = connector.list(&ResourcePath::new("dir1")).await.unwrap();
        assert_eq!(items.len(), 2); // file1.txt 和 file2.txt

        let names: Vec<&str> = items.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"file1.txt"));
        assert!(names.contains(&"file2.txt"));
    }

    #[tokio::test]
    async fn test_local_connector_read() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        let test_content = "hello from local connector";
        tokio::fs::write(root.join("test.txt"), test_content).await.unwrap();

        let connector = LocalEndpointConnector::new(root.to_str().unwrap().to_string());

        // 读取文件
        let mut reader = connector.read(&ResourcePath::new("test.txt")).await.unwrap();
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer).await.unwrap();

        assert_eq!(String::from_utf8(buffer).unwrap(), test_content);
    }

    #[tokio::test]
    async fn test_local_connector_exists() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        tokio::fs::write(root.join("exists.txt"), "content").await.unwrap();

        let connector = LocalEndpointConnector::new(root.to_str().unwrap().to_string());

        // 测试存在的文件
        assert!(connector.exists(&ResourcePath::new("exists.txt")).await.unwrap());

        // 测试不存在的文件
        assert!(!connector.exists(&ResourcePath::new("not_exists.txt")).await.unwrap());

        // 测试存在的目录
        assert!(connector.exists(&ResourcePath::new("/")).await.unwrap());
    }

    #[tokio::test]
    async fn test_local_connector_not_found() {
        let temp_dir = tempfile::tempdir().unwrap();
        let connector = LocalEndpointConnector::new(temp_dir.path().to_str().unwrap().to_string());

        // 测试不存在的文件
        let result = connector.metadata(&ResourcePath::new("not_found.txt")).await;
        assert!(result.is_err());

        // 测试不存在的目录列表
        let result = connector.list(&ResourcePath::new("not_found_dir")).await;
        assert!(result.is_err());

        // 测试读取不存在的文件
        let result = connector.read(&ResourcePath::new("not_found.txt")).await;
        assert!(result.is_err());
    }
}
