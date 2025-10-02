// ============================================================================
// Tar.gz 文件存储源
// ============================================================================

use super::{DataSource, FileEntry, FileIterator, FileMetadata, FileReader, StorageError};
use async_compression::futures::bufread::GzipDecoder;
use async_tar::Archive;
use async_trait::async_trait;
use futures::StreamExt;
use futures::io::AsyncReadExt as FuturesAsyncReadExt;
use log::{debug, warn};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::compat::TokioAsyncReadCompatExt;

/// Tar.gz 文件存储源
///
/// 提供对 tar.gz 归档文件的访问，搜索逻辑由 Server 端执行
pub struct TarGzFile {
  /// tar.gz 文件路径
  path: PathBuf,

  /// 缓存的文件列表（文件路径 -> 文件内容）
  file_cache: Arc<RwLock<HashMap<String, Vec<u8>>>>,

  /// 是否已初始化
  initialized: Arc<RwLock<bool>>,
}

impl TarGzFile {
  /// 创建新的 tar.gz 文件存储源
  ///
  /// # 参数
  ///
  /// * `path` - tar.gz 文件路径
  pub fn new(path: PathBuf) -> Self {
    Self {
      path,
      file_cache: Arc::new(RwLock::new(HashMap::new())),
      initialized: Arc::new(RwLock::new(false)),
    }
  }

  /// 初始化并缓存 tar.gz 中的所有文件
  async fn ensure_initialized(&self) -> Result<(), StorageError> {
    let mut initialized = self.initialized.write().await;

    if *initialized {
      return Ok(());
    }

    debug!("初始化 tar.gz 文件: {:?}", self.path);

    // 打开 tar.gz 文件
    let file = tokio::fs::File::open(&self.path).await.map_err(|e| {
      if e.kind() == std::io::ErrorKind::NotFound {
        StorageError::NotFound(self.path.to_string_lossy().to_string())
      } else if e.kind() == std::io::ErrorKind::PermissionDenied {
        StorageError::PermissionDenied(self.path.to_string_lossy().to_string())
      } else {
        StorageError::Io(e)
      }
    })?;

    // 解压并读取归档
    let reader = tokio::io::BufReader::new(file);
    let compat_reader = reader.compat();
    let decoder = GzipDecoder::new(futures::io::BufReader::new(compat_reader));
    let archive = Archive::new(decoder);
    let mut entries = archive.entries().map_err(StorageError::Io)?;

    let mut cache = self.file_cache.write().await;
    let mut count = 0;

    while let Some(entry_result) = entries.next().await {
      let mut entry = match entry_result {
        Ok(e) => e,
        Err(e) => {
          warn!("读取 tar 条目失败: {}", e);
          continue;
        }
      };

      // 获取文件路径
      let path = match entry.path() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => {
          warn!("获取文件路径失败: {}", e);
          continue;
        }
      };

      // 跳过目录
      let header = entry.header();
      if header.entry_type().is_dir() {
        continue;
      }

      // 读取文件内容到内存
      let mut content = Vec::new();
      if let Err(e) = entry.read_to_end(&mut content).await {
        warn!("读取文件内容失败 {}: {}", path, e);
        continue;
      }

      debug!("缓存文件: {} ({} bytes)", path, content.len());
      cache.insert(path, content);
      count += 1;
    }

    *initialized = true;
    debug!("tar.gz 初始化完成，共缓存 {} 个文件", count);

    Ok(())
  }
}

