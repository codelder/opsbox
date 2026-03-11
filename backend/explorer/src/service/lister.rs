//! ResourceLister - 可复用的资源列表核心组件
//!
//! 提供本地目录列表和归档浏览功能，不依赖数据库或 AgentManager，
//! 可被 Agent 和 Server 共享使用。

use std::path::{Path, PathBuf};
use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use opsbox_core::dfs::{
  archive::{ArchiveType, detect_archive_type_from_head, infer_archive_from_path},
  filesystem::{DirEntry, OpbxFileSystem},
  impls::{ArchiveFileSystem, LocalFileSystem},
  path::ResourcePath,
};

/// 本地条目信息（简化版，用于 Agent API）
#[derive(Debug, Clone)]
pub struct LocalEntry {
  pub name: String,
  pub path: String,
  pub is_dir: bool,
  pub is_symlink: bool,
  pub size: u64,
  pub modified: Option<u64>,
  pub child_count: Option<u64>,
  pub hidden_child_count: Option<u64>,
  pub mime_type: Option<String>,
}

/// 资源列表器配置
#[derive(Debug, Clone)]
pub struct ListerConfig {
  /// 是否进行 MIME 类型检测
  pub mime_detection: bool,
  /// 是否计算隐藏文件数量
  pub hidden_count: bool,
}

impl Default for ListerConfig {
  fn default() -> Self {
    Self {
      mime_detection: true,
      hidden_count: true,
    }
  }
}

/// 可复用的资源列表器（不依赖数据库、AgentManager）
pub struct ResourceLister {
  config: ListerConfig,
}

impl Default for ResourceLister {
  fn default() -> Self {
    Self::new()
  }
}

impl ResourceLister {
  /// 创建新的 ResourceLister 实例
  pub fn new() -> Self {
    Self {
      config: ListerConfig::default(),
    }
  }

  /// 列出本地目录内容
  ///
  /// # Arguments
  /// * `path` - 目录路径
  ///
  /// # Returns
  /// * `Ok(Vec<LocalEntry>)` - 目录条目列表
  /// * `Err(String)` - 错误信息
  pub async fn list_local(&self, path: &Path) -> Result<Vec<LocalEntry>, String> {
    if !path.exists() {
      return Err(format!("Path does not exist: {}", path.display()));
    }

    if !path.is_dir() {
      return Err(format!("Path is not a directory: {}", path.display()));
    }

    // 使用 LocalFileSystem 进行目录读取
    let root = if path.is_absolute() {
      PathBuf::from("/")
    } else {
      PathBuf::from(".")
    };

    let fs = LocalFileSystem::new(root).map_err(|e| format!("Failed to create local FS: {}", e))?;

    // 转换为 ResourcePath
    let resource_path = ResourcePath::parse(&path.display().to_string());

    let entries = fs
      .read_dir(&resource_path)
      .await
      .map_err(|e| format!("Failed to read directory: {}", e))?;

    // 转换为 LocalEntry
    Ok(entries.into_iter().map(|e| self.map_to_local_entry(e)).collect())
  }

