//! 归档文件导航模块
//!
//! 提供 tar/zip 等归档格式的透明访问支持。

use std::pin::Pin;

use async_trait::async_trait;
use futures::StreamExt;
use tokio::io::{AsyncRead, BufReader};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

use opsbox_domain::resource::{
    EndpointConnector, ResourceMetadata, ResourcePath, DomainError,
};

/// 归档端点连接器
///
/// 包装其他连接器，提供归档文件的透明访问。
///
/// # 示例
///
/// ```ignore
/// let local = LocalEndpointConnector::new("/data".to_string());
/// let archive = ArchiveEndpointConnector::new(local, ResourcePath::new("/data/logs.tar.gz"));
/// ```
pub struct ArchiveEndpointConnector<C>
where
    C: EndpointConnector + Send + Sync,
{
    inner: C,
    archive_path: ResourcePath,
}

impl<C> ArchiveEndpointConnector<C>
where
    C: EndpointConnector + Send + Sync,
{
    /// 创建新的归档连接器
    pub fn new(inner: C, archive_path: ResourcePath) -> Self {
        Self {
            inner,
            archive_path,
        }
    }

    /// 获取内部连接器
    pub fn inner(&self) -> &C {
        &self.inner
    }

    /// 获取归档文件路径
    pub fn archive_path(&self) -> &ResourcePath {
        &self.archive_path
    }

    /// 打开归档文件并检测类型
    async fn open_archive(&self) -> Result<(ArchiveKind, Box<dyn AsyncRead + Send + Unpin>), DomainError> {
        let reader = self.inner.read(&self.archive_path).await?;
        let reader = Box::new(reader);

        // 简单的归档类型检测
        let path_str = self.archive_path.as_str().to_lowercase();

        let kind = if path_str.ends_with(".tar.gz") || path_str.ends_with(".tgz") {
            ArchiveKind::TarGz
        } else if path_str.ends_with(".tar") {
            ArchiveKind::Tar
        } else if path_str.ends_with(".gz") {
            ArchiveKind::Gz
        } else if path_str.ends_with(".zip") {
            ArchiveKind::Zip
        } else {
            return Err(DomainError::InvalidResourceIdentifier(format!(
                "Unsupported archive format: {}",
                self.archive_path.as_str()
            )));
        };

        Ok((kind, reader))
    }
}

