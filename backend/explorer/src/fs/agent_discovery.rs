use agent_manager::AgentManager;
use async_trait::async_trait;
use opsbox_core::odfs::fs::{OpsFileSystem, OpsRead};
use opsbox_core::odfs::orl::OpsPath;
use opsbox_core::odfs::types::{OpsEntry, OpsFileType, OpsMetadata};
use std::io;
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
impl OpsFileSystem for AgentDiscoveryFileSystem {
  async fn metadata(&self, _path: &OpsPath) -> io::Result<OpsMetadata> {
    // 虚拟根目录 "/" 元数据
    Ok(OpsMetadata {
      name: "agent_root".to_string(),
      file_type: OpsFileType::Directory,
      size: 0,
      modified: None,
      mode: 0755,
      mime_type: None,
      compression: None,
      is_archive: false,
    })
  }

  async fn read_dir(&self, _path: &OpsPath) -> io::Result<Vec<OpsEntry>> {
    let agents = self.manager.list_online_agents().await;

    let entries = agents
      .into_iter()
      .map(|a| {
        let name = if a.name.is_empty() {
           a.id.clone()
        } else {
           format!("{} ({})", a.name, a.id)
        };

        let path = format!("orl://{}@agent/", a.id);

        let metadata = OpsMetadata {
          name: name.clone(),
          file_type: OpsFileType::Directory,
          size: 0,
          modified: if a.last_heartbeat > 0 {
             Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(a.last_heartbeat as u64))
          } else {
             None
          },
          mode: 0755,
          mime_type: None,
          compression: None,
          is_archive: false, // Agent is not an archive itself, it's a directory-like provider
        };

        OpsEntry {
          name,
          path,
          metadata,
        }
      })
      .collect();

    Ok(entries)
  }

  async fn open_read(&self, _path: &OpsPath) -> io::Result<OpsRead> {
    Err(io::Error::new(io::ErrorKind::PermissionDenied, "Cannot read agent list as file"))
  }

  fn name(&self) -> &str {
    "agent_discovery"
  }
}
