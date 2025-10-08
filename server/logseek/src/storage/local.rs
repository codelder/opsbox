// ============================================================================
// 本地文件系统存储源
// ============================================================================

use super::{DataSource, FileEntry, FileIterator, FileMetadata, FileReader, StorageError};
use async_trait::async_trait;
use log::{debug, warn};
use regex::Regex;
use std::path::PathBuf;
use std::collections::HashSet;

/// 本地文件系统存储源
///
/// 提供对本地文件系统的访问，搜索逻辑由 Server 端执行
/// 
/// # 功能特性
/// - 递归/非递归目录遍历
/// - 文件名模式过滤（正则表达式）
/// - 符号链接处理（防止循环）
/// - 权限检查
/// - 大目录优化（限制每目录文件数）
pub struct LocalFileSystem {
  /// 根目录路径
  root_path: PathBuf,
  /// 是否递归搜索子目录
  recursive: bool,
  /// 是否跟随符号链接
  follow_symlinks: bool,
  /// 文件名过滤模式（正则表达式）
  pattern: Option<Regex>,
  /// 每个目录最大文件数（防止大目录卡顿）
  max_files_per_dir: Option<usize>,
  /// 最大递归深度（防止目录树过深）
  max_depth: Option<usize>,
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
      pattern: None,
      max_files_per_dir: Some(10000), // 默认每目录最多10000个文件
      max_depth: Some(20),             // 默认最大深度20层
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

  /// 设置文件名过滤模式（正则表达式）
  ///
  /// # 参数
  ///
  /// * `pattern` - 正则表达式字符串，例如 `r".*\.log$"` 只匹配 .log 文件
  ///
  /// # 示例
  ///
  /// ```no_run
  /// # use std::path::PathBuf;
  /// # use logseek::storage::local::LocalFileSystem;
  /// let source = LocalFileSystem::new(PathBuf::from("/var/log"))
  ///     .with_pattern(r".*\.log$".to_string()).unwrap();
  /// ```
  pub fn with_pattern(mut self, pattern: String) -> Result<Self, StorageError> {
    let regex = Regex::new(&pattern).map_err(|e| {
      StorageError::Other(format!("无效的正则表达式: {}", e))
    })?;
    self.pattern = Some(regex);
    Ok(self)
  }

  /// 设置每个目录最大文件数
  ///
  /// 用于防止大目录导致内存溢出或长时间阻塞
  pub fn with_max_files_per_dir(mut self, max: usize) -> Self {
    self.max_files_per_dir = Some(max);
    self
  }

  /// 设置最大递归深度
  ///
  /// 防止目录树过深导致栈溢出
  pub fn with_max_depth(mut self, max: usize) -> Self {
    self.max_depth = Some(max);
    self
  }

  /// 检查文件名是否匹配过滤模式
  fn matches_pattern(&self, file_name: &str) -> bool {
    match &self.pattern {
      Some(regex) => regex.is_match(file_name),
      None => true, // 没有模式则匹配所有文件
    }
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
    let pattern = self.pattern.clone();
    let max_files_per_dir = self.max_files_per_dir;
    let max_depth = self.max_depth;

    debug!(
      "开始列举本地文件: root={:?}, recursive={}, follow_symlinks={}, pattern={:?}, max_files_per_dir={:?}, max_depth={:?}",
      root, recursive, follow_symlinks, 
      pattern.as_ref().map(|p| p.as_str()), 
      max_files_per_dir, 
      max_depth
    );

    // 使用 async_stream 创建异步流
    let stream = async_stream::stream! {
        // 用于检测符号链接循环
        let mut visited_inodes = HashSet::new();
        // 目录栈：(path, depth)
        let mut stack = vec![(root, 0)];
        let mut total_file_count = 0;
        let mut skipped_dirs = 0;

        while let Some((dir, depth)) = stack.pop() {
            // 检查深度限制
            if let Some(max) = max_depth {
                if depth >= max {
                    debug!("达到最大深度 {}，跳过目录: {:?}", max, dir);
                    skipped_dirs += 1;
                    continue;
                }
            }

            let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
                warn!("无法读取目录: {:?}", dir);
                continue;
            };

            let mut dir_file_count = 0;

            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();

                // 检查每目录文件数限制
                if let Some(max) = max_files_per_dir {
                    if dir_file_count >= max {
                        warn!(
                            "目录 {:?} 文件数超过限制 {}+，停止扫描该目录",
                            dir, max
                        );
                        break;
                    }
                }

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

                    // 检测循环链接（通过 inode 号）
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::MetadataExt;
                        let inode = real_metadata.ino();
                        if visited_inodes.contains(&inode) {
                            warn!("检测到循环符号链接，跳过: {:?}", path);
                            continue;
                        }
                        visited_inodes.insert(inode);
                    }

                    if real_metadata.is_dir() && recursive {
                        stack.push((path, depth + 1));
                        continue;
                    } else if !real_metadata.is_file() {
                        continue;
                    }
                } else if metadata.is_dir() {
                    if recursive {
                        stack.push((path, depth + 1));
                    }
                    continue;
                } else if !metadata.is_file() {
                    continue;
                }