#[async_trait]
impl DataSource for TarGzFile {
  fn source_type(&self) -> &'static str {
    "TarGzFile"
  }

  async fn list_files(&self) -> Result<FileIterator, StorageError> {
    // 确保已初始化
    self.ensure_initialized().await?;

    let cache = self.file_cache.read().await;

    // 创建文件条目列表
    let entries: Vec<Result<FileEntry, StorageError>> = cache
      .iter()
      .map(|(path, content)| {
        Ok(FileEntry {
          path: path.clone(),
          metadata: FileMetadata {
            size: Some(content.len() as u64),
            modified: None,
            content_type: None,
          },
        })
      })
      .collect();

    debug!("列举 tar.gz 文件: {} 个文件", entries.len());

    // 转换为流
    let stream = futures::stream::iter(entries);
    Ok(Box::new(stream))
  }

  async fn open_file(&self, entry: &FileEntry) -> Result<FileReader, StorageError> {
    // 确保已初始化
    self.ensure_initialized().await?;

    let cache = self.file_cache.read().await;

    // 从缓存中获取文件内容
    let content = cache
      .get(&entry.path)
      .ok_or_else(|| StorageError::NotFound(entry.path.clone()))?;

    debug!("打开 tar.gz 中的文件: {} ({} bytes)", entry.path, content.len());

    // 创建内存读取器
    let cursor = Cursor::new(content.clone());
    Ok(Box::new(cursor))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use flate2::Compression;
  use flate2::write::GzEncoder;
  use std::io::Write;
  use tar::Builder;
  use tokio::io::AsyncReadExt;

  /// 创建测试用的 tar.gz 文件
  fn create_test_targz(path: &std::path::Path) -> std::io::Result<()> {
    // 创建 tar 归档
    let tar_data = Vec::new();
    let mut builder = Builder::new(tar_data);

    // 添加文件1
    let file1_data = b"line 1\nline 2\nline 3\n";
    let mut header1 = tar::Header::new_gnu();
    header1.set_path("dir1/file1.txt")?;
    header1.set_size(file1_data.len() as u64);
    header1.set_mode(0o644);
    header1.set_cksum();
    builder.append(&header1, &file1_data[..])?;

    // 添加文件2
    let file2_data = b"error occurred\nwarning here\n";
    let mut header2 = tar::Header::new_gnu();
    header2.set_path("dir2/file2.log")?;
    header2.set_size(file2_data.len() as u64);
    header2.set_mode(0o644);
    header2.set_cksum();
    builder.append(&header2, &file2_data[..])?;

    let tar_data = builder.into_inner()?;

    // 压缩为 gzip
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar_data)?;
    let gzip_data = encoder.finish()?;

    // 写入文件
    std::fs::write(path, gzip_data)?;

    Ok(())
  }

  #[tokio::test]
  async fn test_targz_list_files() {
    let temp_dir = tempfile::tempdir().unwrap();
    let targz_path = temp_dir.path().join("test.tar.gz");

    create_test_targz(&targz_path).unwrap();

    let source = TarGzFile::new(targz_path);
    let mut files = source.list_files().await.unwrap();

    let mut count = 0;
    while let Some(result) = files.next().await {
      assert!(result.is_ok());
      count += 1;
    }

    assert_eq!(count, 2);
  }

  #[tokio::test]
  async fn test_targz_open_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let targz_path = temp_dir.path().join("test.tar.gz");

    create_test_targz(&targz_path).unwrap();

    let source = TarGzFile::new(targz_path);

    // 先列举文件以初始化
    let mut files = source.list_files().await.unwrap();
    let first_entry = files.next().await.unwrap().unwrap();

    // 打开文件
    let mut reader = source.open_file(&first_entry).await.unwrap();

    let mut content = String::new();
    reader.read_to_string(&mut content).await.unwrap();

    assert!(!content.is_empty());
  }

  #[tokio::test]
  async fn test_targz_nonexistent_file() {
    let source = TarGzFile::new(PathBuf::from("/nonexistent.tar.gz"));

    let result = source.list_files().await;
    assert!(result.is_err());
    if let Err(e) = result {
      assert!(matches!(e, StorageError::NotFound(_)));
    }
  }

  #[tokio::test]
  async fn test_targz_open_nonexistent_entry() {
    let temp_dir = tempfile::tempdir().unwrap();
    let targz_path = temp_dir.path().join("test.tar.gz");

    create_test_targz(&targz_path).unwrap();

    let source = TarGzFile::new(targz_path);
    source.ensure_initialized().await.unwrap();

    let entry = FileEntry {
      path: "nonexistent.txt".to_string(),
      metadata: FileMetadata::default(),
    };

    let result = source.open_file(&entry).await;
    assert!(result.is_err());
    if let Err(e) = result {
      assert!(matches!(e, StorageError::NotFound(_)));
    }
  }

  #[tokio::test]
  async fn test_targz_caching() {
    let temp_dir = tempfile::tempdir().unwrap();
    let targz_path = temp_dir.path().join("test.tar.gz");

    create_test_targz(&targz_path).unwrap();

    let source = TarGzFile::new(targz_path);

    // 第一次列举（触发初始化）
    let mut files1 = source.list_files().await.unwrap();
    let mut count1 = 0;
    while files1.next().await.is_some() {
      count1 += 1;
    }

    // 第二次列举（使用缓存）
    let mut files2 = source.list_files().await.unwrap();
    let mut count2 = 0;
    while files2.next().await.is_some() {
      count2 += 1;
    }

    assert_eq!(count1, count2);
    assert_eq!(count1, 2);
  }
}
