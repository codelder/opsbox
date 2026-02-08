//! LocalFileSystem 模块 - 本地文件系统实现
//!
//! 使用 tokio::fs 实现本地文件系统操作

use std::path::PathBuf;
use std::pin::Pin;

use async_trait::async_trait;
use tokio::fs;

use super::super::{
  filesystem::{DirEntry, FileMetadata, FsError, OpbxFileSystem},
  path::ResourcePath,
};

/// 本地文件系统实现
///
/// 通过 root 目录限定访问范围，所有路径都相对于 root 解析
#[derive(Debug, Clone)]
pub struct LocalFileSystem {
  root: PathBuf,
}

impl LocalFileSystem {
  /// 创建新的本地文件系统实例
  pub fn new(root: PathBuf) -> Result<Self, FsError> {
    if !root.exists() {
      return Err(FsError::NotFound(format!(
        "Root path does not exist: {}",
        root.display()
      )));
    }
    if !root.is_dir() {
      return Err(FsError::InvalidConfig(format!(
        "Root path is not a directory: {}",
        root.display()
      )));
    }
    Ok(Self { root })
  }

  /// 将 ResourcePath 解析为绝对路径
  fn resolve_path(&self, path: &ResourcePath) -> PathBuf {
    let mut resolved = self.root.clone();
    for segment in path.segments() {
      resolved.push(segment);
    }
    resolved
  }
}

#[async_trait]
impl OpbxFileSystem for LocalFileSystem {
  /// 获取文件/目录元数据
  async fn metadata(&self, path: &ResourcePath) -> Result<FileMetadata, FsError> {
    let full_path = self.resolve_path(path);
    let meta = fs::metadata(&full_path)
      .await
      .map_err(|e| FsError::NotFound(format!("{}: {}", full_path.display(), e)))?;

    Ok(FileMetadata {
      is_dir: meta.is_dir(),
      is_file: meta.is_file(),
      size: meta.len(),
      modified: meta.modified().ok(),
      created: meta.created().ok(),
    })
  }

  /// 读取目录内容
  async fn read_dir(&self, path: &ResourcePath) -> Result<Vec<DirEntry>, FsError> {
    let full_path = self.resolve_path(path);
    let mut entries = Vec::new();

    let mut dir_entry = fs::read_dir(&full_path)
      .await
      .map_err(|e| FsError::NotFound(format!("{}: {}", full_path.display(), e)))?;

    while let Some(entry) = dir_entry.next_entry().await.map_err(|e| FsError::Io(e))? {
      let name = entry.file_name().to_string_lossy().to_string();
      let metadata = entry.metadata().await.map_err(FsError::Io)?;

      let file_meta = FileMetadata {
        is_dir: metadata.is_dir(),
        is_file: metadata.is_file(),
        size: metadata.len(),
        modified: metadata.modified().ok(),
        created: metadata.created().ok(),
      };

      // 创建子路径
      let mut segments = path.segments().to_vec();
      segments.push(name.clone());
      let entry_path = ResourcePath::new(segments, path.is_absolute());

      entries.push(DirEntry {
        name,
        path: entry_path,
        metadata: file_meta,
      });
    }

    Ok(entries)
  }

  /// 打开文件用于读取
  async fn open_read(&self, path: &ResourcePath) -> Result<Pin<Box<dyn tokio::io::AsyncRead + Send + Unpin>>, FsError> {
    let full_path = self.resolve_path(path);
    let file = fs::File::open(&full_path)
      .await
      .map_err(|e| FsError::NotFound(format!("{}: {}", full_path.display(), e)))?;

    Ok(Box::pin(file))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs as std_fs;
  use tempfile::TempDir;

  #[tokio::test]
  async fn test_local_fs_new() {
    let temp_dir = TempDir::new().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path().to_path_buf());
    assert!(fs.is_ok());
  }

  #[tokio::test]
  async fn test_local_fs_new_not_found() {
    let fs = LocalFileSystem::new(PathBuf::from("/nonexistent/path"));
    assert!(matches!(fs.unwrap_err(), FsError::NotFound(_)));
  }

  #[tokio::test]
  async fn test_local_fs_new_not_directory() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    std_fs::write(&file_path, "test").unwrap();

    let fs = LocalFileSystem::new(file_path);
    assert!(matches!(fs.unwrap_err(), FsError::InvalidConfig(_)));
  }

  #[tokio::test]
  async fn test_local_fs_metadata_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    std_fs::write(&file_path, "hello world").unwrap();

    let fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
    let path = ResourcePath::from_str("test.txt");
    let metadata = fs.metadata(&path).await.unwrap();

    assert!(metadata.is_file);
    assert!(!metadata.is_dir);
    assert_eq!(metadata.size, 11);
  }

  #[tokio::test]
  async fn test_local_fs_metadata_dir() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path().join("subdir");
    std_fs::create_dir(&dir_path).unwrap();

    let fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
    let path = ResourcePath::from_str("subdir");
    let metadata = fs.metadata(&path).await.unwrap();

    assert!(metadata.is_dir);
    assert!(!metadata.is_file);
  }

  #[tokio::test]
  async fn test_local_fs_metadata_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
    let path = ResourcePath::from_str("nonexistent.txt");
    let result = fs.metadata(&path).await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_local_fs_read_dir() {
    let temp_dir = TempDir::new().unwrap();
    std_fs::write(temp_dir.path().join("file1.txt"), "content1").unwrap();
    std_fs::write(temp_dir.path().join("file2.txt"), "content2").unwrap();
    std_fs::create_dir(temp_dir.path().join("subdir")).unwrap();

    let fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
    let path = ResourcePath::from_str("");
    let entries = fs.read_dir(&path).await.unwrap();

    assert_eq!(entries.len(), 3);
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"file1.txt"));
    assert!(names.contains(&"file2.txt"));
    assert!(names.contains(&"subdir"));
  }

  #[tokio::test]
  async fn test_local_fs_read_dir_nested() {
    let temp_dir = TempDir::new().unwrap();
    let subdir = temp_dir.path().join("subdir");
    std_fs::create_dir(&subdir).unwrap();
    std_fs::write(subdir.join("nested.txt"), "content").unwrap();

    let fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
    let path = ResourcePath::from_str("subdir");
    let entries = fs.read_dir(&path).await.unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "nested.txt");
  }

  #[tokio::test]
  async fn test_local_fs_open_read() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    std_fs::write(&file_path, "hello world").unwrap();

    let fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
    let path = ResourcePath::from_str("test.txt");
    let reader = fs.open_read(&path).await.unwrap();

    // Verify we got a reader
    let _ = reader;
  }

  #[tokio::test]
  async fn test_local_fs_open_read_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();
    let path = ResourcePath::from_str("nonexistent.txt");
    let result = fs.open_read(&path).await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_local_fs_resolve_path() {
    let temp_dir = TempDir::new().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();

    let path = ResourcePath::from_str("subdir/file.txt");
    let resolved = fs.resolve_path(&path);
    assert_eq!(resolved, temp_dir.path().join("subdir/file.txt"));
  }

  #[tokio::test]
  async fn test_local_fs_resolve_path_absolute() {
    let temp_dir = TempDir::new().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).unwrap();

    let path = ResourcePath::from_str("/subdir/file.txt");
    let resolved = fs.resolve_path(&path);
    assert_eq!(resolved, temp_dir.path().join("subdir/file.txt"));
  }
}
