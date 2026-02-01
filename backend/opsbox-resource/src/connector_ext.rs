//! EndpointConnector 扩展
//!
//! 为 EndpointConnector trait 添加搜索相关功能。

use std::pin::Pin;

use async_compression::tokio::bufread::GzipDecoder;
use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt, BufReader};

use opsbox_domain::resource::{EndpointConnector, ResourcePath, DomainError};

use super::stream::{EntryStream, EntryMeta, EntrySource, S3EntryStream, ArchiveEntryStream, TarGzEntryStream};
use super::stream::utils::{PrefixedReader, sniff_archive_kind, ArchiveKind};
use super::{LocalEndpointConnector, S3EndpointConnector};
use super::archive::ArchiveEndpointConnector;

/// EndpointConnector 扩展 trait
///
/// 添加搜索相关的方法，支持将端点转换为条目流。
#[async_trait::async_trait]
pub trait EndpointConnectorExt: EndpointConnector {
    /// 将端点路径转换为条目流
    ///
    /// 用于搜索功能的文件流式遍历。
    async fn as_entry_stream(
        &self,
        path: &ResourcePath,
        recursive: bool,
    ) -> Result<Box<dyn EntryStream>, DomainError>;
}

/// 为 LocalEndpointConnector 实现 EndpointConnectorExt
#[async_trait::async_trait]
impl EndpointConnectorExt for LocalEndpointConnector {
    async fn as_entry_stream(
        &self,
        path: &ResourcePath,
        recursive: bool,
    ) -> Result<Box<dyn EntryStream>, DomainError> {
        use crate::stream::FsEntryStream;

        // 将相对路径与 root 路径结合
        let path_str = path.as_str().trim_start_matches('/');
        let full_path = self.root().join(path_str);

        let stream = FsEntryStream::new(full_path, recursive).await
            .map_err(|e| DomainError::ResourceNotFound(format!("Failed to create entry stream: {}", e)))?;
        Ok(Box::new(stream))
    }
}

/// 为 S3EndpointConnector 实现 EndpointConnectorExt
#[async_trait::async_trait]
impl EndpointConnectorExt for S3EndpointConnector {
    async fn as_entry_stream(
        &self,
        path: &ResourcePath,
        _recursive: bool,
    ) -> Result<Box<dyn EntryStream>, DomainError> {
        let prefix = path.as_str().trim_start_matches('/');
        let prefix = if prefix.is_empty() {
            "".to_string()
        } else if prefix.ends_with('/') {
            prefix.to_string()
        } else {
            format!("{}/", prefix)
        };

        // 列出所有对象键
        let mut keys = Vec::new();
        let mut stream = self.inner().client()
            .list_objects_v2()
            .bucket(self.inner().bucket())
            .prefix(&prefix)
            .into_paginator()
            .send();

        while let Some(res) = stream.next().await {
            let page = res.map_err(|e| DomainError::ResourceNotFound(format!("S3 list failed: {}", e)))?;
            for obj in page.contents() {
                if let Some(k) = obj.key() {
                    if k.ends_with('/') {
                        continue; // 跳过目录占位符
                    }
                    keys.push(k.to_string());
                }
            }
        }

        let stream = S3EntryStream::new(
            self.inner().client().clone(),
            self.inner().bucket().to_string(),
            keys,
        );
        Ok(Box::new(stream))
    }
}

/// 为 ArchiveEndpointConnector 实现 EndpointConnectorExt
///
/// 为归档文件创建流式条目遍历。
/// 自动检测归档类型（tar.gz, tar, gzip等）并返回对应的EntryStream。
///
/// 功能完整版：
/// - 读取4KB头部进行magic bytes检测
/// - 支持tar.gz内嵌检测（解压头部检查"ustar"）
/// - URL解码支持
/// - 详细的错误提示
#[async_trait::async_trait]
impl<C: EndpointConnector + Send + Sync> EndpointConnectorExt for ArchiveEndpointConnector<C> {
    async fn as_entry_stream(
        &self,
        path: &ResourcePath,
        _recursive: bool,
    ) -> Result<Box<dyn EntryStream>, DomainError> {
        // 1. URL解码路径（兼容老实现）
        let decoded_path = percent_encoding::percent_decode_str(path.as_str())
            .decode_utf8()
            .map_err(|e| DomainError::InvalidResourceIdentifier(format!("URL解码失败: {}", e)))?
            .into_owned();

        tracing::info!(
            "[ArchiveEndpointConnector] as_entry_stream: path={}, decoded={}",
            path.as_str(),
            decoded_path
        );

        // 2. 读取归档文件
        let archive_reader = self.inner().read(&ResourcePath::new(&decoded_path)).await?;
        let mut archive_reader: Box<dyn AsyncRead + Send + Unpin> =
            unsafe { Pin::into_inner_unchecked(archive_reader) };

        // 3. 读取4KB头部进行类型检测（与老实现一致）
        let mut head = vec![0u8; 4096];
        let mut n = 0;
        while n < head.len() {
            match AsyncReadExt::read(&mut archive_reader.as_mut(), &mut head[n..]).await {
                Ok(0) => break,
                Ok(len) => n += len,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    return Err(DomainError::ResourceNotFound(format!("读取归档头部失败: {}", e)));
                }
            }
        }
        head.truncate(n);

