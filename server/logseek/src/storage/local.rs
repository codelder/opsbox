// ============================================================================
// 本地文件系统存储源
// ============================================================================

use super::{DataSource, FileEntry, FileIterator, FileMetadata, FileReader, StorageError};
use async_trait::async_trait;
use log::{debug, warn};
use std::path::PathBuf;

/// 本地文件系统存储源
///
/// 提供对本地文件系统的访问，搜索逻辑由 Server 端执行
pub struct LocalFileSystem {
  root_path: PathBuf,
  recursive: bool,
  follow_symlinks: bool,
}

impl LocalFileSystem {
  /// 创建新的本地文件系统存储源
  ///
  /// # 参数
  ///
  /// * `root_path` - 根目录路径
  pub fn new(root_path: PathBuf) -> Self {
    Self {
      root_path,
      recursive: true,
      follow_symlinks: false,
    }
  }

  /// 设置是否递归搜索子目录
  pub fn with_recursive(mut self, recursive: bool) -> Self {
    self.recursive = recursive;
    self
  }

  /// 设置是否跟随符号链接
  pub fn with_follow_symlinks(mut self, follow: bool) -> Self {
    self.follow_symlinks = follow;
    self
  }
}

#[async_trait]
impl DataSource for LocalFileSystem {
  fn source_type(&self) -> &'static str {
    "LocalFileSystem"
  }

  async fn list_files(&self) -> Result<FileIterator, StorageError> {
    let root = self.root_path.clone();
    let recursive = self.recursive;
    let follow_symlinks = self.follow_symlinks;

    debug!(
      "开始列举本地文件: root={:?}, recursive={}, follow_symlinks={}",
      root, recursive, follow_symlinks
    );

    // 使用 async_stream 创建异步流
    let stream = async_stream::stream! {
        let mut stack = vec![root];
        let mut file_count = 0;

        while let Some(dir) = stack.pop() {
            let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
                warn!("无法读取目录: {:?}", dir);
                continue;
            };

            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();

                let Ok(metadata) = entry.metadata().await else {
                    warn!("无法读取文件元数据: {:?}", path);
                    continue;
                };

                // 处理符号链接
                if metadata.is_symlink() {
                    if !follow_symlinks {
                        debug!("跳过符号链接: {:?}", path);
                        continue;
                    }

                    // 跟随符号链接获取真实元数据
                    let Ok(real_metadata) = tokio::fs::metadata(&path).await else {
                        warn!("无法跟随符号链接: {:?}", path);
                        continue;
                    };

                    if real_metadata.is_dir() && recursive {
                        stack.push(path);
                        continue;
                    } else if !real_metadata.is_file() {
                        continue;
                    }
                } else if metadata.is_dir() {
                    if recursive {
                        stack.push(path);
                    }
                    continue;
                } else if !metadata.is_file() {
                    continue;
                }

                // 生成文件条目
                file_count += 1;
                yield Ok(FileEntry {
                    path: path.to_string_lossy().to_string(),
                    metadata: FileMetadata {
                        size: Some(metadata.len()),
                        modified: metadata
                            .modified()
                            .ok()
                            .and_then(|t| {
                                t.duration_since(std::time::UNIX_EPOCH)
                                    .ok()
                                    .map(|d| d.as_secs() as i64)
                            }),
                        content_type: None,
                    },
                });
            }
        }

        debug!("本地文件列举完成，共 {} 个文件", file_count);
    };

        Ok(Box::new(Box::pin(stream)))
  }

  async fn open_file(&self, entry: &FileEntry) -> Result<FileReader, StorageError> {
    debug!("打开本地文件: {}", entry.path);

    let file = tokio::fs::File::open(&entry.path).await.map_err(|e| {
      if e.kind() == std::io::ErrorKind::NotFound {
        StorageError::NotFound(entry.path.clone())
      } else if e.kind() == std::io::ErrorKind::PermissionDenied {
        StorageError::PermissionDenied(entry.path.clone())
      } else {
        StorageError::Io(e)
      }
    })?;

    Ok(Box::new(file))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use futures::StreamExt;
  use std::io::Write;

  #[tokio::test]
  async fn test_list_files_empty_dir() {
    let temp_dir = tempfile::tempdir().unwrap();
    let source = LocalFileSystem::new(temp_dir.path().to_path_buf());

    let mut files = source.list_files().await.unwrap();
    let mut count = 0;

    while files.next().await.is_some() {
      count += 1;
    }

    assert_eq!(count, 0);
  }

  #[tokio::test]
  async fn test_list_files_with_files() {
    let temp_dir = tempfile::tempdir().unwrap();

    // 创建测试文件
    std::fs::File::create(temp_dir.path().join("file1.txt"))
      .unwrap()
      .write_all(b"test1")
      .unwrap();
    std::fs::File::create(temp_dir.path().join("file2.log"))
      .unwrap()
      .write_all(b"test2")
      .unwrap();

    let source = LocalFileSystem::new(temp_dir.path().to_path_buf());
    let mut files = source.list_files().await.unwrap();

    let mut count = 0;
    while let Some(result) = files.next().await {
      assert!(result.is_ok());
      count += 1;
    }

    assert_eq!(count, 2);
  }

  #[tokio::test]
  async fn test_list_files_recursive() {
    let temp_dir = tempfile::tempdir().unwrap();

    // 创建嵌套目录结构
    std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();
    std::fs::File::create(temp_dir.path().join("file1.txt"))
      .unwrap()
      .write_all(b"test1")
      .unwrap();
    std::fs::File::create(temp_dir.path().join("subdir/file2.txt"))
      .unwrap()
      .write_all(b"test2")
      .unwrap();

    let source = LocalFileSystem::new(temp_dir.path().to_path_buf()).with_recursive(true);
    let mut files = source.list_files().await.unwrap();

    let mut count = 0;
    while files.next().await.is_some() {
      count += 1;
    }

    assert_eq!(count, 2);
  }

  #[tokio::test]
  async fn test_list_files_non_recursive() {
    let temp_dir = tempfile::tempdir().unwrap();

    // 创建嵌套目录结构
    std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();
    std::fs::File::create(temp_dir.path().join("file1.txt"))
      .unwrap()
      .write_all(b"test1")
      .unwrap();
    std::fs::File::create(temp_dir.path().join("subdir/file2.txt"))
      .unwrap()
      .write_all(b"test2")
      .unwrap();

    let source = LocalFileSystem::new(temp_dir.path().to_path_buf()).with_recursive(false);
    let mut files = source.list_files().await.unwrap();

    let mut count = 0;
    while files.next().await.is_some() {
      count += 1;
    }

    assert_eq!(count, 1); // 只有 file1.txt
  }

  #[tokio::test]
  async fn test_open_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    std::fs::File::create(&file_path)
      .unwrap()
      .write_all(b"hello world")
      .unwrap();

    let source = LocalFileSystem::new(temp_dir.path().to_path_buf());
    let entry = FileEntry {
      path: file_path.to_string_lossy().to_string(),
      metadata: FileMetadata::default(),
    };

    let mut reader = source.open_file(&entry).await.unwrap();

    let mut content = String::new();
    use tokio::io::AsyncReadExt;
    reader.read_to_string(&mut content).await.unwrap();

    assert_eq!(content, "hello world");
  }

    #[tokio::test]
    async fn test_open_nonexistent_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source = LocalFileSystem::new(temp_dir.path().to_path_buf());
        
        let entry = FileEntry {
            path: "/nonexistent/file.txt".to_string(),
            metadata: FileMetadata::default(),
        };
        
        let result = source.open_file(&entry).await;
        assert!(result.is_err());
        // 验证是 NotFound 错误
        if let Err(e) = result {
            assert!(matches!(e, StorageError::NotFound(_)));
        }
    }
}