  /// 列出归档内容
  ///
  /// # Arguments
  /// * `archive_path` - 归档文件路径
  /// * `archive_type` - 归档类型
  /// * `inner_path` - 归档内路径（可选，默认为根）
  ///
  /// # Returns
  /// * `Ok(Vec<LocalEntry>)` - 归档条目列表
  /// * `Err(String)` - 错误信息
  pub async fn list_archive(
    &self,
    archive_path: &Path,
    archive_type: ArchiveType,
    inner_path: Option<&str>,
  ) -> Result<Vec<LocalEntry>, String> {
    // 获取归档文件的父目录作为 LocalFileSystem 的根
    let archive_dir = archive_path
      .parent()
      .ok_or_else(|| "Failed to get archive parent directory".to_string())?;

    // 创建本地文件系统
    let local_fs =
      LocalFileSystem::new(archive_dir.to_path_buf()).map_err(|e| format!("Failed to create local FS: {}", e))?;

    // 创建归档文件系统
    let archive_fs = ArchiveFileSystem::with_path(local_fs, archive_type, archive_path.to_path_buf());

    // 解析归档内路径
    let inner = inner_path.unwrap_or("/");
    let inner_resource_path = ResourcePath::parse(inner);

    // 读取归档内容
    let entries = archive_fs.read_dir(&inner_resource_path)
            .await
            .map_err(|e| {
                let error_str = e.to_string();
                if error_str.contains("Failed to read TAR entry") || error_str.contains("numeric field did not have utf-8") {
                    "无法解析归档文件：文件可能损坏或使用了不兼容的格式。建议：1) 使用 'tar -tzf 文件名.tar.gz' 验证文件完整性 2) 尝试使用 'gunzip -c 文件名.tar.gz | tar tf -' 重新打包".to_string()
                } else if error_str.contains("Failed to read TAR entries") {
                    format!("无法读取归档内容：{}", error_str)
                } else {
                    format!("Failed to read archive directory: {}", error_str)
                }
            })?;

    // 转换为 LocalEntry
    Ok(entries.into_iter().map(|e| self.map_to_local_entry(e)).collect())
  }

  /// 下载本地文件
  ///
  /// # Arguments
  /// * `path` - 文件路径
  ///
  /// # Returns
  /// * `Ok((String, u64, Pin<Box<dyn AsyncRead + Send + Unpin>>))` - (文件名, 文件大小, 读取器)
  /// * `Err(String)` - 错误信息
  pub async fn download_local(
    &self,
    path: &Path,
  ) -> Result<(String, u64, Pin<Box<dyn AsyncRead + Send + Unpin>>), String> {
    if !path.exists() {
      return Err(format!("File does not exist: {}", path.display()));
    }

    if !path.is_file() {
      return Err(format!("Path is not a file: {}", path.display()));
    }

    // 获取文件名
    let name = path
      .file_name()
      .and_then(|n| n.to_str())
      .unwrap_or("download")
      .to_string();

    // 获取文件大小
    let metadata = tokio::fs::metadata(path)
      .await
      .map_err(|e| format!("Failed to get file metadata: {}", e))?;
    let size = metadata.len();

    // 打开文件
    let file = tokio::fs::File::open(path)
      .await
      .map_err(|e| format!("Failed to open file: {}", e))?;

    Ok((name, size, Box::pin(file)))
  }

  /// 下载归档内文件
  ///
  /// # Arguments
  /// * `archive_path` - 归档文件路径
  /// * `entry_path` - 归档内文件路径
  /// * `archive_type` - 归档类型
  ///
  /// # Returns
  /// * `Ok((String, Option<u64>, Pin<Box<dyn AsyncRead + Send + Unpin>>))` - (文件名, 文件大小, 读取器)
  /// * `Err(String)` - 错误信息
  pub async fn download_archive_entry(
    &self,
    archive_path: &Path,
    entry_path: &str,
    archive_type: ArchiveType,
  ) -> Result<(String, Option<u64>, Pin<Box<dyn AsyncRead + Send + Unpin>>), String> {
    // 获取归档文件的父目录作为 LocalFileSystem 的根
    let archive_dir = archive_path
      .parent()
      .ok_or_else(|| "Failed to get archive parent directory".to_string())?;

    // 创建本地文件系统
    let local_fs =
      LocalFileSystem::new(archive_dir.to_path_buf()).map_err(|e| format!("Failed to create local FS: {}", e))?;

    // 创建归档文件系统
    let archive_fs = ArchiveFileSystem::with_path(local_fs, archive_type, archive_path.to_path_buf());

    // 解析归档内路径
    let entry_resource_path = ResourcePath::parse(entry_path);

    // 获取元数据
    let meta = archive_fs.metadata(&entry_resource_path).await.map_err(|e| {
      let error_str = e.to_string();
      if error_str.contains("numeric field did not have utf-8") {
        "无法解析归档文件：文件可能损坏或使用了不兼容的格式".to_string()
      } else {
        format!("Failed to get metadata: {}", error_str)
      }
    })?;

    // 打开文件
    let reader = archive_fs.open_read(&entry_resource_path).await.map_err(|e| {
      let error_str = e.to_string();
      if error_str.contains("numeric field did not have utf-8") {
        "无法读取归档内文件：文件可能损坏".to_string()
      } else {
        format!("Failed to open file: {}", error_str)
      }
    })?;

    // 获取文件名
    let name = entry_resource_path
      .segments()
      .last()
      .cloned()
      .unwrap_or_else(|| "download".to_string());

    Ok((name, Some(meta.size), reader))
  }

