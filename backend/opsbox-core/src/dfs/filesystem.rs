//! Filesystem 模块 - 文件系统抽象
//!
//! 定义了 OpbxFileSystem trait 和相关类型

use crate::error::AppError;
use async_trait::async_trait;
use std::io;
use std::pin::Pin;
use std::time::SystemTime;

use super::path::ResourcePath;

/// 文件系统错误
#[derive(Debug, thiserror::Error)]
pub enum FsError {
  #[error("Resource not found: {0}")]
  NotFound(String),

  #[error("Permission denied: {0}")]
  PermissionDenied(String),

  #[error("Invalid archive format")]
  InvalidArchiveFormat,

  #[error("IO error: {0}")]
  Io(#[from] io::Error),

  #[error("S3 error: {0}")]
  S3(String),

  #[error("Agent error: {0}")]
  Agent(String),

  #[error("Missing config: {0}")]
  MissingConfig(String),

  #[error("Invalid config: {0}")]
  InvalidConfig(String),

  #[error("Config mismatch")]
  ConfigMismatch,
}

impl From<FsError> for AppError {
  fn from(err: FsError) -> Self {
    AppError::internal(err.to_string())
  }
}

/// 文件元数据
#[derive(Debug, Clone)]
pub struct FileMetadata {
  /// 是否为目录
  pub is_dir: bool,
  /// 是否为文件
  pub is_file: bool,
  /// 是否为符号链接
  pub is_symlink: bool,
  /// 文件大小
  pub size: u64,
  /// 修改时间
  pub modified: Option<SystemTime>,
  /// 创建时间
  pub created: Option<SystemTime>,
}

impl FileMetadata {
  /// 创建目录元数据
  pub fn dir(size: u64) -> Self {
    Self {
      is_dir: true,
      is_file: false,
      is_symlink: false,
      size,
      modified: None,
      created: None,
    }
  }

  /// 创建文件元数据
  pub fn file(size: u64) -> Self {
    Self {
      is_dir: false,
      is_file: true,
      is_symlink: false,
      size,
      modified: None,
      created: None,
    }
  }
}

/// 目录条目
#[derive(Debug, Clone)]
pub struct DirEntry {
  /// 名称
  pub name: String,
  /// 路径
  pub path: ResourcePath,
  /// 元数据
  pub metadata: FileMetadata,
}

/// OpbxFileSystem - OpsBox 文件系统核心抽象
///
/// 定义了访问不同存储后端的统一操作
#[async_trait]
pub trait OpbxFileSystem: Send + Sync {
  /// 获取文件/目录元数据
  async fn metadata(&self, path: &ResourcePath) -> Result<FileMetadata, FsError>;

  /// 读取目录内容
  async fn read_dir(&self, path: &ResourcePath) -> Result<Vec<DirEntry>, FsError>;

  /// 打开文件用于读取
  async fn open_read(&self, path: &ResourcePath) -> Result<Pin<Box<dyn tokio::io::AsyncRead + Send + Unpin>>, FsError>;
}

/// 异步读取 trait
///
/// 用于文件读取的抽象
///
/// 统一使用 tokio::io::AsyncRead trait
/// 支持真正的流式读取，同时可以通过 downcasting 检查数据是否在内存中
pub trait AsyncRead: tokio::io::AsyncRead + Send {}

// 为常见的类型自动实现 AsyncRead
impl<T> AsyncRead for T where T: tokio::io::AsyncRead + Send {}

/// 内存数据读取器
///
/// 对于已经在内存中的数据（如 S3 下载、归档解压）
/// 提供零拷贝的字节访问
pub struct MemoryReader {
  data: Vec<u8>,
  pos: usize,
}

impl MemoryReader {
  pub fn new(data: Vec<u8>) -> Self {
    Self { data, pos: 0 }
  }

  /// 获取字节数组（零拷贝）
  pub fn as_bytes(&self) -> &[u8] {
    &self.data
  }

  /// 检查是否已读取完毕
  pub fn is_empty(&self) -> bool {
    self.pos >= self.data.len()
  }
}

impl tokio::io::AsyncRead for MemoryReader {
  fn poll_read(
    mut self: std::pin::Pin<&mut Self>,
    _cx: &mut std::task::Context<'_>,
    buf: &mut tokio::io::ReadBuf<'_>,
  ) -> std::task::Poll<std::io::Result<()>> {
    if self.pos >= self.data.len() {
      // 已读完所有数据，返回 EOF
      return std::task::Poll::Ready(Ok(()));
    }

    let remaining = &self.data[self.pos..];
    let to_copy = std::cmp::min(remaining.len(), buf.remaining());

    buf.put_slice(&remaining[..to_copy]);
    self.pos += to_copy;

    std::task::Poll::Ready(Ok(()))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_fs_error_display() {
    let err = FsError::NotFound("/path/to/file".to_string());
    assert_eq!(err.to_string(), "Resource not found: /path/to/file");
  }

  #[test]
  fn test_fs_error_from_io() {
    let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
    let fs_err: FsError = io_err.into();
    assert!(matches!(fs_err, FsError::Io(_)));
  }

  #[test]
  fn test_file_metadata_dir() {
    let meta = FileMetadata::dir(4096);
    assert!(meta.is_dir);
    assert!(!meta.is_file);
    assert_eq!(meta.size, 4096);
  }

  #[test]
  fn test_file_metadata_file() {
    let meta = FileMetadata::file(1024);
    assert!(!meta.is_dir);
    assert!(meta.is_file);
    assert_eq!(meta.size, 1024);
  }

  #[test]
  fn test_fs_error_to_app_error() {
    let fs_err = FsError::NotFound("/test".to_string());
    let app_err: AppError = fs_err.into();
    assert!(app_err.to_string().contains("Resource not found"));
  }
}
