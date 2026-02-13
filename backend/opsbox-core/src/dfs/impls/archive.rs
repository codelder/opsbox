//! ArchiveFileSystem 模块 - 归档文件系统包装器
//!
//! 将归档文件视为虚拟文件系统，支持访问归档内的文件

use async_trait::async_trait;
use std::pin::Pin;
use async_compression::tokio::bufread::GzipDecoder;
use futures_lite::io::AsyncReadExt as FuturesAsyncReadExt;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, BufReader};
use tokio_stream::StreamExt;
use tokio_tar::Archive;

use super::super::{
    archive::ArchiveType,
    filesystem::{DirEntry, FileMetadata, FsError, MemoryReader, OpbxFileSystem},
    path::ResourcePath,
    searchable::{SearchConfig, Streamable},
};
use crate::fs::EntryStream;

/// 归档文件系统
///
/// 包装底层文件系统，提供对归档内文件的访问
#[derive(Clone)]
pub struct ArchiveFileSystem<F> {
    inner: F,
    archive_type: ArchiveType,
    // 临时文件路径（用于将归档内容下载到本地）
    temp_path: Option<PathBuf>,
    // 临时文件句柄（用于 RAII）
    _temp_file: Option<Arc<NamedTempFile>>,
}

/// 归档内的条目信息
#[derive(Debug, Clone)]
struct ArchiveEntry {
    /// 归档内的完整路径
    path: String,
    /// 文件大小
    size: u64,
    /// 是否为目录
    is_dir: bool,
}

impl<F> std::fmt::Debug for ArchiveFileSystem<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArchiveFileSystem")
            .field("archive_type", &self.archive_type)
            .field("temp_path", &self.temp_path)
            .finish()
    }
}