#[async_trait]
impl<C> EndpointConnector for ArchiveEndpointConnector<C>
where
    C: EndpointConnector + Send + Sync,
{
    /// 获取归档元数据
    ///
    /// 返回归档文件本身的元数据。
    async fn metadata(&self, path: &ResourcePath) -> Result<ResourceMetadata, DomainError> {
        // 如果路径为空或为根，返回归档本身的元数据
        if path.as_str() == "/" || path.as_str().is_empty() {
            return self.inner.metadata(&self.archive_path).await;
        }

        // 获取归档内的条目元数据
        let entry_path = path.as_str().trim_start_matches('/');
        let list_result = self.list(&ResourcePath::new("/")).await?;

        list_result
            .into_iter()
            .find(|m| m.name == entry_path || m.name == entry_path.trim_start_matches('/'))
            .ok_or_else(|| DomainError::ResourceNotFound(format!("Entry not found: {}", entry_path)))
    }

    /// 列出归档内容
    ///
    /// 返回归档内所有条目的元数据列表。
    async fn list(&self, path: &ResourcePath) -> Result<Vec<ResourceMetadata>, DomainError> {
        let _ = path; // 当前忽略路径参数，总是列出根级别内容

        let (kind, reader) = self.open_archive().await?;

        match kind {
            ArchiveKind::Tar | ArchiveKind::TarGz => {
                self.list_tar(reader, kind == ArchiveKind::TarGz).await
            }
            ArchiveKind::Zip => {
                self.list_zip(reader).await
            }
            ArchiveKind::Gz => {
                // 纯 gzip 文件没有目录结构
                Err(DomainError::InvalidResourceIdentifier(
                    "Gzip files have no directory structure".to_string()
                ))
            }
        }
    }

    /// 读取归档内的文件
    async fn read(&self, path: &ResourcePath) -> Result<Pin<Box<dyn AsyncRead + Send + Unpin>>, DomainError> {
        let entry_path = path.as_str().trim_start_matches('/');
        let (kind, reader) = self.open_archive().await?;

        match kind {
            ArchiveKind::Tar | ArchiveKind::TarGz => {
                self.read_tar_entry(reader, entry_path, kind == ArchiveKind::TarGz).await
            }
            ArchiveKind::Zip => {
                self.read_zip_entry(reader, entry_path).await
            }
            ArchiveKind::Gz => {
                // 纯 gzip 文件直接返回解压流
                self.read_gz(reader).await
            }
        }
    }

    /// 检查归档是否存在
    async fn exists(&self, path: &ResourcePath) -> Result<bool, DomainError> {
        // 检查归档文件本身是否存在
        if path.as_str() == "/" || path.as_str().is_empty() {
            return self.inner.exists(&self.archive_path).await;
        }

        // 检查归档内的条目是否存在
        match self.metadata(path).await {
            Ok(_) => Ok(true),
            Err(DomainError::ResourceNotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

impl<C> ArchiveEndpointConnector<C>
where
    C: EndpointConnector + Send + Sync,
{
    /// 列出 tar 归档内容
    async fn list_tar(
        &self,
        reader: Box<dyn AsyncRead + Send + Unpin>,
        is_gz: bool,
    ) -> Result<Vec<ResourceMetadata>, DomainError> {
        use async_compression::tokio::bufread::GzipDecoder;
        use async_tar::Archive;

        let mut result = Vec::new();

        if is_gz {
            let buf_reader = BufReader::new(reader);
            let gz = GzipDecoder::new(buf_reader);
            let archive = Archive::new(gz.compat());
            let mut entries = archive.entries().map_err(|e| DomainError::ResourceNotFound(e.to_string()))?;

            while let Some(entry) = entries.next().await {
                match entry {
                    Ok(entry) => {
                        let path = entry.path().map_err(|e| DomainError::ResourceNotFound(e.to_string()))?;
                        let path_str = path.to_string_lossy().to_string();

                        let header = entry.header();
                        let size = header.size().ok().unwrap_or(0);
                        let is_dir = path_str.ends_with('/');

                        result.push(ResourceMetadata {
                            name: path_str,
                            size,
                            is_dir,
                            modified: header.mtime().ok().map(|t| t as i64),
                            mime_type: None,
                            is_archive: false,
                            child_count: None,
                        });
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read tar entry: {}", e);
                        continue;
                    }
                }
            }
        } else {
            let buf_reader = BufReader::new(reader);
            let archive = Archive::new(buf_reader.compat());
            let mut entries = archive.entries().map_err(|e| DomainError::ResourceNotFound(e.to_string()))?;

            while let Some(entry) = entries.next().await {
                match entry {
                    Ok(entry) => {
                        let path = entry.path().map_err(|e| DomainError::ResourceNotFound(e.to_string()))?;
                        let path_str = path.to_string_lossy().to_string();

                        let header = entry.header();
                        let size = header.size().ok().unwrap_or(0);
                        let is_dir = path_str.ends_with('/');

                        result.push(ResourceMetadata {
                            name: path_str,
                            size,
                            is_dir,
                            modified: header.mtime().ok().map(|t| t as i64),
                            mime_type: None,
                            is_archive: false,
                            child_count: None,
                        });
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read tar entry: {}", e);
                        continue;
                    }
                }
            }
        }

        Ok(result)
    }

    /// 列出 zip 归档内容（简化版本 - 实际需要完整实现）
    async fn list_zip(
        &self,
        _reader: Box<dyn AsyncRead + Send + Unpin>,
    ) -> Result<Vec<ResourceMetadata>, DomainError> {
        // ZIP 处理较复杂，暂时返回空列表
        // 完整实现需要将整个 ZIP 读入内存并使用 async_zip
        tracing::warn!("ZIP listing not fully implemented yet");
        Ok(Vec::new())
    }

    /// 从 tar 归档读取指定条目
    async fn read_tar_entry(
        &self,
        reader: Box<dyn AsyncRead + Send + Unpin>,
        entry_path: &str,
        is_gz: bool,
    ) -> Result<Pin<Box<dyn AsyncRead + Send + Unpin>>, DomainError> {
        use async_compression::tokio::bufread::GzipDecoder;
        use async_tar::Archive;

        if is_gz {
            let buf_reader = BufReader::new(reader);
            let gz = GzipDecoder::new(buf_reader);
            let archive = Archive::new(gz.compat());
            let mut entries = archive.entries().map_err(|e| DomainError::ResourceNotFound(e.to_string()))?;

            while let Some(entry) = entries.next().await {
                match entry {
                    Ok(entry) => {
                        let path = entry.path().map_err(|e| DomainError::ResourceNotFound(e.to_string()))?;
                        let path_str = path.to_string_lossy().to_string();

                        if path_str == entry_path || path_str == entry_path.trim_start_matches('/') {
                            let reader = entry.compat();
                            return Ok(Box::pin(reader));
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read tar entry: {}", e);
                        continue;
                    }
                }
            }
        } else {
            let buf_reader = BufReader::new(reader);
            let archive = Archive::new(buf_reader.compat());
            let mut entries = archive.entries().map_err(|e| DomainError::ResourceNotFound(e.to_string()))?;

            while let Some(entry) = entries.next().await {
                match entry {
                    Ok(entry) => {
                        let path = entry.path().map_err(|e| DomainError::ResourceNotFound(e.to_string()))?;
                        let path_str = path.to_string_lossy().to_string();

                        if path_str == entry_path || path_str == entry_path.trim_start_matches('/') {
                            let reader = entry.compat();
                            return Ok(Box::pin(reader));
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read tar entry: {}", e);
                        continue;
                    }
                }
            }
        }

        Err(DomainError::ResourceNotFound(format!(
            "Entry not found in archive: {}",
            entry_path
        )))
    }

    /// 从 zip 归档读取指定条目（简化版本 - 实际需要完整实现）
    async fn read_zip_entry(
        &self,
        _reader: Box<dyn AsyncRead + Send + Unpin>,
        entry_path: &str,
    ) -> Result<Pin<Box<dyn AsyncRead + Send + Unpin>>, DomainError> {
        // ZIP 处理较复杂，暂时返回错误
        Err(DomainError::ResourceNotFound(format!(
            "ZIP entry reading not fully implemented yet: {}",
            entry_path
        )))
    }

    /// 读取纯 gzip 文件
    async fn read_gz(&self, reader: Box<dyn AsyncRead + Send + Unpin>) -> Result<Pin<Box<dyn AsyncRead + Send + Unpin>>, DomainError> {
        use async_compression::tokio::bufread::GzipDecoder;

        let buf_reader = BufReader::new(reader);
        let decoder = GzipDecoder::new(buf_reader);

        Ok(Box::pin(decoder))
    }
}

/// 归档类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Tar,
    TarGz,
    Gz,
    Zip,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::LocalEndpointConnector;

    #[test]
    fn test_archive_connector_creation() {
        let local = LocalEndpointConnector::new("/tmp".to_string());
        let archive = ArchiveEndpointConnector::new(local, ResourcePath::new("/tmp/test.tar.gz"));

        assert_eq!(archive.archive_path().as_str(), "/tmp/test.tar.gz");
    }

    #[test]
    fn test_archive_kind_detection() {
        // 测试归档类型检测逻辑
        let test_cases = vec![
            ("/data/logs.tar.gz", ArchiveKind::TarGz),
            ("/data/logs.tgz", ArchiveKind::TarGz),
            ("/data/logs.tar", ArchiveKind::Tar),
            ("/data/data.gz", ArchiveKind::Gz),
            ("/data/data.zip", ArchiveKind::Zip),
        ];

        for (path, expected) in test_cases {
            let path_lower = path.to_lowercase();
            let kind = if path_lower.ends_with(".tar.gz") || path_lower.ends_with(".tgz") {
                ArchiveKind::TarGz
            } else if path_lower.ends_with(".tar") {
                ArchiveKind::Tar
            } else if path_lower.ends_with(".gz") {
                ArchiveKind::Gz
            } else if path_lower.ends_with(".zip") {
                ArchiveKind::Zip
            } else {
                panic!("Unknown archive type");
            };

            assert_eq!(kind, expected, "Path: {}", path);
        }
    }
}
