//! Agent 发现文件系统
//!
//! 提供在线 Agent 的虚拟目录视图，使用 DFS 抽象

use agent_manager::AgentManager;
use async_trait::async_trait;
use opsbox_core::dfs::{
    filesystem::{DirEntry, FileMetadata, FsError, OpbxFileSystem},
    path::ResourcePath,
};
use opsbox_core::fs::EntryStream;
use std::pin::Pin;
use std::sync::Arc;

/// Agent 发现文件系统
/// 提供在线 Agent 的虚拟目录视图
pub struct AgentDiscoveryFileSystem {
  manager: Arc<AgentManager>,
}

impl AgentDiscoveryFileSystem {
  pub fn new(manager: Arc<AgentManager>) -> Self {
    Self { manager }
  }
}

#[async_trait]
impl OpbxFileSystem for AgentDiscoveryFileSystem {
  /// 获取虚拟根目录的元数据
  async fn metadata(&self, _path: &ResourcePath) -> Result<FileMetadata, opsbox_core::dfs::FsError> {
    Ok(FileMetadata::dir(0))
  }

  /// 列出所有在线 Agent
  async fn read_dir(&self, _path: &ResourcePath) -> Result<Vec<DirEntry>, opsbox_core::dfs::FsError> {
    let agents = self.manager.list_online_agents().await;

    tracing::info!(
      "AgentDiscoveryFileSystem::read_dir: found {} online agents",
      agents.len()
    );

    let entries = agents
      .into_iter()
      .map(|a| {
        let name = if a.name.is_empty() {
          a.id.clone()
        } else {
          format!("{} ({})", a.name, a.id)
        };

        // Agent discovery 条目的路径不重要，重要的是名称
        // map_entry 会从名称中提取 agent ID 并构造正确的 ORL
        let path = ResourcePath::from_str("/");

        DirEntry {
          name: name.clone(),
          path,
          metadata: FileMetadata::dir(0),
        }
      })
      .collect();

    Ok(entries)
  }

  /// 不支持读取 agent 列表作为文件
  async fn open_read(
    &self,
    _path: &ResourcePath,
  ) -> Result<Pin<Box<dyn tokio::io::AsyncRead + Send + Unpin>>, opsbox_core::dfs::FsError> {
    Err(opsbox_core::dfs::FsError::InvalidConfig(
      "Cannot read agent list as file".to_string(),
    ))
  }

  /// 不支持条目流（虚拟目录）
  async fn as_entry_stream(
    &self,
    _path: &ResourcePath,
    _recursive: bool,
  ) -> Result<Box<dyn EntryStream>, FsError> {
    Err(FsError::InvalidConfig(
      "AgentDiscoveryFileSystem does not support entry streaming".to_string(),
    ))
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn test_agent_discovery_new() {
    // 这是一个基础的单元测试，实际的集成测试需要 AgentManager
    // 这里只测试类型系统的正确性
  }
}