impl<F> ArchiveFileSystem<F>
where
    F: OpbxFileSystem,
{
    /// 创建新的归档文件系统
    pub fn new(inner: F, archive_type: ArchiveType) -> Self {
        Self {
            inner,
            archive_type,
            temp_path: None,
            _temp_file: None,
        }
    }

    /// 使用指定路径创建归档文件系统（不拥有文件，用于本地文件）
    pub fn with_path(inner: F, archive_type: ArchiveType, temp_path: PathBuf) -> Self {
        Self {
            inner,
            archive_type,
            temp_path: Some(temp_path),
            _temp_file: None,
        }
    }

    /// 使用临时文件创建归档文件系统
    pub fn with_temp_file(inner: F, archive_type: ArchiveType, temp_path: PathBuf, temp_file: NamedTempFile) -> Self {
        Self {
            inner,
            archive_type,
            temp_path: Some(temp_path),
            _temp_file: Some(Arc::new(temp_file)),
        }
    }

    /// 扫描归档中的所有条目
    async fn scan_all_entries(&self) -> Result<Vec<ArchiveEntry>, FsError> {
        match self.archive_type {
            ArchiveType::Unknown => {
                Err(FsError::InvalidConfig("Unknown archive type".to_string()))
            }
            ArchiveType::Tar => {
                let mut archive = self.new_tar_archive().await?;
                self.collect_tar_entries(&mut archive).await
            }
            ArchiveType::TarGz | ArchiveType::Tgz => {
                // 使用同步 flate2 以获得更好的性能（类似 tar tvf）
                self.collect_tar_gz_entries_blocking().await
            }
            ArchiveType::Zip => {
                let reader = self.new_zip_reader().await?;
                self.collect_zip_entries(&reader).await
            }
            ArchiveType::Gz => {
                // Gzip 文件只有一个虚拟条目
                let path = self.archive_path()?;
                let file_name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("content")
                    .to_string();
                Ok(vec![ArchiveEntry {
                    path: file_name,
                    size: 0,
                    is_dir: false,
                }])
            }
        }
    }

    /// 从 tar 归档中收集所有条目
    ///
    /// 对于 tar.gz 文件，使用 spawn_blocking + 同步 flate2 以获得更好的性能
    async fn collect_tar_entries<R>(&self, archive: &mut Archive<R>) -> Result<Vec<ArchiveEntry>, FsError>
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        use std::time::Instant;

        let start = Instant::now();
        tracing::debug!("collect_tar_entries: starting to scan archive");

        let mut entries = archive
            .entries()
            .map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entries: {}", e)))?;

        let mut result = Vec::new();
        let mut count = 0;
        let start_loop = Instant::now();

        while let Some(entry) = entries.next().await {
            let entry = entry.map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entry: {}", e)))?;
            let entry_path_ref = entry.path()
                .map_err(|e| FsError::InvalidConfig(format!("Invalid TAR entry path: {}", e)))?;
            let entry_path = Self::normalize_path(entry_path_ref.as_ref())?;

            let size = entry.header().size().unwrap_or(0);
            let is_dir = entry.header().entry_type().is_dir();

            result.push(ArchiveEntry {
                path: entry_path,
                size,
                is_dir,
            });

            count += 1;
            if count % 5000 == 0 {
                let elapsed = start_loop.elapsed();
                tracing::debug!("collect_tar_entries: processed {} entries in {:?}", count, elapsed);
            }
        }

        let total_elapsed = start.elapsed();
        tracing::debug!("collect_tar_entries: completed, collected {} entries in {:?}", count, total_elapsed);

        Ok(result)
    }

    /// 从 tar.gz 归档中收集所有条目（使用同步 flate2 以获得更好性能）
    async fn collect_tar_gz_entries_blocking(&self) -> Result<Vec<ArchiveEntry>, FsError> {
        use std::time::Instant;

        let path = self.archive_path()?.to_path_buf();

        tokio::task::spawn_blocking(move || {
            let start = Instant::now();
            tracing::debug!("collect_tar_gz_entries_blocking: starting to scan tar.gz");

            use flate2::read::GzDecoder;
            use std::fs::File;
            use std::io::BufReader;

            let file = File::open(&path)
                .map_err(|e| FsError::Io(io::Error::new(io::ErrorKind::NotFound, e.to_string())))?;

            let reader = BufReader::with_capacity(64 * 1024, file); // 64KB 缓冲区
            let decoder = GzDecoder::new(reader);
            let mut archive = tar::Archive::new(decoder);

            let mut result = Vec::new();
            let mut count = 0;
            let start_loop = Instant::now();

            for entry in archive.entries().map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entries: {}", e)))? {
                let entry = entry.map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entry: {}", e)))?;
                let entry_path_ref = entry.path()
                    .map_err(|e| FsError::InvalidConfig(format!("Invalid TAR entry path: {}", e)))?;
                let entry_path = Self::normalize_path(entry_path_ref.as_ref())?;

                let size = entry.header().size().unwrap_or(0);
                let is_dir = entry.header().entry_type().is_dir();

                result.push(ArchiveEntry {
                    path: entry_path,
                    size,
                    is_dir,
                });

                count += 1;
                if count % 5000 == 0 {
                    let elapsed = start_loop.elapsed();
                    tracing::debug!("collect_tar_gz_entries_blocking: processed {} entries in {:?}", count, elapsed);
                }
            }

            let total_elapsed = start.elapsed();
            tracing::debug!("collect_tar_gz_entries_blocking: completed, collected {} entries in {:?}", count, total_elapsed);

            Ok(result)
        })
        .await
        .map_err(|e| FsError::InvalidConfig(format!("Failed to join blocking task: {}", e)))?
    }

    /// 从 zip 归档中收集所有条目
    async fn collect_zip_entries(&self, reader: &async_zip::tokio::read::seek::ZipFileReader<BufReader<File>>) -> Result<Vec<ArchiveEntry>, FsError> {
        let items = reader.file().entries();
        let mut result = Vec::new();

        for entry in items {
            let name = entry
                .filename()
                .as_str()
                .map_err(|_| FsError::InvalidConfig("Invalid ZIP filename".to_string()))?
                .to_string();
            let name = Self::normalize_path(std::path::Path::new(&name))?;

            result.push(ArchiveEntry {
                path: name,
                size: entry.uncompressed_size(),
                is_dir: entry.dir().unwrap_or(false),
            });
        }

        Ok(result)
    }

    /// 获取归档类型
    pub fn archive_type(&self) -> ArchiveType {
        self.archive_type
    }

    /// 获取底层文件系统
    pub fn inner(&self) -> &F {
        &self.inner
    }

    /// 获取归档文件的本地路径
    fn archive_path(&self) -> Result<&PathBuf, FsError> {
        self.temp_path.as_ref()
            .ok_or_else(|| FsError::InvalidConfig("Archive not downloaded to local temp file".to_string()))
    }

    /// 创建 TAR 归档读取器
    async fn new_tar_archive(&self) -> Result<Archive<BufReader<File>>, FsError> {
        let path = self.archive_path()?;
        let file = File::open(path)
            .await
            .map_err(|e| FsError::Io(io::Error::new(io::ErrorKind::NotFound, e.to_string())))?;
        let reader = BufReader::new(file);
        Ok(Archive::new(reader))
    }

    /// 创建 TAR.GZ 归档读取器（流式解压）
    ///
    /// 使用流式解压，避免将整个文件解压到内存
    async fn new_tar_gz_archive(&self) -> Result<Archive<BufReader<GzipDecoder<BufReader<File>>>>, FsError> {
        use async_compression::tokio::bufread::GzipDecoder;

        let path = self.archive_path()?;
        let file = File::open(path)
            .await
            .map_err(|e| FsError::Io(io::Error::new(io::ErrorKind::NotFound, e.to_string())))?;

        // 创建流式 gzip 解码器，直接传递给 Archive
        // tokio-tar 会在读取时按需解压，不需要预先解压整个文件
        let reader = BufReader::new(file);
        let decoder = GzipDecoder::new(reader);
        Ok(Archive::new(BufReader::new(decoder)))
    }

    /// 创建 ZIP 归档读取器
    async fn new_zip_reader(&self) -> Result<async_zip::tokio::read::seek::ZipFileReader<BufReader<File>>, FsError> {
        use async_zip::tokio::read::seek::ZipFileReader;

        let path = self.archive_path()?;
        let file = File::open(path)
            .await
            .map_err(|e| FsError::Io(io::Error::new(io::ErrorKind::NotFound, e.to_string())))?;
        let reader = BufReader::new(file);
        ZipFileReader::with_tokio(reader)
            .await
            .map_err(|e| FsError::InvalidConfig(format!("Failed to create ZIP reader: {}", e)))
    }

    /// 规范化路径（移除前导斜杠和 ./）
    ///
    /// 优化版本：在一次分配中完成所有字符串操作，避免多次分配
    fn normalize_path(path: &std::path::Path) -> Result<String, FsError> {
        Ok(path.to_string_lossy()
            .trim_start_matches("./")
            .trim_start_matches('/')
            .to_string())
    }
}