        tracing::debug!(
            "[ArchiveEndpointConnector] 读取头部: {} bytes, path={}",
            head.len(),
            decoded_path
        );

        // 4. 使用magic bytes检测归档类型
        let kind = sniff_archive_kind(&head, Some(&decoded_path));

        tracing::info!(
            "[ArchiveEndpointConnector] 检测归档类型: kind={:?}, path={}",
            kind,
            decoded_path
        );

        // 5. 对于gzip类型，需要进一步检测是否为tar.gz（在移动数据前完成检测）
        let is_tar = if matches!(kind, ArchiveKind::Gzip) {
            let mut gz = GzipDecoder::new(std::io::Cursor::new(&head));
            let mut inner_head = vec![0u8; 512];
            match gz.read_exact(&mut inner_head).await {
                Ok(_) => {
                    // 检查tar header: "ustar" at position 257-262
                    inner_head.len() >= 262 && &inner_head[257..262] == b"ustar"
                }
                Err(_) => {
                    // 如果内部数据太少，尝试通过后缀名判断
                    let lower = decoded_path.to_lowercase();
                    lower.ends_with(".tar.gz") || lower.ends_with(".tgz")
                }
            }
        } else {
            false
        };

        tracing::debug!(
            "[ArchiveEndpointConnector] gzip检测: is_tar={}, kind={:?}",
            is_tar,
            kind
        );

        // 6. 重构流（组合头部和剩余流）- 只创建一次PrefixedReader
        let prefixed = PrefixedReader::new(head, archive_reader);

