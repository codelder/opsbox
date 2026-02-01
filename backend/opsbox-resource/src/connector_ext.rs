//! EndpointConnector 扩展
//!
//! 为 EndpointConnector trait 添加搜索相关功能。

use std::pin::Pin;

use async_compression::tokio::bufread::GzipDecoder;
use tokio::io::{AsyncRead, AsyncReadExt, BufReader};

use opsbox_domain::resource::{EndpointConnector, ResourcePath, DomainError};

use super::stream::{EntryStream, EntryMeta, EntrySource, S3EntryStream, ArchiveEntryStream};
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
        // 使用 self.archive_path 读取归档文件，而不是 path 参数
        // path 参数是归档内部的路径（如 /），archive_path 是归档文件本身的路径
        let archive_path_str = self.archive_path().as_str().to_string();

        tracing::info!(
            "[ArchiveEndpointConnector] as_entry_stream: inner_path={}, archive_path={}",
            path.as_str(),
            archive_path_str
        );

        // 2. 读取归档文件
        let archive_reader = self.inner().read(self.archive_path()).await?;
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
            "[ArchiveEndpointConnector] 读取头部: {} bytes, archive_path={}",
            head.len(),
            archive_path_str
        );

        // 4. 使用magic bytes检测归档类型
        let kind = sniff_archive_kind(&head, Some(&archive_path_str));

        tracing::info!(
            "[ArchiveEndpointConnector] 检测归档类型: kind={:?}, archive_path={}",
            kind,
            archive_path_str
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
                    let lower = archive_path_str.to_lowercase();
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
                // tar格式（无压缩）- 使用 ArchiveEntryStream 预读取所有条目
                ArchiveEntryStream::new_tar(prefixed, Some(archive_path_str))
                    .await
                    .map_err(|e| DomainError::ResourceNotFound(format!("读取tar失败: {}", e)))
                    .map(|s| Box::new(s) as Box<dyn EntryStream>)
            }
            ArchiveKind::Gzip => {
                if is_tar {
                    // tar.gz格式 - 使用PrefixedReader保留已读取的数据
                    let gz = GzipDecoder::new(BufReader::new(prefixed));

                    ArchiveEntryStream::new_tar_gz(gz, Some(archive_path_str))
                        .await
                        .map_err(|e| DomainError::ResourceNotFound(format!("解析tar.gz失败: {}", e)))
                        .map(|s| Box::new(s) as Box<dyn EntryStream>)
                } else {
                    // 纯gzip格式（单文件）
                    let (entry_path, container_path) = if let Some(stem) =
                        std::path::Path::new(&archive_path_str).file_stem()
                    {
                        (stem.to_string_lossy().to_string(), Some(archive_path_str))
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
                let lower = archive_path_str.to_lowercase();
                if lower.ends_with(".tar") {
                    // tar文件（无压缩）- 使用 ArchiveEntryStream
                    ArchiveEntryStream::new_tar(prefixed, Some(archive_path_str))
                        .await
                        .map_err(|e| DomainError::ResourceNotFound(format!("读取tar失败: {}", e)))
                        .map(|s| Box::new(s) as Box<dyn EntryStream>)
                } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
                    // tar.gz文件 - 使用保留的数据
                    let gz = GzipDecoder::new(BufReader::new(prefixed));

                    ArchiveEntryStream::new_tar_gz(gz, Some(archive_path_str))
                        .await
                        .map_err(|e| DomainError::ResourceNotFound(format!("解析tar.gz失败: {}", e)))
                        .map(|s| Box::new(s) as Box<dyn EntryStream>)
                } else if lower.ends_with(".gz") {
                    let meta = EntryMeta {
                        path: archive_path_str,
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
                        archive_path_str
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

    /// 测试 ArchiveEndpointConnector 的 as_entry_stream 展开归档功能
    ///
    /// 这个测试验证了关键的 Bug 修复：
    /// - as_entry_stream 应该使用 self.archive_path() 读取归档文件
    /// - 而不是使用 path 参数（path 是归档内的路径，如 "/"）
    #[tokio::test]
    async fn test_archive_entry_stream_expansion() {
        use crate::archive::ArchiveEndpointConnector;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        // 创建一个测试归档文件，包含内部文件
        let archive_name = "test-archive.tar.gz";
        let archive_path = root.join(archive_name);

        // 使用标准库创建 tar.gz 文件
        {
            let tar_gz_file = std::fs::File::create(&archive_path).unwrap();
            let enc = flate2::write::GzEncoder::new(tar_gz_file, flate2::Compression::default());
            let mut tar = tar::Builder::new(enc);

            let temp_file = root.join("temp_test.log");
            std::fs::write(&temp_file, "2024-01-01 INFO Archive test content\n").unwrap();

            tar.append_file("internal/test.log", &mut std::fs::File::open(&temp_file).unwrap())
                .unwrap();

            std::fs::remove_file(&temp_file).unwrap();
        }

        // 创建 LocalEndpointConnector，使用 root 目录
        let connector = LocalEndpointConnector::new(root.to_str().unwrap().to_string());

        // ArchiveEndpointConnector 的 archive_path 是相对于 connector 的 root
        let archive = ArchiveEndpointConnector::new(
            connector,
            ResourcePath::new(&format!("/{}", archive_name)),
        );

        // 关键测试：使用 "/" 作为归档内路径，应该能展开归档并读取到内部文件
        let mut stream = archive.as_entry_stream(&ResourcePath::new("/"), true).await.unwrap();

        // 应该能读取到 internal/test.log
        let result = stream.next_entry().await.unwrap();
        assert!(result.is_some(), "应该能读取到归档内的第一个条目");

        let (meta, mut reader) = result.unwrap();
        assert!(meta.path.contains("test.log"), "路径应该包含 test.log, 实际: {}", meta.path);

        // 验证文件内容
        let mut content = String::new();
        reader.read_to_string(&mut content).await.unwrap();
        assert!(content.contains("Archive test content"), "内容应该包含测试文本");

        // 第二次调用应该返回 None（只有一个文件）
        let result = stream.next_entry().await.unwrap();
        assert!(result.is_none(), "应该只有一个条目");
    }

    /// 测试 ArchiveEndpointConnector 展开包含多个文件的归档
    #[tokio::test]
    async fn test_archive_entry_stream_multiple_files() {
        use crate::archive::ArchiveEndpointConnector;

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        // 创建包含多个文件的归档
        let archive_name = "multi-archive.tar.gz";
        let archive_path = root.join(archive_name);

        // 使用标准库创建 tar.gz 文件
        {
            let tar_gz_file = std::fs::File::create(&archive_path).unwrap();
            let enc = flate2::write::GzEncoder::new(tar_gz_file, flate2::Compression::default());
            let mut tar = tar::Builder::new(enc);

            // 添加多个文件到归档
            let files = vec![
                ("logs/app.log", "2024-01-01 INFO App started\n"),
                ("logs/error.log", "2024-01-01 ERROR An error\n"),
                ("config.json", "{\"key\": \"value\"}\n"),
            ];

            for (path, content) in files {
                let temp_file = root.join(&format!("temp_{}", path.replace("/", "_")));
                std::fs::write(&temp_file, content).unwrap();

                tar.append_file(path, &mut std::fs::File::open(&temp_file).unwrap())
                    .unwrap();

                std::fs::remove_file(&temp_file).unwrap();
            }
        }

        // 创建 LocalEndpointConnector
        let connector = LocalEndpointConnector::new(root.to_str().unwrap().to_string());

        // ArchiveEndpointConnector 的 archive_path 是相对于 connector 的 root
        let archive = ArchiveEndpointConnector::new(
            connector,
            ResourcePath::new(&format!("/{}", archive_name)),
        );

        // 展开归档并验证所有文件
        let mut stream = archive.as_entry_stream(&ResourcePath::new("/"), true).await.unwrap();

        let mut found_files = Vec::new();
        while let Some((meta, mut reader)) = stream.next_entry().await.unwrap() {
            let mut content = String::new();
            reader.read_to_string(&mut content).await.unwrap();
            found_files.push((meta.path.clone(), content));
        }

        assert_eq!(found_files.len(), 3, "应该找到 3 个文件");
        assert!(found_files.iter().any(|(p, _)| p.contains("app.log")));
        assert!(found_files.iter().any(|(p, _)| p.contains("error.log")));
        assert!(found_files.iter().any(|(p, _)| p.contains("config.json")));
    }
}