                // 文件名过滤
                let file_name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                
                if let Some(ref regex) = pattern {
                    if !regex.is_match(file_name) {
                        debug!("文件名不匹配过滤模式，跳过: {}", file_name);
                        continue;
                    }
                }

                // 检查文件读取权限
                if let Err(e) = tokio::fs::File::open(&path).await {
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        debug!("没有读取权限，跳过: {:?}", path);
                        continue;
                    }
                    // 其他错误也跳过
                    warn!("无法打开文件: {:?}, 错误: {}", path, e);
                    continue;
                }

                // 生成文件条目
                dir_file_count += 1;
                total_file_count += 1;
                
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

        debug!(
            "本地文件列举完成：总计 {} 个文件，跳过 {} 个目录",
            total_file_count, skipped_dirs
        );
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

    #[tokio::test]
    async fn test_pattern_filtering() {
        let temp_dir = tempfile::tempdir().unwrap();

        // 创建不同类型的文件
        std::fs::File::create(temp_dir.path().join("app.log"))
            .unwrap()
            .write_all(b"log1")
            .unwrap();
        std::fs::File::create(temp_dir.path().join("error.log"))
            .unwrap()
            .write_all(b"log2")
            .unwrap();
        std::fs::File::create(temp_dir.path().join("config.txt"))
            .unwrap()
            .write_all(b"config")
            .unwrap();
        std::fs::File::create(temp_dir.path().join("data.json"))
            .unwrap()
            .write_all(b"json")
            .unwrap();

        // 只匹配 .log 文件
        let source = LocalFileSystem::new(temp_dir.path().to_path_buf())
            .with_pattern(r".*\.log$".to_string())
            .unwrap();

        let mut files = source.list_files().await.unwrap();
        let mut count = 0;
        let mut log_files = vec![];

        while let Some(result) = files.next().await {
            assert!(result.is_ok());
            let entry = result.unwrap();
            log_files.push(entry.path);
            count += 1;
        }

        assert_eq!(count, 2); // 只有两个 .log 文件
        assert!(log_files.iter().all(|p| p.ends_with(".log")));
    }

    #[tokio::test]
    async fn test_max_files_per_dir() {
        let temp_dir = tempfile::tempdir().unwrap();

        // 创建 100 个文件
        for i in 0..100 {
            std::fs::File::create(temp_dir.path().join(format!("file{}.txt", i)))
                .unwrap()
                .write_all(b"test")
                .unwrap();
        }

        // 限制每目录最多 50 个文件
        let source = LocalFileSystem::new(temp_dir.path().to_path_buf())
            .with_max_files_per_dir(50);

        let mut files = source.list_files().await.unwrap();
        let mut count = 0;

        while files.next().await.is_some() {
            count += 1;
        }

        assert_eq!(count, 50); // 只读取了 50 个文件
    }

    #[tokio::test]
    async fn test_max_depth() {
        let temp_dir = tempfile::tempdir().unwrap();

        // 创建深度为 5 的目录树
        let mut current = temp_dir.path().to_path_buf();
        for i in 0..5 {
            current = current.join(format!("level{}", i));
            std::fs::create_dir(&current).unwrap();
            std::fs::File::create(current.join(format!("file{}.txt", i)))
                .unwrap()
                .write_all(b"test")
                .unwrap();
        }

        // 限制最大深度为 3
        let source = LocalFileSystem::new(temp_dir.path().to_path_buf())
            .with_max_depth(3);

        let mut files = source.list_files().await.unwrap();
        let mut count = 0;

        while files.next().await.is_some() {
            count += 1;
        }

        assert_eq!(count, 3); // 只读取了深度 0, 1, 2 的文件
    }

    #[tokio::test]
    #[cfg(unix)] // 符号链接只在 Unix 系统上测试
    async fn test_symlink_loop_detection() {
        let temp_dir = tempfile::tempdir().unwrap();

        // 创建目录 A 和 B
        let dir_a = temp_dir.path().join("dir_a");
        let dir_b = temp_dir.path().join("dir_b");
        std::fs::create_dir(&dir_a).unwrap();
        std::fs::create_dir(&dir_b).unwrap();

        // 创建循环符号链接: A -> B 和 B -> A
        std::os::unix::fs::symlink(&dir_b, dir_a.join("link_to_b")).unwrap();
        std::os::unix::fs::symlink(&dir_a, dir_b.join("link_to_a")).unwrap();

        // 创建一个文件在 A 中
        std::fs::File::create(dir_a.join("file.txt"))
            .unwrap()
            .write_all(b"test")
            .unwrap();

        // 启用符号链接跟随
        let source = LocalFileSystem::new(temp_dir.path().to_path_buf())
            .with_follow_symlinks(true);

        let mut files = source.list_files().await.unwrap();
        let mut count = 0;

        while files.next().await.is_some() {
            count += 1;
            // 防止无限循环，设置上限
            if count > 100 {
                panic!("检测到可能的无限循环");
            }
        }

        // 应该只有一个文件，循环链接被检测并跳过
        assert_eq!(count, 1);
    }
}