#[async_trait]
impl<F> OpbxFileSystem for ArchiveFileSystem<F>
where
    F: OpbxFileSystem + Send + Sync,
{
    /// 获取归档内文件的元数据
    async fn metadata(&self, path: &ResourcePath) -> Result<FileMetadata, FsError> {
        let target = Self::normalize_path(std::path::Path::new(path.to_string().as_str()))?;

        match self.archive_type {
            ArchiveType::Unknown => {
                Err(FsError::InvalidConfig("Unknown archive type".to_string()))
            }
            ArchiveType::Tar => {
                let mut archive = self.new_tar_archive().await?;
                let mut entries = archive
                    .entries()
                    .map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entries: {}", e)))?;

                while let Some(entry) = entries.next().await {
                    let entry = entry.map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entry: {}", e)))?;
                    let entry_path_ref = entry
                        .path()
                        .map_err(|e| FsError::InvalidConfig(format!("Invalid TAR entry path: {}", e)))?;
                    let entry_path = Self::normalize_path(entry_path_ref.as_ref())?;

                    if entry_path == target {
                        let size = entry.header().size().unwrap_or(0);
                        let is_dir = entry.header().entry_type().is_dir();

                        return Ok(FileMetadata {
                            is_dir,
                            is_file: !is_dir,
                            is_symlink: false,
                            size,
                            modified: None,
                            created: None,
                        });
                    }
                }

                Err(FsError::NotFound(format!("Entry not found in archive: {}", target)))
            }
            ArchiveType::TarGz | ArchiveType::Tgz => {
                // 对于 tar.gz 和 tgz 文件，需要先解压 gzip
                let mut archive = self.new_tar_gz_archive().await?;
                let mut entries = archive
                    .entries()
                    .map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entries: {}", e)))?;

                while let Some(entry) = entries.next().await {
                    let entry = entry.map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entry: {}", e)))?;
                    let entry_path_ref = entry
                        .path()
                        .map_err(|e| FsError::InvalidConfig(format!("Invalid TAR entry path: {}", e)))?;
                    let entry_path = Self::normalize_path(entry_path_ref.as_ref())?;

                    if entry_path == target {
                        let size = entry.header().size().unwrap_or(0);
                        let is_dir = entry.header().entry_type().is_dir();

                        return Ok(FileMetadata {
                            is_dir,
                            is_file: !is_dir,
                            is_symlink: false,
                            size,
                            modified: None,
                            created: None,
                        });
                    }
                }

                Err(FsError::NotFound(format!("Entry not found in archive: {}", target)))
            }
            ArchiveType::Gz => {
                // Gzip 文件只包含单个文件
                // 接受任何路径（Gz 只有一个内容，所有路径都指向它）
                let path = self.archive_path()?;
                let metadata = File::open(path)
                    .await
                    .map_err(|e| FsError::Io(io::Error::new(io::ErrorKind::NotFound, e.to_string())))?
                    .metadata()
                    .await
                    .map_err(|e| FsError::Io(io::Error::other(e.to_string())))?;

                Ok(FileMetadata {
                    is_dir: false,
                    is_file: true,
                    is_symlink: false,
                    size: metadata.len(),
                    modified: metadata.modified().ok(),
                    created: None,
                })
            }
            ArchiveType::Zip => {
                let reader = self.new_zip_reader().await?;
                let items = reader.file().entries();

                // 1. 精确匹配文件
                if let Some(entry) = items
                    .iter()
                    .find(|e| {
                        e.filename()
                            .as_str()
                            .ok()
                            .and_then(|s| Self::normalize_path(std::path::Path::new(s)).ok())
                            .as_deref() == Some(target.as_str())
                    })
                {
                    return Ok(FileMetadata {
                        is_dir: entry.dir().unwrap_or(false),
                        is_file: !entry.dir().unwrap_or(false),
                        is_symlink: false,
                        size: entry.uncompressed_size(),
                        modified: None,
                        created: None,
                    });
                }

                // 2. 目录匹配（模拟）
                let prefix = format!("{}/", target);
                if items
                    .iter()
                    .any(|e| {
                        e.filename()
                            .as_str()
                            .ok()
                            .and_then(|s| Self::normalize_path(std::path::Path::new(s)).ok())
                            .as_deref()
                            .unwrap_or("")
                            .starts_with(&prefix)
                    })
                {
                    return Ok(FileMetadata::dir(0));
                }

                Err(FsError::NotFound(format!("Entry not found in archive: {}", target)))
            }
        }
    }

    /// 读取归档内的目录
    async fn read_dir(&self, path: &ResourcePath) -> Result<Vec<DirEntry>, FsError> {
        let dir_path = Self::normalize_path(std::path::Path::new(path.to_string().as_str()))?;
        let prefix = if dir_path.is_empty() {
            String::new()
        } else {
            format!("{}/", dir_path)
        };

        match self.archive_type {
            ArchiveType::Unknown => {
                Err(FsError::InvalidConfig("Unknown archive type".to_string()))
            }
            ArchiveType::Tar | ArchiveType::TarGz | ArchiveType::Tgz | ArchiveType::Zip => {
                // 扫描归档中的所有条目
                let entries = self.scan_all_entries().await?;

                let mut result = Vec::new();
                let mut seen = std::collections::HashSet::new();

                for entry in entries {
                    if entry.path.starts_with(&prefix) {
                        let relative = &entry.path[prefix.len()..];

                        if relative.is_empty() {
                            continue;
                        }

                        let (component, rest) = match relative.split_once('/') {
                            Some((c, r)) => (c, Some(r)),
                            None => (relative, None),
                        };

                        // 计算正确的条目路径
                        // 对于归档内的条目，path 应该是基于当前目录的相对路径
                        // 而不是归档内的完整路径
                        let entry_path = if dir_path.is_empty() {
                            format!("/{}", component)
                        } else {
                            format!("/{}/{}", dir_path, component)
                        };

                        // 目录组件
                        if rest.is_some() || entry.is_dir {
                            if seen.insert(component.to_string()) {
                                result.push(DirEntry {
                                    name: component.to_string(),
                                    path: ResourcePath::parse(&entry_path),
                                    metadata: FileMetadata::dir(0),
                                });
                            }
                        } else {
                            // 文件组件
                            result.push(DirEntry {
                                name: component.to_string(),
                                path: ResourcePath::parse(&entry_path),
                                metadata: FileMetadata::file(entry.size),
                            });
                        }
                    }
                }

                Ok(result)
            }
            ArchiveType::Gz => {
                // Gzip 文件只包含单个文件
                let path = self.archive_path()?;
                let file_name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("content");

                if dir_path.is_empty() || dir_path == "/" {
                    // 返回虚拟文件条目，路径为 /<file_name> 而不是 /
                    // 这样 map_entry 可以正确构建 ORL 的 entry 参数
                    let entry_path = ResourcePath::parse(&format!("/{}", file_name));
                    Ok(vec![DirEntry {
                        name: file_name.to_string(),
                        path: entry_path,
                        metadata: FileMetadata::file(0), // Size requires reading the file
                    }])
                } else {
                    Err(FsError::NotFound(format!("Gzip file has no subdirectories: {}", dir_path)))
                }
            }
        }
    }

    /// 打开归档内的文件用于读取
    async fn open_read(
        &self,
        path: &ResourcePath,
    ) -> Result<Pin<Box<dyn tokio::io::AsyncRead + Send + Unpin>>, FsError> {
        let target = Self::normalize_path(std::path::Path::new(path.to_string().as_str()))?;

        match self.archive_type {
            ArchiveType::Unknown => {
                Err(FsError::InvalidConfig("Unknown archive type".to_string()))
            }
            ArchiveType::Tar => {
                let mut archive = self.new_tar_archive().await?;
                let mut entries = archive
                    .entries()
                    .map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entries: {}", e)))?;

                while let Some(entry) = entries.next().await {
                    let mut entry = entry.map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entry: {}", e)))?;
                    let entry_path_ref = entry
                        .path()
                        .map_err(|e| FsError::InvalidConfig(format!("Invalid TAR entry path: {}", e)))?;
                    let entry_path = Self::normalize_path(entry_path_ref.as_ref())?;

                    if entry_path == target {
                        let mut buf = Vec::new();
                        entry
                            .read_to_end(&mut buf)
                            .await
                            .map_err(|e| FsError::Io(io::Error::other(e.to_string())))?;

                        return Ok(Box::pin(MemoryReader::new(buf)));
                    }
                }

                Err(FsError::NotFound(format!("Entry not found in archive: {}", target)))
            }
            ArchiveType::TarGz | ArchiveType::Tgz => {
                // 对于 tar.gz 和 tgz 文件，需要先解压 gzip
                let mut archive = self.new_tar_gz_archive().await?;
                let mut entries = archive
                    .entries()
                    .map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entries: {}", e)))?;

                while let Some(entry) = entries.next().await {
                    let mut entry = entry.map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entry: {}", e)))?;
                    let entry_path_ref = entry
                        .path()
                        .map_err(|e| FsError::InvalidConfig(format!("Invalid TAR entry path: {}", e)))?;
                    let entry_path = Self::normalize_path(entry_path_ref.as_ref())?;

                    if entry_path == target {
                        let mut buf = Vec::new();
                        entry
                            .read_to_end(&mut buf)
                            .await
                            .map_err(|e| FsError::Io(io::Error::other(e.to_string())))?;

                        return Ok(Box::pin(MemoryReader::new(buf)));
                    }
                }

                Err(FsError::NotFound(format!("Entry not found in archive: {}", target)))
            }
            ArchiveType::Zip => {
                let mut reader = self.new_zip_reader().await?;

                if let Some(index) = reader
                    .file()
                    .entries()
                    .iter()
                    .position(|e| {
                        e.filename()
                            .as_str()
                            .ok()
                            .and_then(|s| Self::normalize_path(std::path::Path::new(s)).ok())
                            .as_deref() == Some(target.as_str())
                    })
                {
                    let mut entry_reader = reader
                        .reader_with_entry(index)
                        .await
                        .map_err(|e| FsError::InvalidConfig(format!("Failed to create ZIP entry reader: {}", e)))?;

                    let mut buf = Vec::new();
                    FuturesAsyncReadExt::read_to_end(&mut entry_reader, &mut buf)
                        .await
                        .map_err(|e| FsError::Io(io::Error::other(e.to_string())))?;

                    return Ok(Box::pin(MemoryReader::new(buf)));
                }

                Err(FsError::NotFound(format!("Entry not found in archive: {}", target)))
            }
            ArchiveType::Gz => {
                // Gzip 文件只包含单个文件，任何路径都解压整个文件
                let path = self.archive_path()?;
                let file = File::open(path)
                    .await
                    .map_err(|e| FsError::Io(io::Error::new(io::ErrorKind::NotFound, e.to_string())))?;

                let mut decoder = GzipDecoder::new(BufReader::new(file));
                let mut buf = Vec::new();
                AsyncReadExt::read_to_end(&mut decoder, &mut buf)
                    .await
                    .map_err(|e| FsError::Io(io::Error::other(e.to_string())))?;

                Ok(Box::pin(MemoryReader::new(buf)))
            }
        }
    }
}

