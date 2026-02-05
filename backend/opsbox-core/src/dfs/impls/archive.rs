//! ArchiveFileSystem 模块 - 归档文件系统包装器
//!
//! 将归档文件视为虚拟文件系统，支持访问归档内的文件

use async_trait::async_trait;
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
    filesystem::{DirEntry, FileMetadata, FsError, OpbxFileSystem},
    path::ResourcePath,
};

/// 归档文件系统
///
/// 包装底层文件系统，提供对归档内文件的访问
#[derive(Debug, Clone)]
pub struct ArchiveFileSystem<F> {
    inner: F,
    archive_type: ArchiveType,
    // 临时文件路径（用于将归档内容下载到本地）
    temp_path: Option<PathBuf>,
    // 临时文件句柄（用于 RAII）
    _temp_file: Option<Arc<NamedTempFile>>,
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

    /// 使用临时文件创建归档文件系统
    pub fn with_temp_file(inner: F, archive_type: ArchiveType, temp_path: PathBuf, temp_file: NamedTempFile) -> Self {
        Self {
            inner,
            archive_type,
            temp_path: Some(temp_path),
            _temp_file: Some(Arc::new(temp_file)),
        }
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
    fn normalize_path(path: &str) -> String {
        let mut result = path;
        // 移除前导 ./
        while result.starts_with("./") {
            result = &result[2..];
        }
        // 移除前导 /
        while result.starts_with('/') {
            result = &result[1..];
        }
        result.to_string()
    }
}

#[async_trait]
impl<F> OpbxFileSystem for ArchiveFileSystem<F>
where
    F: OpbxFileSystem + Send + Sync,
{
    /// 获取归档内文件的元数据
    async fn metadata(&self, path: &ResourcePath) -> Result<FileMetadata, FsError> {
        let target = Self::normalize_path(&path.to_string());

        match self.archive_type {
            ArchiveType::Tar | ArchiveType::TarGz | ArchiveType::Tgz => {
                let mut archive = self.new_tar_archive().await?;
                let mut entries = archive
                    .entries()
                    .map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entries: {}", e)))?;

                while let Some(entry) = entries.next().await {
                    let entry = entry.map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entry: {}", e)))?;
                    let entry_path = entry
                        .path()
                        .map_err(|e| FsError::InvalidConfig(format!("Invalid TAR entry path: {}", e)))?
                        .to_string_lossy()
                        .to_string();
                    let entry_path = Self::normalize_path(&entry_path);

                    if entry_path == target {
                        let size = entry.header().size().unwrap_or(0);
                        let is_dir = entry.header().entry_type().is_dir();

                        return Ok(FileMetadata {
                            is_dir,
                            is_file: !is_dir,
                            size,
                            modified: None,
                            created: None,
                        });
                    }
                }

                Err(FsError::NotFound(format!("Entry not found in archive: {}", target)))
            }
            ArchiveType::Gz => {
                // Gzip 文件只包含单个文件，路径应该是根
                if target.is_empty() || target == "/" {
                    let path = self.archive_path()?;
                    let metadata = File::open(path)
                        .await
                        .map_err(|e| FsError::Io(io::Error::new(io::ErrorKind::NotFound, e.to_string())))?
                        .metadata()
                        .await
                        .map_err(|e| FsError::Io(io::Error::new(io::ErrorKind::Other, e.to_string())))?;

                    Ok(FileMetadata {
                        is_dir: false,
                        is_file: true,
                        size: metadata.len(),
                        modified: metadata.modified().ok(),
                        created: None,
                    })
                } else {
                    Err(FsError::NotFound(format!("Gzip file contains only one entry: {}", target)))
                }
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
                            .map(|s| Self::normalize_path(s))
                            .ok()
                            .as_deref() == Some(target.as_str())
                    })
                {
                    return Ok(FileMetadata {
                        is_dir: entry.dir().unwrap_or(false),
                        is_file: !entry.dir().unwrap_or(false),
                        size: entry.uncompressed_size() as u64,
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
                            .map(|s| Self::normalize_path(s))
                            .ok()
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
        let dir_path = Self::normalize_path(&path.to_string());
        let prefix = if dir_path.is_empty() {
            String::new()
        } else {
            format!("{}/", dir_path)
        };

        match self.archive_type {
            ArchiveType::Tar | ArchiveType::TarGz | ArchiveType::Tgz => {
                let mut archive = self.new_tar_archive().await?;
                let mut entries = archive
                    .entries()
                    .map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entries: {}", e)))?;

                let mut result = Vec::new();
                let mut seen = std::collections::HashSet::new();

                while let Some(entry) = entries.next().await {
                    let entry = entry.map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entry: {}", e)))?;
                    let entry_path = entry
                        .path()
                        .map_err(|e| FsError::InvalidConfig(format!("Invalid TAR entry path: {}", e)))?
                        .to_string_lossy()
                        .to_string();
                    let entry_path = Self::normalize_path(&entry_path);

                    if entry_path.starts_with(&prefix) {
                        let relative = &entry_path[prefix.len()..];

                        if relative.is_empty() {
                            continue;
                        }

                        let (component, rest) = match relative.split_once('/') {
                            Some((c, r)) => (c, Some(r)),
                            None => (relative, None),
                        };

                        // 目录组件
                        if rest.is_some() || entry.header().entry_type().is_dir() {
                            if seen.insert(component.to_string()) {
                                result.push(DirEntry {
                                    name: component.to_string(),
                                    path: ResourcePath::from_str(&format!("/{}", entry_path)),
                                    metadata: FileMetadata::dir(0),
                                });
                            }
                        } else {
                            // 文件组件
                            result.push(DirEntry {
                                name: component.to_string(),
                                path: ResourcePath::from_str(&format!("/{}", entry_path)),
                                metadata: FileMetadata::file(entry.header().size().unwrap_or(0)),
                            });
                        }
                    }
                }

                Ok(result)
            }
            ArchiveType::Zip => {
                let reader = self.new_zip_reader().await?;
                let items = reader.file().entries();

                let mut entries = Vec::new();
                let mut seen_dirs = std::collections::HashSet::new();

                for entry in items {
                    let name = entry
                        .filename()
                        .as_str()
                        .map(|s| Self::normalize_path(s))
                        .map_err(|_| FsError::InvalidConfig("Invalid ZIP filename".to_string()))?;

                    if name.starts_with(&prefix) {
                        let relative = &name[prefix.len()..];

                        if relative.is_empty() {
                            continue;
                        }

                        let (component, rest) = match relative.split_once('/') {
                            Some((c, r)) => (c, Some(r)),
                            None => (relative, None),
                        };

                        if rest.is_some() {
                            if seen_dirs.insert(component.to_string()) {
                                entries.push(DirEntry {
                                    name: component.to_string(),
                                    path: ResourcePath::from_str(&format!("/{}", name)),
                                    metadata: FileMetadata::dir(0),
                                });
                            }
                        } else {
                            entries.push(DirEntry {
                                name: component.to_string(),
                                path: ResourcePath::from_str(&format!("/{}", name)),
                                metadata: FileMetadata::file(entry.uncompressed_size() as u64),
                            });
                        }
                    }
                }

                Ok(entries)
            }
            ArchiveType::Gz => {
                // Gzip 文件只包含单个文件
                let path = self.archive_path()?;
                let file_name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("content");

                if dir_path.is_empty() || dir_path == "/" {
                    Ok(vec![DirEntry {
                        name: file_name.to_string(),
                        path: ResourcePath::from_str("/"),
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
    ) -> Result<Box<dyn super::super::filesystem::AsyncRead + Send + Unpin>, FsError> {
        let target = Self::normalize_path(&path.to_string());

        match self.archive_type {
            ArchiveType::Tar | ArchiveType::TarGz | ArchiveType::Tgz => {
                let mut archive = self.new_tar_archive().await?;
                let mut entries = archive
                    .entries()
                    .map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entries: {}", e)))?;

                while let Some(entry) = entries.next().await {
                    let mut entry = entry.map_err(|e| FsError::InvalidConfig(format!("Failed to read TAR entry: {}", e)))?;
                    let entry_path = entry
                        .path()
                        .map_err(|e| FsError::InvalidConfig(format!("Invalid TAR entry path: {}", e)))?
                        .to_string_lossy()
                        .to_string();
                    let entry_path = Self::normalize_path(&entry_path);

                    if entry_path == target {
                        let mut buf = Vec::new();
                        entry
                            .read_to_end(&mut buf)
                            .await
                            .map_err(|e| FsError::Io(io::Error::new(io::ErrorKind::Other, e.to_string())))?;

                        return Ok(Box::new(ArchiveFileReader::new(buf)));
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
                            .map(|s| Self::normalize_path(s))
                            .ok()
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
                        .map_err(|e| FsError::Io(io::Error::new(io::ErrorKind::Other, e.to_string())))?;

                    return Ok(Box::new(ArchiveFileReader::new(buf)));
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
                    .map_err(|e| FsError::Io(io::Error::new(io::ErrorKind::Other, e.to_string())))?;

                Ok(Box::new(ArchiveFileReader::new(buf)))
            }
        }
    }
}

/// 归档文件读取器
///
/// 将归档内文件的内容读入内存
pub struct ArchiveFileReader {
    data: Vec<u8>,
}

impl ArchiveFileReader {
    /// 创建新的归档文件读取器
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// 获取文件内容的字节数组
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// 获取文件内容长度
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// 检查文件是否为空
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl super::super::filesystem::AsyncRead for ArchiveFileReader {
    fn bytes(&self) -> Option<&[u8]> {
        Some(&self.data)
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
    async fn test_archive_file_reader() {
        let reader = ArchiveFileReader::new(vec![1, 2, 3, 4, 5]);
        assert_eq!(reader.len(), 5);
        assert!(!reader.is_empty());
        assert_eq!(reader.bytes(), &[1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn test_archive_file_reader_empty() {
        let reader = ArchiveFileReader::new(vec![]);
        assert_eq!(reader.len(), 0);
        assert!(reader.is_empty());
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

        let meta = archive_fs.metadata(&ResourcePath::from_str("/test.txt")).await.unwrap();
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

        let entries = archive_fs.read_dir(&ResourcePath::from_str("/")).await.unwrap();
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

        let reader = archive_fs.open_read(&ResourcePath::from_str("/test.txt")).await.unwrap();
        assert_eq!(reader.bytes().unwrap(), b"hello content");
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

        let meta = archive_fs.metadata(&ResourcePath::from_str("/test.txt")).await.unwrap();
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

        let entries = archive_fs.read_dir(&ResourcePath::from_str("/")).await.unwrap();
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

        let reader = archive_fs.open_read(&ResourcePath::from_str("/test.txt")).await.unwrap();
        assert_eq!(reader.bytes().unwrap(), b"hello zip content");
    }

    #[tokio::test]
    async fn test_normalize_path() {
        assert_eq!(ArchiveFileSystem::<LocalFileSystem>::normalize_path("./test.txt"), "test.txt");
        assert_eq!(ArchiveFileSystem::<LocalFileSystem>::normalize_path("/test.txt"), "test.txt");
        assert_eq!(ArchiveFileSystem::<LocalFileSystem>::normalize_path("./dir/test.txt"), "dir/test.txt");
        assert_eq!(ArchiveFileSystem::<LocalFileSystem>::normalize_path("/dir/test.txt"), "dir/test.txt");
        assert_eq!(ArchiveFileSystem::<LocalFileSystem>::normalize_path("test.txt"), "test.txt");
    }
}
