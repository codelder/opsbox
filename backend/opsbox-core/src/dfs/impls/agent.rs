//! AgentProxyFS 模块 - Agent 代理文件系统实现
//!
//! 通过 HTTP 代理与远程 Agent 通信，访问远程文件系统

use async_trait::async_trait;
use serde::Deserialize;
use serde_json;
use std::pin::Pin;

use super::super::{
  filesystem::{DirEntry, FileMetadata, FsError, MemoryReader, OpbxFileSystem},
  path::ResourcePath,
  searchable::{SearchConfig, Streamable},
};
use crate::fs::EntryStream;

/// Agent HTTP 客户端
#[derive(Debug, Clone)]
pub struct AgentClient {
  /// Agent 主机地址
  pub host: String,
  /// Agent 端口
  pub port: u16,
  /// HTTP 客户端
  client: reqwest::Client,
}

impl AgentClient {
  /// 创建新的 Agent 客户端
  pub fn new(host: String, port: u16) -> Result<Self, FsError> {
    Ok(Self {
      host,
      port,
      client: reqwest::Client::new(),
    })
  }

  /// 构建完整的 API URL
  fn build_url(&self, path: &str) -> String {
    format!("http://{}:{}{}", self.host, self.port, path)
  }
}

/// Agent list_files API 响应
#[derive(Debug, Deserialize)]
struct AgentListResponse {
  items: Vec<AgentFileItem>,
}

/// Agent 文件项
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AgentFileItem {
  name: String,
  path: String,
  #[serde(alias = "is_dir")]
  is_dir: bool,
  #[serde(alias = "is_symlink")]
  is_symlink: bool,
  size: Option<u64>,
  modified: Option<i64>,
  child_count: Option<u32>,
  #[serde(default)]
  hidden_child_count: Option<u32>,
  #[serde(default)]
  mime_type: Option<String>,
}

/// Agent 代理文件系统
///
/// 通过 HTTP API 与远程 Agent 通信
#[derive(Debug, Clone)]
pub struct AgentProxyFS {
  client: AgentClient,
}

impl AgentProxyFS {
  /// 创建新的 Agent 代理文件系统
  pub fn new(client: AgentClient) -> Self {
    Self { client }
  }

  /// 将 ResourcePath 转换为 API 路径
  ///
  /// 特殊处理 Windows 驱动器路径（如 D:/path），避免生成 /D:/path 这样的无效路径
  fn resource_path_to_api_path(&self, path: &ResourcePath) -> String {
    let segments = path.segments();
    if segments.is_empty() {
      return "/".to_string();
    }

    // 检查是否是 Windows 驱动器路径（第一个 segment 以 : 结尾，如 "D:"）
    let is_windows_path = segments
      .first()
      .map(|s| s.len() == 2 && s.ends_with(':') && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic()))
      .unwrap_or(false);

    let joined = segments.join("/");
    if is_windows_path {
      // Windows 路径不需要前导斜杠：D:/path/to/file
      joined
    } else {
      // Unix 路径需要前导斜杠：/path/to/file
      format!("/{}", joined)
    }
  }
}

#[async_trait]
impl OpbxFileSystem for AgentProxyFS {
  /// 获取文件/目录元数据
  /// 注意：Agent 没有单独的 metadata API，我们通过 list_files 来获取单个文件的信息
  async fn metadata(&self, path: &ResourcePath) -> Result<FileMetadata, FsError> {
    let api_path = self.resource_path_to_api_path(path);

    // 对于根目录，返回默认目录元数据
    if api_path == "/" {
      return Ok(FileMetadata::dir(0));
    }

    // 获取路径的父目录
    let segments = path.segments();
    let parent_segments: Vec<_> = if segments.len() > 1 {
      segments[..segments.len() - 1].to_vec()
    } else {
      vec![]
    };

    let parent_path = ResourcePath::new(parent_segments, path.is_absolute());
    let parent_api_path = self.resource_path_to_api_path(&parent_path);
    let url = self.client.build_url("/api/v1/list_files");

    let response = self
      .client
      .client
      .get(&url)
      .query(&[("path", &parent_api_path)])
      .send()
      .await
      .map_err(|e| FsError::Agent(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
      return Err(FsError::NotFound(format!(
        "Agent returned error: {}",
        response.status()
      )));
    }

    let list_response: AgentListResponse = response
      .json()
      .await
      .map_err(|e| FsError::Agent(format!("Failed to parse response: {}", e)))?;

    // 查找匹配的文件
    let file_name = segments.last().map(|s| s.as_str()).unwrap_or("");
    for item in list_response.items {
      if item.name == file_name {
        return Ok(FileMetadata {
          is_dir: item.is_dir,
          is_file: !item.is_dir,
          is_symlink: item.is_symlink,
          size: item.size.unwrap_or(0),
          modified: item.modified.and_then(|ts| {
            if ts >= 0 {
              std::time::SystemTime::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(ts as u64))
            } else {
              None
            }
          }),
          created: None,
        });
      }
    }

    Err(FsError::NotFound(format!("File not found: {}", api_path)))
  }

