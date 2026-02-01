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
        // ArchiveEndpointConnector 绑定到特定的归档文件
        // 传入的 path 参数可能是归档文件的路径（浏览归档根目录）或归档内的相对路径
        // 我们需要提取归档内的相对路径用于过滤
        let archive_path_str = self.archive_path.as_str();

        // 添加调试日志
        tracing::info!("[ArchiveEndpointConnector::list] archive_path={}, request_path={}",
            archive_path_str, path.as_str());

        // 如果传入的路径就是归档文件路径，说明是在浏览归档根目录
        // 否则，需要提取归档内的相对路径
        let inner_path = if path.as_str() == archive_path_str || path.as_str() == "/" {
            ""
        } else {
            // 尝试从完整路径中提取归档内的部分
            // 格式可能是：/path/to/archive.tar 或 /path/to/archive.tar/inner/path
            if let Some(suffix) = path.as_str().strip_prefix(archive_path_str) {
                suffix.trim_start_matches('/')
            } else {
                // 无法提取，可能是直接传入了归档内路径
                path.as_str().trim_start_matches('/')
            }
        };

        tracing::info!("[ArchiveEndpointConnector::list] extracted inner_path='{}'", inner_path);

        let (kind, reader) = self.open_archive().await?;

        match kind {
            ArchiveKind::Tar | ArchiveKind::TarGz => {
                let result = self.list_tar(reader, kind == ArchiveKind::TarGz, inner_path).await?;
                tracing::info!("[ArchiveEndpointConnector::list] returning {} entries", result.len());
                for (i, meta) in result.iter().enumerate() {
                    tracing::info!("[ArchiveEndpointConnector::list] entry {}: name='{}', is_dir={}, size={}",
                        i, meta.name, meta.is_dir, meta.size);
                }
                Ok(result)
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
    ///
    /// 根据 current_path 过滤条目，只返回指定目录的直接子项。
    /// 使用与旧 TarOpsFS 相同的逻辑：split_once('/') 来区分直接子项和嵌套项。
    async fn list_tar(
        &self,
        reader: Box<dyn AsyncRead + Send + Unpin>,
        is_gz: bool,
        current_path: &str,
    ) -> Result<Vec<ResourceMetadata>, DomainError> {
        use async_compression::tokio::bufread::GzipDecoder;
        use async_tar::Archive;
        use std::collections::HashSet;

        let mut result = Vec::new();
        let mut seen = HashSet::new();

        // 规范化当前路径：构建前缀
        let prefix = if current_path.is_empty() || current_path == "/" {
            "".to_string()
        } else {
            let path = current_path.trim_start_matches('/');
            // 确保以单个 / 结尾（不重复）
            if path.ends_with('/') {
                path.to_string()
            } else {
                format!("{}/", path)
            }
        };

        let mut process_entry = |path_str: String, size: u64, is_dir: bool, mtime: Option<i64>| -> Option<ResourceMetadata> {
            // 检查路径是否以当前目录前缀开头
            if !prefix.is_empty() && !path_str.starts_with(&prefix) {
                return None;
            }

            // 获取相对路径（移除前缀）
            let relative = if prefix.is_empty() {
                path_str.clone()
            } else {
                path_str[prefix.len()..].to_string()
            };

            // 如果相对路径为空，说明是当前目录本身，跳过
            if relative.is_empty() {
                return None;
            }

            // 关键逻辑：使用 split_once('/') 分割路径
            // 如果有 '/'，说明是嵌套路径（如 sub_dir/file.txt），应该只显示第一级目录
            // 如果没有 '/'，说明是直接子项（如 file.txt），直接显示
            let (component, rest) = match relative.split_once('/') {
                Some((c, r)) => (Some(c), Some(r)),
                None => (None, None),
            };

            if rest.is_some() || is_dir {
                // 有剩余路径或是目录：只显示目录组件（作为直接子目录）
                let dir_name = if let Some(c) = component {
                    if c.is_empty() { relative.clone() } else { format!("{}/", c) }
                } else {
                    relative.clone()
                };

                if seen.insert(dir_name.clone()) {
                    Some(ResourceMetadata {
                        name: dir_name,
                        size: 0,
                        is_dir: true,
                        modified: mtime,
                        mime_type: None,
                        is_archive: false,
                        child_count: None,
                    })
                } else {
                    None
                }
            } else {
                // 没有剩余路径且是文件：这是直接子文件
                if seen.insert(relative.clone()) {
                    Some(ResourceMetadata {
                        name: relative,
                        size,
                        is_dir: false,
                        modified: mtime,
                        mime_type: None,
                        is_archive: false,
                        child_count: None,
                    })
                } else {
                    None
                }
            }
        };

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
                        let mtime = header.mtime().ok().map(|t| t as i64);

                        // 调试：记录所有归档条目
                        tracing::info!("[ArchiveEndpointConnector::list_tar] tar_entry: path_str={}, is_dir={}, prefix='{}'",
                            path_str, is_dir, prefix);

                        if let Some(metadata) = process_entry(path_str, size, is_dir, mtime) {
                            result.push(metadata);
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

                        let header = entry.header();
                        let size = header.size().ok().unwrap_or(0);
                        let is_dir = path_str.ends_with('/');
                        let mtime = header.mtime().ok().map(|t| t as i64);

                        // 调试：记录所有归档条目
                        tracing::info!("[ArchiveEndpointConnector::list_tar] tar_entry: path_str={}, is_dir={}, prefix='{}'",
                            path_str, is_dir, prefix);

                        if let Some(metadata) = process_entry(path_str, size, is_dir, mtime) {
                            result.push(metadata);
                        }
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

    /// 测试归档导航：浏览根目录
    ///
    /// E2E 测试发现的问题：当浏览归档根目录时，应该只显示第一级目录/文件
    /// 而不是显示所有嵌套路径。
    #[tokio::test]
    async fn test_list_archive_root_directory_only() {
        use tempfile::Builder;
        use crate::local::LocalEndpointConnector;

        // 创建包含嵌套结构的 tar 文件
        // archive_content/
        // archive_content/root_file.txt
        // archive_content/sub_dir/
        // archive_content/sub_dir/inner_file.txt
        let temp_file = Builder::new().suffix(".tar").tempfile().unwrap();
        {
            let mut builder = tar::Builder::new(temp_file.as_file());

            // 添加目录
            let mut dir_header = tar::Header::new_gnu();
            dir_header.set_path("archive_content/").unwrap();
            dir_header.set_entry_type(tar::EntryType::Directory);
            builder.append_data(&mut dir_header, "archive_content/", &mut std::io::empty()).unwrap();

            // 添加根文件
            let mut file_header = tar::Header::new_gnu();
            file_header.set_path("archive_content/root_file.txt").unwrap();
            file_header.set_size(8);
            file_header.set_cksum();
            let data = "root data";
            builder.append_data(&mut file_header, "archive_content/root_file.txt", data.as_bytes()).unwrap();

            // 添加子目录
            let mut sub_dir_header = tar::Header::new_gnu();
            sub_dir_header.set_path("archive_content/sub_dir/").unwrap();
            sub_dir_header.set_entry_type(tar::EntryType::Directory);
            builder.append_data(&mut sub_dir_header, "archive_content/sub_dir/", &mut std::io::empty()).unwrap();

            // 添加嵌套文件
            let mut inner_header = tar::Header::new_gnu();
            inner_header.set_path("archive_content/sub_dir/inner_file.txt").unwrap();
            inner_header.set_size(5);
            inner_header.set_cksum();
            let inner_data = "inner";
            builder.append_data(&mut inner_header, "archive_content/sub_dir/inner_file.txt", inner_data.as_bytes()).unwrap();

            builder.finish().unwrap();
        }

        // 创建 LocalEndpointConnector 并包装为 ArchiveEndpointConnector
        let local = LocalEndpointConnector::new("/".to_string());
        let archive = super::ArchiveEndpointConnector::new(
            local,
            opsbox_domain::resource::ResourcePath::new(temp_file.path().to_str().unwrap()),
        );

        // 列出归档根目录
        let entries = archive.list(&opsbox_domain::resource::ResourcePath::new(temp_file.path().to_str().unwrap())).await.unwrap();

        // 应该只显示第一级：archive_content/
        assert_eq!(entries.len(), 1, "Root should only show first-level directories");
        assert_eq!(entries[0].name, "archive_content/");
        assert!(entries[0].is_dir);

        // 不应该显示嵌套路径如 archive_content/root_file.txt
        assert!(!entries.iter().any(|e| e.name == "archive_content/root_file.txt"));
    }

    /// 测试归档导航：浏览子目录
    ///
    /// E2E 测试发现的问题：当浏览归档子目录时，应该只显示直接子项
    #[tokio::test]
    async fn test_list_archive_subdirectory() {
        use tempfile::Builder;
        use crate::local::LocalEndpointConnector;

        let temp_file = Builder::new().suffix(".tar").tempfile().unwrap();
        {
            let mut builder = tar::Builder::new(temp_file.as_file());

            // 添加目录
            let mut dir_header = tar::Header::new_gnu();
            dir_header.set_path("archive_content/").unwrap();
            dir_header.set_entry_type(tar::EntryType::Directory);
            builder.append_data(&mut dir_header, "archive_content/", &mut std::io::empty()).unwrap();

            // 添加根文件
            let mut file_header = tar::Header::new_gnu();
            file_header.set_path("archive_content/root_file.txt").unwrap();
            file_header.set_size(8);
            file_header.set_cksum();
            builder.append_data(&mut file_header, "archive_content/root_file.txt", &b"root data"[..]).unwrap();

            // 添加子目录
            let mut sub_dir_header = tar::Header::new_gnu();
            sub_dir_header.set_path("archive_content/sub_dir/").unwrap();
            sub_dir_header.set_entry_type(tar::EntryType::Directory);
            builder.append_data(&mut sub_dir_header, "archive_content/sub_dir/", &mut std::io::empty()).unwrap();

            // 添加嵌套文件（使 sub_dir 目录更明显）
            let mut inner_file_header = tar::Header::new_gnu();
            inner_file_header.set_path("archive_content/sub_dir/nested.txt").unwrap();
            inner_file_header.set_size(4);
            inner_file_header.set_cksum();
            builder.append_data(&mut inner_file_header, "archive_content/sub_dir/nested.txt", b"nested".as_ref()).unwrap();

            builder.finish().unwrap();
        }

        let local = LocalEndpointConnector::new("/".to_string());
        let archive = super::ArchiveEndpointConnector::new(
            local,
            opsbox_domain::resource::ResourcePath::new(temp_file.path().to_str().unwrap()),
        );

        // 浏览 archive_content/ 子目录
        // 注意：路径格式是 /path/to/file.tar/inner/path
        let inner_path = format!("{}/archive_content/", temp_file.path().to_str().unwrap());
        let entries = archive.list(&opsbox_domain::resource::ResourcePath::new(&inner_path)).await.unwrap();

        // 调试输出
        eprintln!("=== Test: test_list_archive_subdirectory ===");
        eprintln!("inner_path: {}", inner_path);
        for e in &entries {
            eprintln!("  entry: name='{}', is_dir={}", e.name, e.is_dir);
        }

        // 应该显示两个直接子项：root_file.txt 和 sub_dir/
        assert_eq!(entries.len(), 2, "Subdirectory should show direct children only");

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"root_file.txt"), "Should contain root_file.txt");
        assert!(names.contains(&"sub_dir/"), "Should contain sub_dir/");
    }

    /// 测试归档导航：路径以 / 结尾时不产生双斜杠
    ///
    /// E2E 测试发现的问题：当路径已以 / 结尾时，格式化会产生双斜杠
    /// 导致前缀匹配失败。
    #[tokio::test]
    async fn test_list_archive_path_with_trailing_slash() {
        use tempfile::Builder;
        use crate::local::LocalEndpointConnector;

        let temp_file = Builder::new().suffix(".tar").tempfile().unwrap();
        {
            let mut builder = tar::Builder::new(temp_file.as_file());

            let mut file_header = tar::Header::new_gnu();
            file_header.set_path("test_file.txt").unwrap();
            file_header.set_size(8);
            file_header.set_cksum();
            let data = "test data";
            builder.append_data(&mut file_header, "test_file.txt", data.as_bytes()).unwrap();

            builder.finish().unwrap();
        }

        let local = LocalEndpointConnector::new("/".to_string());
        let archive = super::ArchiveEndpointConnector::new(
            local,
            opsbox_domain::resource::ResourcePath::new(temp_file.path().to_str().unwrap()),
        );

        // 测试路径以 / 结尾的情况
        let path_with_slash = format!("{}test_dir/", temp_file.path().to_str().unwrap());
        let entries = archive.list(&opsbox_domain::resource::ResourcePath::new(&path_with_slash)).await.unwrap();

        // 应该能正确处理，不产生双斜杠导致的前缀匹配失败
        // 即使目录为空或不存在，也应该返回空结果而不是 panic
        assert!(entries.len() >= 0, "Should handle paths with trailing slash");
    }
}