#[async_trait]
impl<F> Streamable for ArchiveFileSystem<F>
where
    F: OpbxFileSystem + Send + Sync,
{
    /// 获取条目流用于搜索
    async fn as_entry_stream(
        &self,
        _path: &ResourcePath,
        _recursive: bool,
        _config: &SearchConfig,
    ) -> Result<Box<dyn EntryStream>, FsError> {
        let archive_path = self.archive_path()?.clone();

        match self.archive_type {
            ArchiveType::Tar => {
                let stream = crate::fs::TarEntryStream::new(
                    tokio::fs::File::open(&archive_path).await.map_err(FsError::Io)?,
                    Some(archive_path.to_string_lossy().to_string()),
                )
                .await
                .map_err(|e| FsError::Io(io::Error::other(e)))?;
                Ok(Box::new(stream))
            }
            ArchiveType::TarGz | ArchiveType::Tgz => {
                let stream = crate::fs::TarGzEntryStream::new(
                    tokio::fs::File::open(&archive_path).await.map_err(FsError::Io)?,
                    Some(archive_path.to_string_lossy().to_string()),
                )
                .await
                .map_err(|e| FsError::Io(io::Error::other(e)))?;
                Ok(Box::new(stream))
            }
            ArchiveType::Gz => {
                let stream = crate::fs::GzipEntryStream::new(
                    tokio::fs::File::open(&archive_path).await.map_err(FsError::Io)?,
                    archive_path.to_string_lossy().to_string(),
                    true,
                );
                Ok(Box::new(stream))
            }
            ArchiveType::Zip => {
                // ZIP 暂不支持流式搜索
                Err(FsError::InvalidConfig(
                    "ZIP archive search is not yet supported".to_string(),
                ))
            }
            ArchiveType::Unknown => Err(FsError::InvalidConfig(
                "Unknown archive type for search".to_string(),
            )),
        }
    }

    /// 归档文件系统支持流式搜索
    fn supports_streaming_search(&self) -> bool {
        matches!(self.archive_type, ArchiveType::Tar | ArchiveType::TarGz | ArchiveType::Tgz | ArchiveType::Gz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dfs::LocalFileSystem;
    use tar::Builder;
    use tempfile::TempDir;
    use tokio::fs;
    use async_zip::{Compression, ZipEntryBuilder, tokio::write::ZipFileWriter};

    async fn create_test_tar(dir: &TempDir) -> PathBuf {
        let tar_path = dir.path().join("test.tar");
        let file = fs::File::create(&tar_path).await.unwrap();
        // Convert tokio File to std File using try_into_inner
        let std_file = file.into_std().await;
        let mut builder = Builder::new(std_file);

        // Add a file
        let mut header = tar::Header::new_gnu();
        header.set_path("test.txt").unwrap();
        header.set_size(13);
        header.set_cksum();
        builder
            .append_data(&mut header, "test.txt", b"hello content".as_slice())
            .unwrap();

        // Add a directory entry
        let mut dir_header = tar::Header::new_gnu();
        dir_header.set_path("logs/").unwrap();
        dir_header.set_entry_type(tar::EntryType::Directory);
        dir_header.set_size(0);
        dir_header.set_cksum();
        builder.append_data(&mut dir_header, "logs/", b"".as_slice()).unwrap();

        // Add file in directory
        let mut log_header = tar::Header::new_gnu();
        log_header.set_path("logs/app.log").unwrap();
        log_header.set_size(8);
        log_header.set_cksum();
        builder
            .append_data(&mut log_header, "logs/app.log", b"log data".as_slice())
            .unwrap();

        builder.finish().unwrap();
        tar_path
    }

    async fn create_test_zip(dir: &TempDir) -> PathBuf {
        let zip_path = dir.path().join("test.zip");
        let tokio_file = fs::File::create(&zip_path).await.unwrap();
        let mut writer = ZipFileWriter::with_tokio(tokio_file);

        // Add a file
        let builder = ZipEntryBuilder::new("test.txt".into(), Compression::Stored);
        writer.write_entry_whole(builder, b"hello zip content").await.unwrap();

        // Add a directory entry
        let builder_dir = ZipEntryBuilder::new("logs/".into(), Compression::Stored);
        writer.write_entry_whole(builder_dir, b"").await.unwrap();

        // Add file in directory
        let builder_log = ZipEntryBuilder::new("logs/app.log".into(), Compression::Stored);
        writer.write_entry_whole(builder_log, b"log data zip").await.unwrap();

        writer.close().await.unwrap();
        zip_path
    }

    #[tokio::test]
    async fn test_memory_reader() {
        let reader = MemoryReader::new(vec![1, 2, 3, 4, 5]);
        assert_eq!(reader.as_bytes().len(), 5);
        assert!(!reader.as_bytes().is_empty());
        assert_eq!(reader.as_bytes(), &[1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn test_memory_reader_empty() {
        let reader = MemoryReader::new(vec![]);
        assert_eq!(reader.as_bytes().len(), 0);
        assert!(reader.as_bytes().is_empty());
    }

    #[tokio::test]
    async fn test_tar_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let tar_path = create_test_tar(&temp_dir).await;

        let local_fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
        let archive_fs = ArchiveFileSystem::with_temp_file(
            local_fs,
            ArchiveType::Tar,
            tar_path,
            NamedTempFile::new().unwrap(),
        );

        let meta = archive_fs.metadata(&ResourcePath::parse("/test.txt")).await.unwrap();
        assert!(meta.is_file);
        assert_eq!(meta.size, 13);
    }

    #[tokio::test]
    async fn test_tar_read_dir() {
        let temp_dir = TempDir::new().unwrap();
        let tar_path = create_test_tar(&temp_dir).await;

        let local_fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
        let archive_fs = ArchiveFileSystem::with_temp_file(
            local_fs,
            ArchiveType::Tar,
            tar_path,
            NamedTempFile::new().unwrap(),
        );

        let entries = archive_fs.read_dir(&ResourcePath::parse("/")).await.unwrap();
        assert!(entries.iter().any(|e| e.name == "test.txt"));
        assert!(entries.iter().any(|e| e.name == "logs"));
    }

    #[tokio::test]
    async fn test_tar_open_read() {
        let temp_dir = TempDir::new().unwrap();
        let tar_path = create_test_tar(&temp_dir).await;

        let local_fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
        let archive_fs = ArchiveFileSystem::with_temp_file(
            local_fs,
            ArchiveType::Tar,
            tar_path,
            NamedTempFile::new().unwrap(),
        );

        let mut reader = archive_fs.open_read(&ResourcePath::parse("/test.txt")).await.unwrap();
        use tokio::io::AsyncReadExt;
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer).await.unwrap();
        assert_eq!(buffer, b"hello content");
    }

    #[tokio::test]
    async fn test_zip_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let zip_path = create_test_zip(&temp_dir).await;

        let local_fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
        let archive_fs = ArchiveFileSystem::with_temp_file(
            local_fs,
            ArchiveType::Zip,
            zip_path,
            NamedTempFile::new().unwrap(),
        );

        let meta = archive_fs.metadata(&ResourcePath::parse("/test.txt")).await.unwrap();
        assert!(meta.is_file);
        assert_eq!(meta.size, 17);
    }

    #[tokio::test]
    async fn test_zip_read_dir() {
        let temp_dir = TempDir::new().unwrap();
        let zip_path = create_test_zip(&temp_dir).await;

        let local_fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
        let archive_fs = ArchiveFileSystem::with_temp_file(
            local_fs,
            ArchiveType::Zip,
            zip_path,
            NamedTempFile::new().unwrap(),
        );

        let entries = archive_fs.read_dir(&ResourcePath::parse("/")).await.unwrap();
        assert!(entries.iter().any(|e| e.name == "test.txt"));
        assert!(entries.iter().any(|e| e.name == "logs"));
    }

    #[tokio::test]
    async fn test_zip_open_read() {
        let temp_dir = TempDir::new().unwrap();
        let zip_path = create_test_zip(&temp_dir).await;

        let local_fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
        let archive_fs = ArchiveFileSystem::with_temp_file(
            local_fs,
            ArchiveType::Zip,
            zip_path,
            NamedTempFile::new().unwrap(),
        );

        let mut reader = archive_fs.open_read(&ResourcePath::parse("/test.txt")).await.unwrap();
        use tokio::io::AsyncReadExt;
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer).await.unwrap();
        assert_eq!(buffer, b"hello zip content");
    }

    #[tokio::test]
    async fn test_normalize_path() {
        assert_eq!(ArchiveFileSystem::<LocalFileSystem>::normalize_path(std::path::Path::new("./test.txt")).unwrap(), "test.txt");
        assert_eq!(ArchiveFileSystem::<LocalFileSystem>::normalize_path(std::path::Path::new("/test.txt")).unwrap(), "test.txt");
        assert_eq!(ArchiveFileSystem::<LocalFileSystem>::normalize_path(std::path::Path::new("./dir/test.txt")).unwrap(), "dir/test.txt");
        assert_eq!(ArchiveFileSystem::<LocalFileSystem>::normalize_path(std::path::Path::new("/dir/test.txt")).unwrap(), "dir/test.txt");
        assert_eq!(ArchiveFileSystem::<LocalFileSystem>::normalize_path(std::path::Path::new("test.txt")).unwrap(), "test.txt");
    }

    /// 创建测试用的 tar.gz 文件
    async fn create_test_tar_gz(dir: &TempDir) -> PathBuf {
        let tar_gz_path = dir.path().join("test.tar.gz");
        let tokio_file = fs::File::create(&tar_gz_path).await.unwrap();
        let std_file = tokio_file.into_std().await;

        // 使用 flate2 压缩 tar 数据
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let mut tar_data = Vec::new();
        {
            let mut builder = Builder::new(&mut tar_data);

            // 添加文件
            let mut header = tar::Header::new_gnu();
            header.set_path("test.txt").unwrap();
            header.set_size(12);  // "hello tar.gz" 是 12 字节
            header.set_cksum();
            builder
                .append_data(&mut header, "test.txt", b"hello tar.gz".as_slice())
                .unwrap();

            // 添加目录条目
            let mut dir_header = tar::Header::new_gnu();
            dir_header.set_path("logs/").unwrap();
            dir_header.set_entry_type(tar::EntryType::Directory);
            dir_header.set_size(0);
            dir_header.set_cksum();
            builder.append_data(&mut dir_header, "logs/", b"".as_slice()).unwrap();

            // 添加目录中的文件
            let mut log_header = tar::Header::new_gnu();
            log_header.set_path("logs/app.log").unwrap();
            log_header.set_size(10);
            log_header.set_cksum();
            builder
                .append_data(&mut log_header, "logs/app.log", b"tar gz log".as_slice())
                .unwrap();

            builder.finish().unwrap();
        }

        // 压缩 tar 数据
        let mut encoder = GzEncoder::new(std_file, Compression::default());
        use std::io::Write;
        encoder.write_all(&tar_data).unwrap();
        encoder.finish().unwrap();

        tar_gz_path
    }

    #[tokio::test]
    async fn test_tar_gz_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let tar_gz_path = create_test_tar_gz(&temp_dir).await;

        let local_fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
        let archive_fs = ArchiveFileSystem::with_temp_file(
            local_fs,
            ArchiveType::TarGz,
            tar_gz_path,
            NamedTempFile::new().unwrap(),
        );

        let meta = archive_fs.metadata(&ResourcePath::parse("/test.txt")).await.unwrap();
        assert!(meta.is_file);
        assert_eq!(meta.size, 12);  // "hello tar.gz" 是 12 字节
    }

    #[tokio::test]
    async fn test_tar_gz_read_dir() {
        let temp_dir = TempDir::new().unwrap();
        let tar_gz_path = create_test_tar_gz(&temp_dir).await;

        let local_fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
        let archive_fs = ArchiveFileSystem::with_temp_file(
            local_fs,
            ArchiveType::TarGz,
            tar_gz_path,
            NamedTempFile::new().unwrap(),
        );

        let entries = archive_fs.read_dir(&ResourcePath::parse("/")).await.unwrap();
        assert!(entries.iter().any(|e| e.name == "test.txt"));
        assert!(entries.iter().any(|e| e.name == "logs"));
    }

    #[tokio::test]
    async fn test_tar_gz_open_read() {
        let temp_dir = TempDir::new().unwrap();
        let tar_gz_path = create_test_tar_gz(&temp_dir).await;

        let local_fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
        let archive_fs = ArchiveFileSystem::with_temp_file(
            local_fs,
            ArchiveType::TarGz,
            tar_gz_path,
            NamedTempFile::new().unwrap(),
        );

        let mut reader = archive_fs.open_read(&ResourcePath::parse("/test.txt")).await.unwrap();
        let mut bytes = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut bytes)
            .await
            .unwrap();
        // 检查数据开头是否正确（可能有末尾的 null 填充）
        assert!(bytes.starts_with(b"hello tar.gz"));
    }

    /// 创建包含深层嵌套目录的测试 tar.gz 文件
    /// 用于测试归档根目录列表时 path 字段的正确性
    async fn create_test_nested_tar_gz(dir: &TempDir) -> PathBuf {
        let tar_gz_path = dir.path().join("nested.tar.gz");
        let tokio_file = fs::File::create(&tar_gz_path).await.unwrap();
        let std_file = tokio_file.into_std().await;

        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let mut tar_data = Vec::new();
        {
            let mut builder = Builder::new(&mut tar_data);

            // 创建深层嵌套结构: home/bbipadm/logs/bjbbip-gateway/app_jsonServerMsg.log
            let file_content = b"sample log content";

            let mut header = tar::Header::new_gnu();
            header.set_path("home/bbipadm/logs/bjbbip-gateway/app_jsonServerMsg.log").unwrap();
            header.set_size(file_content.len() as u64);
            header.set_cksum();
            builder
                .append_data(&mut header, "home/bbipadm/logs/bjbbip-gateway/app_jsonServerMsg.log", file_content.as_slice())
                .unwrap();

            // 添加另一个文件在同级目录
            let mut header2 = tar::Header::new_gnu();
            header2.set_path("home/readme.txt").unwrap();
            header2.set_size(5);
            header2.set_cksum();
            builder
                .append_data(&mut header2, "home/readme.txt", b"hello".as_slice())
                .unwrap();

            builder.finish().unwrap();
        }

        // 压缩 tar 数据并写入文件
        let mut encoder = GzEncoder::new(std_file, Compression::default());
        encoder.write_all(&tar_data).unwrap();
        encoder.finish().unwrap();

        tar_gz_path
    }

    /// Bug 测试：验证归档根目录中的目录条目 path 正确性
    ///
    /// 这个测试重现了 bug：当列出归档根目录时，目录条目的 path 应该
    /// 是相对路径（如 "/home"），而不是完整路径（如 "/home/bbipadm/logs/..."）。
    ///
    /// 用户报告的问题：
    /// - 双击归档根目录中的 "home" 目录
    /// - 前端收到的 path 是 "orl://...tar.gz?entry=/home/bbipadm/logs/bjbbip-gateway/app_jsonServerMsg.log"
    /// - 应该是 "orl://...tar.gz?entry=/home"
    #[tokio::test]
    async fn test_nested_tar_gz_dir_entry_path_bug() {
        let temp_dir = TempDir::new().unwrap();
        let tar_gz_path = create_test_nested_tar_gz(&temp_dir).await;

        let local_fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
        let archive_fs = ArchiveFileSystem::with_temp_file(
            local_fs,
            ArchiveType::TarGz,
            tar_gz_path,
            NamedTempFile::new().unwrap(),
        );

        // 列出归档根目录
        let entries = archive_fs.read_dir(&ResourcePath::parse("/")).await.unwrap();

        // 找到 "home" 目录条目
        let home_entry = entries.iter().find(|e| e.name == "home").expect("应该找到 home 目录");

        // Bug 检查：目录的 path 应该是 "/home"，而不是完整路径
        let path_str = home_entry.path.to_string();

        // 当前有 bug：path 是完整路径 "/home/bbipadm/logs/bjbbip-gateway/app_jsonServerMsg.log"
        // 期望：path 应该是 "/home"
        assert_eq!(
            path_str,
            "/home",
            "目录条目的 path 应该是相对路径 '/home'，而不是完整路径。实际: '{}'",
            path_str
        );

        // 验证可以正确进入 home 目录
        let home_entries = archive_fs.read_dir(&ResourcePath::parse("/home")).await.unwrap();
        assert!(home_entries.iter().any(|e| e.name == "bbipadm"), "应该在 home 目录中找到 bbipadm");
        assert!(home_entries.iter().any(|e| e.name == "readme.txt"), "应该在 home 目录中找到 readme.txt");
    }
}