  /// 读取目录内容
  async fn read_dir(&self, path: &ResourcePath) -> Result<Vec<DirEntry>, FsError> {
    let api_path = self.resource_path_to_api_path(path);

    let url = self.client.build_url("/api/v1/list_files");

    let response = self
      .client
      .client
      .get(&url)
      .query(&[("path", &api_path)])
      .send()
      .await
      .map_err(|e| FsError::Agent(format!("HTTP request failed: {}", e)))?;

    let status = response.status();
    let response_text = response
      .text()
      .await
      .map_err(|e| FsError::Agent(format!("Failed to read response: {}", e)))?;

    if !status.is_success() {
      return Err(FsError::NotFound(format!("Agent returned error: {}", status)));
    }

    let list_response: AgentListResponse = serde_json::from_str(&response_text).map_err(|e| {
      tracing::error!("AgentProxyFS::read_dir: failed to parse JSON: {}", e);
      FsError::Agent(format!("Failed to parse response: {}", e))
    })?;

    let mut entries = Vec::new();
    for item in list_response.items {
      let file_meta = FileMetadata {
        is_dir: item.is_dir,
        is_file: !item.is_dir,
        is_symlink: item.is_symlink,
        size: item.size.unwrap_or(0),
        modified: item.modified.and_then(|ts| {
          if ts >= 0 {
            std::time::SystemTime::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(ts as u64))
          } else {
            None
          }
        }),
        created: None,
      };

      // 使用相对路径构建 ResourcePath
      let entry_path = ResourcePath::parse(&item.path);

      entries.push(DirEntry {
        name: item.name,
        path: entry_path,
        metadata: file_meta,
      });
    }

    Ok(entries)
  }

  /// 打开文件用于读取
  async fn open_read(&self, path: &ResourcePath) -> Result<Pin<Box<dyn tokio::io::AsyncRead + Send + Unpin>>, FsError> {
    let api_path = self.resource_path_to_api_path(path);
    let url = self.client.build_url("/api/v1/file_raw");

    let response = self
      .client
      .client
      .get(&url)
      .query(&[("path", &api_path)])
      .send()
      .await
      .map_err(|e| FsError::Agent(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
      return Err(FsError::NotFound(format!(
        "Agent returned error: {}",
        response.status()
      )));
    }

    let bytes = response
      .bytes()
      .await
      .map_err(|e| FsError::Agent(format!("Failed to read response body: {}", e)))?;

    Ok(Box::pin(MemoryReader::new(bytes.to_vec())))
  }
}

/// Agent 条目流
///
/// 通过 HTTP API 从远程 Agent 获取文件列表
pub struct AgentEntryStream {
  client: AgentClient,
  api_path: String,
  files: Vec<String>,
  idx: usize,
  loaded: bool,
}

impl AgentEntryStream {
  /// 创建新的 Agent 条目流
  pub fn new(client: AgentClient, api_path: String) -> Self {
    Self {
      client,
      api_path,
      files: Vec::new(),
      idx: 0,
      loaded: false,
    }
  }