  /// 检测归档类型（基于 magic bytes）
  ///
  /// # Arguments
  /// * `path` - 文件路径
  ///
  /// # Returns
  /// * `Some(ArchiveType)` - 检测到的归档类型
  /// * `None` - 不是归档文件或无法检测
  pub async fn detect_archive_type(&self, path: &Path) -> Option<ArchiveType> {
    // 首先尝试读取文件头
    if let Ok(mut file) = tokio::fs::File::open(path).await {
      let mut buffer = vec![0u8; 2048];
      if let Ok(n) = file.read(&mut buffer).await {
        buffer.truncate(n);

        if !buffer.is_empty() {
          // 使用 magic bytes 检测
          let archive_type = detect_archive_type_from_head(&buffer);
          if archive_type != ArchiveType::Unknown {
            return Some(archive_type);
          }
        }
      }
    }

    // 回退到扩展名检测
    infer_archive_from_path(&path.to_string_lossy())
  }

  /// 将 DirEntry 映射为 LocalEntry
  fn map_to_local_entry(&self, entry: DirEntry) -> LocalEntry {
    let mime_type = if self.config.mime_detection && !entry.metadata.is_dir {
      Self::guess_mime_type(&entry.name)
    } else {
      None
    };

    LocalEntry {
      name: entry.name,
      path: entry.path.to_string(),
      is_dir: entry.metadata.is_dir,
      is_symlink: entry.metadata.is_symlink,
      size: entry.metadata.size,
      modified: entry
        .metadata
        .modified
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs()),
      child_count: None,
      hidden_child_count: None,
      mime_type,
    }
  }

  /// 根据文件扩展名推断 MIME 类型
  pub fn guess_mime_type(name: &str) -> Option<String> {
    let ext = name.rsplit('.').next()?.to_lowercase();
    let mime = match ext.as_str() {
      // 文本 / 日志 / 配置
      "txt" | "log" | "out" | "err" => "text/plain",
      "csv" => "text/csv",
      "json" => "application/json",
      "xml" => "application/xml",
      "yaml" | "yml" => "text/yaml",
      "toml" => "text/toml",
      "ini" | "cfg" | "conf" | "properties" => "text/plain",
      "md" | "markdown" => "text/markdown",
      "html" | "htm" => "text/html",
      "css" => "text/css",
      // 代码
      "js" | "mjs" | "cjs" => "text/javascript",
      "ts" | "tsx" | "jsx" => "text/typescript",
      "rs" => "text/x-rust",
      "py" => "text/x-python",
      "go" => "text/x-go",
      "java" => "text/x-java",
      "c" | "h" => "text/x-c",
      "cpp" | "cc" | "cxx" | "hpp" => "text/x-c++",
      "sh" | "bash" | "zsh" => "text/x-shellscript",
      "sql" => "application/sql",
      // 归档
      "gz" | "gzip" => "application/gzip",
      "tar" => "application/x-tar",
      "zip" => "application/zip",
      "tgz" => "application/gzip",
      "bz2" => "application/x-bzip2",
      "xz" => "application/x-xz",
      "7z" => "application/x-7z-compressed",
      "rar" => "application/x-rar-compressed",
      // 图片
      "png" => "image/png",
      "jpg" | "jpeg" => "image/jpeg",
      "gif" => "image/gif",
      "webp" => "image/webp",
      "svg" => "image/svg+xml",
      "ico" => "image/x-icon",
      // 音视频
      "mp3" => "audio/mpeg",
      "mp4" => "video/mp4",
      "wav" => "audio/wav",
      // 其他
      "pdf" => "application/pdf",
      "wasm" => "application/wasm",
      _ => return None,
    };
    Some(mime.to_string())
  }

  /// 列出归档内容（使用已有的 reader，用于远程归档场景）
  ///
  /// 这个方法接收一个 AsyncRead，将其写入临时文件，然后列出归档内容。
  /// 适用于 S3 或 Agent 远程归档文件。
  pub async fn list_archive_from_reader(
    &self,
    mut reader: Pin<Box<dyn AsyncRead + Send + Unpin>>,
    archive_type: ArchiveType,
    inner_path: Option<&str>,
  ) -> Result<Vec<LocalEntry>, String> {
    // 创建临时文件
    let temp_file = tokio::task::spawn_blocking(tempfile::NamedTempFile::new)
      .await
      .map_err(|e| format!("Failed to spawn blocking task: {}", e))?
      .map_err(|e| format!("Failed to create temp file: {}", e))?;

    let temp_path = temp_file.path().to_path_buf();

    // 将 reader 内容写入临时文件
    let mut dst = tokio::fs::File::from_std(
      temp_file
        .as_file()
        .try_clone()
        .map_err(|e| format!("Failed to clone temp file: {}", e))?,
    );

    tokio::io::copy(&mut reader, &mut dst)
      .await
      .map_err(|e| format!("Failed to copy archive data: {}", e))?;

    dst
      .flush()
      .await
      .map_err(|e| format!("Failed to flush temp file: {}", e))?;

    // 使用临时文件列出归档内容
    self
      .list_archive_with_temp_file(&temp_path, archive_type, inner_path, temp_file)
      .await
  }

  /// 使用临时文件列出归档内容
  async fn list_archive_with_temp_file(
    &self,
    archive_path: &Path,
    archive_type: ArchiveType,
    inner_path: Option<&str>,
    _temp_file: tempfile::NamedTempFile, // 保持所有权以延长临时文件生命周期
  ) -> Result<Vec<LocalEntry>, String> {
    // 获取归档文件的父目录作为 LocalFileSystem 的根
    let archive_dir = archive_path
      .parent()
      .ok_or_else(|| "Failed to get archive parent directory".to_string())?;

    // 创建本地文件系统
    let local_fs =
      LocalFileSystem::new(archive_dir.to_path_buf()).map_err(|e| format!("Failed to create local FS: {}", e))?;

    // 创建归档文件系统（使用临时文件）
    let archive_fs = ArchiveFileSystem::with_path(local_fs, archive_type, archive_path.to_path_buf());

    // 解析归档内路径
    let inner = inner_path.unwrap_or("/");
    let inner_resource_path = ResourcePath::parse(inner);

    // 读取归档内容
    let entries = archive_fs
      .read_dir(&inner_resource_path)
      .await
      .map_err(|e| format!("Failed to read archive directory: {}", e))?;

    // 转换为 LocalEntry
    Ok(entries.into_iter().map(|e| self.map_to_local_entry(e)).collect())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;

  #[tokio::test]
  async fn test_list_local_directory() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file1 = temp_dir.path().join("file1.txt");
    let dir1 = temp_dir.path().join("subdir");

    fs::write(&file1, "hello").unwrap();
    fs::create_dir(&dir1).unwrap();

    let lister = ResourceLister::new();
    let entries = lister.list_local(temp_dir.path()).await.unwrap();

    assert_eq!(entries.len(), 2);

    let file_entry = entries.iter().find(|e| e.name == "file1.txt").unwrap();
    assert!(!file_entry.is_dir);
    assert_eq!(file_entry.size, 5);

    let dir_entry = entries.iter().find(|e| e.name == "subdir").unwrap();
    assert!(dir_entry.is_dir);
  }

  #[tokio::test]
  async fn test_list_local_nonexistent() {
    let lister = ResourceLister::new();
    let result = lister.list_local(Path::new("/nonexistent/path/12345")).await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_download_local_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "hello download").unwrap();

    let lister = ResourceLister::new();
    let (name, size, mut reader) = lister.download_local(&file_path).await.unwrap();

    assert_eq!(name, "test.txt");
    assert_eq!(size, 14);

    let mut content = String::new();
    reader.read_to_string(&mut content).await.unwrap();
    assert_eq!(content, "hello download");
  }

  #[tokio::test]
  async fn test_detect_archive_type() {
    let temp_dir = tempfile::tempdir().unwrap();
    let lister = ResourceLister::new();

    // 测试扩展名检测
    let tar_file = temp_dir.path().join("test.tar");
    fs::write(&tar_file, "").unwrap();
    assert_eq!(lister.detect_archive_type(&tar_file).await, Some(ArchiveType::Tar));

    let tar_gz_file = temp_dir.path().join("test.tar.gz");
    fs::write(&tar_gz_file, "").unwrap();
    assert_eq!(lister.detect_archive_type(&tar_gz_file).await, Some(ArchiveType::TarGz));

    let zip_file = temp_dir.path().join("test.zip");
    fs::write(&zip_file, "").unwrap();
    assert_eq!(lister.detect_archive_type(&zip_file).await, Some(ArchiveType::Zip));

    let gz_file = temp_dir.path().join("test.gz");
    fs::write(&gz_file, "").unwrap();
    assert_eq!(lister.detect_archive_type(&gz_file).await, Some(ArchiveType::Gz));
  }

  #[test]
  fn test_guess_mime_type() {
    assert_eq!(
      ResourceLister::guess_mime_type("test.txt"),
      Some("text/plain".to_string())
    );
    assert_eq!(
      ResourceLister::guess_mime_type("test.json"),
      Some("application/json".to_string())
    );
    assert_eq!(
      ResourceLister::guess_mime_type("test.log"),
      Some("text/plain".to_string())
    );
    assert_eq!(
      ResourceLister::guess_mime_type("test.png"),
      Some("image/png".to_string())
    );
    assert_eq!(ResourceLister::guess_mime_type("test.unknown"), None);
  }

  #[tokio::test]
  async fn test_list_tar_archive() {
    use tar::Builder;

    let temp_dir = tempfile::tempdir().unwrap();
    let tar_path = temp_dir.path().join("test.tar");

    // 创建 tar 归档
    {
      let file = fs::File::create(&tar_path).unwrap();
      let mut builder = Builder::new(file);

      let mut header = tar::Header::new_gnu();
      header.set_size(5);
      header.set_cksum();
      builder
        .append_data(&mut header, "file1.txt", "hello".as_bytes())
        .unwrap();

      builder.finish().unwrap();
    }

    let lister = ResourceLister::new();
    let entries = lister.list_archive(&tar_path, ArchiveType::Tar, None).await.unwrap();

    assert!(!entries.is_empty());
    let file_entry = entries.iter().find(|e| e.name == "file1.txt");
    assert!(file_entry.is_some());
  }

  #[tokio::test]
  async fn test_list_tar_gz_archive() {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tar::Builder;

    let temp_dir = tempfile::tempdir().unwrap();
    let tar_gz_path = temp_dir.path().join("test.tar.gz");

    // 创建 tar.gz 归档
    {
      let file = fs::File::create(&tar_gz_path).unwrap();
      let encoder = GzEncoder::new(file, Compression::default());
      let mut builder = Builder::new(encoder);

      let mut header = tar::Header::new_gnu();
      header.set_size(5);
      header.set_cksum();
      builder
        .append_data(&mut header, "inner.log", "hello".as_bytes())
        .unwrap();

      builder.finish().unwrap();
    }

    let lister = ResourceLister::new();
    let entries = lister
      .list_archive(&tar_gz_path, ArchiveType::TarGz, None)
      .await
      .unwrap();

    assert!(!entries.is_empty());
    let file_entry = entries.iter().find(|e| e.name == "inner.log");
    assert!(file_entry.is_some());
  }
}
