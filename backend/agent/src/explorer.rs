//! Agent Explorer 封装层
//!
//! 封装 explorer 库的 ResourceLister，添加 Agent 特定的安全检查和配置。

use std::pin::Pin;
use std::sync::Arc;
use tokio::io::AsyncRead;

use explorer::service::{LocalEntry, ResourceLister};

use crate::config::AgentConfig;
use crate::path::resolve_directory_path;

/// Agent Explorer 错误类型
#[derive(Debug)]
pub enum AgentExplorerError {
  /// 路径解析错误
  PathResolution(String),
  /// 归档类型不支持
  UnsupportedArchiveType(String),
  /// 文件操作错误
  FileOperation(String),
}

impl std::fmt::Display for AgentExplorerError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      AgentExplorerError::PathResolution(s) => write!(f, "Path resolution error: {}", s),
      AgentExplorerError::UnsupportedArchiveType(s) => write!(f, "Unsupported archive type: {}", s),
      AgentExplorerError::FileOperation(s) => write!(f, "File operation error: {}", s),
    }
  }
}

impl std::error::Error for AgentExplorerError {}

/// Agent Explorer - 封装 ResourceLister 添加安全检查
pub struct AgentExplorer {
  lister: ResourceLister,
  config: Arc<AgentConfig>,
}

impl AgentExplorer {
  /// 创建新的 AgentExplorer 实例
  pub fn new(config: Arc<AgentConfig>) -> Self {
    Self {
      lister: ResourceLister::new(),
      config,
    }
  }

  /// 列出目录内容（带安全检查和归档支持）
  ///
  /// # Arguments
  /// * `path` - 目录路径（原始字符串）
  /// * `entry` - 归档内路径（可选）
  ///
  /// # Returns
  /// * `Ok(Vec<LocalEntry>)` - 目录条目列表
  /// * `Err(AgentExplorerError)` - 错误
  pub async fn list(&self, path: &str, entry: Option<&str>) -> Result<Vec<LocalEntry>, AgentExplorerError> {
    // 如果有 entry 参数，说明是归档内浏览
    if let Some(inner_path) = entry {
      return self.list_archive_entry(path, inner_path).await;
    }

    // 安全检查：解析并验证路径
    let resolved_paths = resolve_directory_path(&self.config, path).map_err(AgentExplorerError::PathResolution)?;

    let resolved_path = &resolved_paths[0];

    // 检测是否是归档文件
    if let Some(archive_type) = self.lister.detect_archive_type(resolved_path).await {
      return self
        .lister
        .list_archive(resolved_path, archive_type, None)
        .await
        .map_err(AgentExplorerError::FileOperation);
    }

    // 普通目录列表
    self
      .lister
      .list_local(resolved_path)
      .await
      .map_err(AgentExplorerError::FileOperation)
  }

  /// 列出归档内条目
  async fn list_archive_entry(
    &self,
    archive_path: &str,
    inner_path: &str,
  ) -> Result<Vec<LocalEntry>, AgentExplorerError> {
    // 解析归档文件路径（使用文件路径解析逻辑）
    let resolved_paths =
      resolve_directory_path(&self.config, archive_path).map_err(AgentExplorerError::PathResolution)?;

    let resolved_path = &resolved_paths[0];

    // 检测归档类型
    let archive_type = self
      .lister
      .detect_archive_type(resolved_path)
      .await
      .ok_or_else(|| AgentExplorerError::UnsupportedArchiveType(format!("Unknown archive type: {}", archive_path)))?;

    // 列出归档内容
    self
      .lister
      .list_archive(resolved_path, archive_type, Some(inner_path))
      .await
      .map_err(AgentExplorerError::FileOperation)
  }

  /// 下载文件（带安全检查和归档支持）
  ///
  /// # Arguments
  /// * `path` - 文件路径
  /// * `entry` - 归档内路径（可选）
  ///
  /// # Returns
  /// * `Ok((String, Option<u64>, Pin<Box<dyn AsyncRead + Send + Unpin>>))` - (文件名, 大小, 读取器)
  /// * `Err(AgentExplorerError)` - 错误
  pub async fn download(
    &self,
    path: &str,
    entry: Option<&str>,
  ) -> Result<(String, Option<u64>, Pin<Box<dyn AsyncRead + Send + Unpin>>), AgentExplorerError> {
    // 如果有 entry 参数，说明是下载归档内文件
    if let Some(inner_path) = entry {
      return self.download_archive_entry(path, inner_path).await;
    }

    // 安全检查：解析并验证路径
    let resolved_paths = resolve_directory_path(&self.config, path).map_err(AgentExplorerError::PathResolution)?;

    let resolved_path = &resolved_paths[0];

    // 下载文件
    let (name, size, reader) = self
      .lister
      .download_local(resolved_path)
      .await
      .map_err(AgentExplorerError::FileOperation)?;

    Ok((name, Some(size), reader))
  }