  /// 从 Agent 加载文件列表
  async fn load_files(&mut self) -> std::io::Result<()> {
    #[derive(Debug, Deserialize)]
    struct AgentListResponse {
      items: Vec<AgentFileItem>,
    }

    #[derive(Debug, Deserialize)]
    struct AgentFileItem {
      path: String,
      #[serde(alias = "is_dir")]
      is_dir: bool,
    }

    let url = self.client.build_url("/api/v1/list_files");

    let response = self
      .client
      .client
      .get(&url)
      .query(&[("path", &self.api_path)])
      .query(&[("recursive", "true")])
      .send()
      .await
      .map_err(|e| std::io::Error::other(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
      return Err(std::io::Error::other(format!(
        "Agent returned error: {}",
        response.status()
      )));
    }

    let list_response: AgentListResponse = response
      .json()
      .await
      .map_err(|e| std::io::Error::other(format!("Failed to parse response: {}", e)))?;

    self.files = list_response
      .items
      .into_iter()
      .filter(|item| !item.is_dir)
      .map(|item| item.path)
      .collect();
    self.loaded = true;

    Ok(())
  }
}

#[async_trait::async_trait]
impl EntryStream for AgentEntryStream {
  async fn next_entry(
    &mut self,
  ) -> std::io::Result<Option<(crate::fs::EntryMeta, Box<dyn tokio::io::AsyncRead + Send + Unpin>)>> {
    // 首次调用时加载文件列表
    if !self.loaded {
      self.load_files().await?;
    }

    // 返回下一个文件
    if self.idx >= self.files.len() {
      return Ok(None);
    }

    let path = std::mem::take(&mut self.files[self.idx]);
    self.idx += 1;

    // 下载文件内容
    let url = self.client.build_url("/api/v1/file_raw");
    let response = self
      .client
      .client
      .get(&url)
      .query(&[("path", &path)])
      .send()
      .await
      .map_err(|e| std::io::Error::other(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
      return Err(std::io::Error::other(format!(
        "Agent returned error: {}",
        response.status()
      )));
    }

    let bytes = response
      .bytes()
      .await
      .map_err(|e| std::io::Error::other(format!("Failed to read response body: {}", e)))?;

    let meta = crate::fs::EntryMeta {
      path,
      container_path: None,
      size: Some(bytes.len() as u64),
      source: crate::fs::EntrySource::File,
    };

    let reader: Box<dyn tokio::io::AsyncRead + Send + Unpin> = Box::new(MemoryReader::new(bytes.to_vec()));

    Ok(Some((meta, reader)))
  }
}

#[async_trait]
impl Streamable for AgentProxyFS {
  /// 获取条目流用于搜索
  async fn as_entry_stream(
    &self,
    path: &ResourcePath,
    _recursive: bool,
    _config: &SearchConfig,
  ) -> Result<Box<dyn EntryStream>, FsError> {
    let api_path = self.resource_path_to_api_path(path);

    // 创建 Agent 条目流
    let stream = AgentEntryStream::new(self.client.clone(), api_path);
    Ok(Box::new(stream))
  }

  /// Agent 支持流式搜索（通过 list_files + file_raw API）
  fn supports_streaming_search(&self) -> bool {
    true
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::dfs::filesystem::MemoryReader;

  #[test]
  fn test_agent_client_new() {
    let client = AgentClient::new("192.168.1.100".to_string(), 4001);
    assert!(client.is_ok());
    let client = client.unwrap();
    assert_eq!(client.host, "192.168.1.100");
    assert_eq!(client.port, 4001);
  }

  #[test]
  fn test_agent_client_build_url() {
    let client = AgentClient::new("example.com".to_string(), 8080).unwrap();
    assert_eq!(client.build_url("/api/test"), "http://example.com:8080/api/test");
  }

  #[test]
  fn test_agent_proxy_fs_new() {
    let client = AgentClient::new("localhost".to_string(), 4001).unwrap();
    let fs = AgentProxyFS::new(client);
    assert_eq!(fs.client.host, "localhost");
    assert_eq!(fs.client.port, 4001);
  }

  #[test]
  fn test_resource_path_to_api_path() {
    let client = AgentClient::new("localhost".to_string(), 4001).unwrap();
    let fs = AgentProxyFS::new(client);

    // 空路径
    let path = ResourcePath::parse("");
    assert_eq!(fs.resource_path_to_api_path(&path), "/");

    // 单级路径
    let path = ResourcePath::parse("file.txt");
    assert_eq!(fs.resource_path_to_api_path(&path), "/file.txt");

    // 多级路径
    let path = ResourcePath::parse("dir/file.txt");
    assert_eq!(fs.resource_path_to_api_path(&path), "/dir/file.txt");

    // 绝对路径
    let path = ResourcePath::parse("/var/log/app.log");
    assert_eq!(fs.resource_path_to_api_path(&path), "/var/log/app.log");
  }

  #[test]
  fn test_memory_reader() {
    let reader = MemoryReader::new(vec![1, 2, 3, 4, 5]);
    assert_eq!(reader.as_bytes().len(), 5);
    assert!(!reader.as_bytes().is_empty());
    assert_eq!(reader.as_bytes(), &[1, 2, 3, 4, 5]);
  }

  #[test]
  fn test_memory_reader_empty() {
    let reader = MemoryReader::new(vec![]);
    assert_eq!(reader.as_bytes().len(), 0);
    assert!(reader.as_bytes().is_empty());
  }
}