        match kind {
            ArchiveKind::Tar => {
                // tar格式（无压缩）
                let stream = TarGzEntryStream::new(prefixed, Some(decoded_path))
                    .await
                    .map_err(|e| DomainError::ResourceNotFound(format!("读取tar失败: {}", e)))?;
                Ok(Box::new(stream))
            }
            ArchiveKind::Gzip => {
                if is_tar {
                    // tar.gz格式 - 使用PrefixedReader保留已读取的数据
                    let gz = GzipDecoder::new(BufReader::new(prefixed));

                    ArchiveEntryStream::new_tar_gz(gz, Some(decoded_path))
                        .await
                        .map_err(|e| DomainError::ResourceNotFound(format!("解析tar.gz失败: {}", e)))
                        .map(|s| Box::new(s) as Box<dyn EntryStream>)
                } else {
                    // 纯gzip格式（单文件）
                    let (entry_path, container_path) = if let Some(stem) =
                        std::path::Path::new(&decoded_path).file_stem()
                    {
                        (stem.to_string_lossy().to_string(), Some(decoded_path))
                    } else {
                        ("<gzip>".to_string(), None)
                    };

                    let meta = EntryMeta {
                        path: entry_path,
                        container_path,
                        size: None,
                        is_compressed: true,
                        source: EntrySource::Gz,
                    };

                    let gz = GzipDecoder::new(BufReader::new(prefixed));
                    Ok(Box::new(ArchiveEntryStream::new_gz(meta, Box::new(gz))))
                }
            }
            ArchiveKind::Zip => {
                Err(DomainError::ResourceNotFound(
                    "ZIP归档暂不支持，请解压后使用".to_string(),
                ))
            }
            ArchiveKind::Unknown => {
                // 尝试通过扩展名作为最后的补救
                let lower = decoded_path.to_lowercase();
                if lower.ends_with(".tar") {
                    let stream = TarGzEntryStream::new(prefixed, Some(decoded_path))
                        .await
                        .map_err(|e| DomainError::ResourceNotFound(format!("读取tar失败: {}", e)))?;
                    Ok(Box::new(stream))
                } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
                    // tar.gz文件 - 使用保留的数据
                    let gz = GzipDecoder::new(BufReader::new(prefixed));

                    ArchiveEntryStream::new_tar_gz(gz, Some(decoded_path))
                        .await
                        .map_err(|e| DomainError::ResourceNotFound(format!("解析tar.gz失败: {}", e)))
                        .map(|s| Box::new(s) as Box<dyn EntryStream>)
                } else if lower.ends_with(".gz") {
                    let meta = EntryMeta {
                        path: decoded_path,
                        container_path: None,
                        size: None,
                        is_compressed: true,
                        source: EntrySource::Gz,
                    };
                    let gz = GzipDecoder::new(BufReader::new(prefixed));
                    Ok(Box::new(ArchiveEntryStream::new_gz(meta, Box::new(gz))))
                } else {
                    Err(DomainError::ResourceNotFound(format!(
                        "未知归档格式或不支持的归档: {}",
                        decoded_path
                    )))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    /// 测试 LocalEndpointConnector 的 EntryStream 功能
    #[tokio::test]
    async fn test_local_entry_stream_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        // 创建测试目录和文件
        let test_dir = root.join("test_stream_dir");
        tokio::fs::create_dir(&test_dir).await.unwrap();
        tokio::fs::write(test_dir.join("file1.txt"), "content 1").await.unwrap();
        tokio::fs::write(test_dir.join("file2.log"), "content 2").await.unwrap();

        // 创建子目录（应该被跳过）
        let sub_dir = test_dir.join("subdir");
        tokio::fs::create_dir(&sub_dir).await.unwrap();
        tokio::fs::write(sub_dir.join("file3.txt"), "content 3").await.unwrap();

        let connector = LocalEndpointConnector::new(root.to_str().unwrap().to_string());

        // 测试递归遍历
        let mut stream = connector.as_entry_stream(&ResourcePath::new("test_stream_dir"), true).await.unwrap();

        let mut found_files = Vec::new();
        while let Some((meta, mut reader)) = stream.next_entry().await.unwrap() {
            let mut content = String::new();
            reader.read_to_string(&mut content).await.unwrap();
            found_files.push((meta.path.clone(), content));
        }

        // 应该找到所有文件
        assert_eq!(found_files.len(), 3);
        assert!(found_files.iter().any(|(p, _)| p.contains("file1.txt")));
        assert!(found_files.iter().any(|(p, _)| p.contains("file2.log")));
        assert!(found_files.iter().any(|(p, _)| p.contains("file3.txt")));
    }

    /// 测试 LocalEndpointConnector 的非递归 EntryStream
    #[tokio::test]
    async fn test_local_entry_stream_non_recursive() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        // 创建测试目录和文件
        let test_dir = root.join("test_non_recursive");
        tokio::fs::create_dir(&test_dir).await.unwrap();
        tokio::fs::write(test_dir.join("file1.txt"), "content 1").await.unwrap();

        // 创建子目录（应该被跳过）
        let sub_dir = test_dir.join("subdir");
        tokio::fs::create_dir(&sub_dir).await.unwrap();
        tokio::fs::write(sub_dir.join("file2.txt"), "content 2").await.unwrap();

        let connector = LocalEndpointConnector::new(root.to_str().unwrap().to_string());

        // 测试非递归遍历
        let mut stream = connector.as_entry_stream(&ResourcePath::new("test_non_recursive"), false).await.unwrap();

        let mut found_files = Vec::new();
        while let Some((meta, _)) = stream.next_entry().await.unwrap() {
            found_files.push(meta.path);
        }

        // 应该只找到直接子文件
        assert_eq!(found_files.len(), 1);
        assert!(found_files[0].contains("file1.txt"));
        assert!(!found_files.iter().any(|p| p.contains("file2.txt")));
    }

    /// 测试 LocalEndpointConnector 单文件的 EntryStream
    #[tokio::test]
    async fn test_local_entry_stream_single_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        let test_content = "hello from entry stream";
        tokio::fs::write(root.join("single.txt"), test_content).await.unwrap();

        let connector = LocalEndpointConnector::new(root.to_str().unwrap().to_string());

        // 单文件作为 EntryStream
        let mut stream = connector.as_entry_stream(&ResourcePath::new("single.txt"), false).await.unwrap();

        let result = stream.next_entry().await.unwrap();
        assert!(result.is_some());

        let (meta, mut reader) = result.unwrap();
        assert!(meta.path.contains("single.txt"));

        let mut content = String::new();
        reader.read_to_string(&mut content).await.unwrap();
        assert_eq!(content, test_content);

        // 第二次调用应该返回 None
        let result = stream.next_entry().await.unwrap();
        assert!(result.is_none());
    }
}