  /// 下载归档内文件
  async fn download_archive_entry(
    &self,
    archive_path: &str,
    inner_path: &str,
  ) -> Result<(String, Option<u64>, Pin<Box<dyn AsyncRead + Send + Unpin>>), AgentExplorerError> {
    // 解析归档文件路径
    let resolved_paths =
      resolve_directory_path(&self.config, archive_path).map_err(AgentExplorerError::PathResolution)?;

    let resolved_path = &resolved_paths[0];

    // 检测归档类型
    let archive_type = self
      .lister
      .detect_archive_type(resolved_path)
      .await
      .ok_or_else(|| AgentExplorerError::UnsupportedArchiveType(format!("Unknown archive type: {}", archive_path)))?;

    // 下载归档内文件
    self
      .lister
      .download_archive_entry(resolved_path, inner_path, archive_type)
      .await
      .map_err(AgentExplorerError::FileOperation)
  }

  /// 获取 MIME 类型
  pub fn guess_mime_type(name: &str) -> Option<String> {
    ResourceLister::guess_mime_type(name)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::PathBuf;
  use tokio::sync::Mutex;

  fn create_test_config(roots: Vec<String>) -> Arc<AgentConfig> {
    Arc::new(AgentConfig {
      agent_id: "test-agent".to_string(),
      agent_name: "Test Agent".to_string(),
      server_endpoint: "http://localhost:4000".to_string(),
      search_roots: roots,
      listen_port: 3976,
      enable_heartbeat: false,
      heartbeat_interval_secs: 30,
      worker_threads: None,
      log_dir: PathBuf::from("/tmp"),
      log_retention: 7,
      reload_handle: None,
      current_log_level: Arc::new(Mutex::new("info".to_string())),
    })
  }

  #[tokio::test]
  async fn test_list_local_directory() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file1 = temp_dir.path().join("file1.txt");
    let dir1 = temp_dir.path().join("subdir");

    std::fs::write(&file1, "hello").unwrap();
    std::fs::create_dir(&dir1).unwrap();

    // 需要使用规范化的路径
    let canon_root = std::fs::canonicalize(temp_dir.path()).unwrap();
    let config = create_test_config(vec![canon_root.to_string_lossy().to_string()]);

    let explorer = AgentExplorer::new(config);
    let entries = explorer.list(&canon_root.to_string_lossy(), None).await.unwrap();

    assert_eq!(entries.len(), 2);

    let file_entry = entries.iter().find(|e| e.name == "file1.txt").unwrap();
    assert!(!file_entry.is_dir);

    let dir_entry = entries.iter().find(|e| e.name == "subdir").unwrap();
    assert!(dir_entry.is_dir);
  }

  #[tokio::test]
  async fn test_list_nonexistent_path() {
    let config = create_test_config(vec!["/tmp".to_string()]);
    let explorer = AgentExplorer::new(config);

    let result = explorer.list("/nonexistent/path/12345", None).await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_download_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    std::fs::write(&file_path, "hello download").unwrap();

    let canon_root = std::fs::canonicalize(temp_dir.path()).unwrap();
    let canon_file = std::fs::canonicalize(&file_path).unwrap();

    let config = create_test_config(vec![canon_root.to_string_lossy().to_string()]);
    let explorer = AgentExplorer::new(config);

    let (name, size, mut reader) = explorer.download(&canon_file.to_string_lossy(), None).await.unwrap();

    assert_eq!(name, "test.txt");
    assert_eq!(size, Some(14));

    use tokio::io::AsyncReadExt;
    let mut content = String::new();
    reader.read_to_string(&mut content).await.unwrap();
    assert_eq!(content, "hello download");
  }

  #[test]
  fn test_guess_mime_type() {
    assert_eq!(
      AgentExplorer::guess_mime_type("test.txt"),
      Some("text/plain".to_string())
    );
    assert_eq!(
      AgentExplorer::guess_mime_type("test.json"),
      Some("application/json".to_string())
    );
    assert_eq!(
      AgentExplorer::guess_mime_type("test.tar.gz"),
      Some("application/gzip".to_string())
    );
  }
}
