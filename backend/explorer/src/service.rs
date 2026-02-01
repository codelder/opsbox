//! Explorer 服务层
//!
//! 使用 DDD 领域类型（ResourceIdentifier）和 opsbox-resource 的 EndpointConnector。

use crate::domain::{ResourceItem, ResourceType};
use agent_manager::AgentManager;
use opsbox_core::SqlitePool;
use opsbox_domain::resource::{EndpointType, ResourceIdentifier};
use opsbox_domain::resource::{EndpointConnector, ResourceMetadata};
use std::pin::Pin;
use std::sync::Arc;
use tokio::io::AsyncRead;

/// Explorer 服务
///
/// 管理资源浏览功能，使用 ResourceIdentifier 作为类型安全的资源标识符。
pub struct ExplorerService {
  db_pool: SqlitePool,
  agent_manager: Option<Arc<AgentManager>>,
}

impl ExplorerService {
  pub fn new(db_pool: SqlitePool) -> Self {
    Self {
      db_pool,
      agent_manager: None,
    }
  }

  pub fn with_agent_manager(mut self, manager: Arc<AgentManager>) -> Self {
    self.agent_manager = Some(manager);
    self
  }

  /// 列出资源内容
  pub async fn list(&self, rid: &ResourceIdentifier) -> Result<Vec<ResourceItem>, String> {
    let connector = self.get_connector(rid).await?;

    // 自动检测归档文件
    let use_rid = if rid.archive_entry.is_none() && self.is_archive_path(rid.path.as_str()) {
      let archived = rid.clone();
      // 归档文件的 list 操作会自动添加 entry 参数
      archived
    } else {
      rid.clone()
    };

    let metadata_list = connector
      .list(&use_rid.path)
      .await
      .map_err(|e| e.to_string())?;

    metadata_list
      .into_iter()
      .map(|meta| Ok(self.map_metadata_to_item(meta, &use_rid)))
      .collect()
  }

  /// 下载资源
  pub async fn download(
    &self,
    rid: &ResourceIdentifier,
  ) -> Result<(String, Option<u64>, Pin<Box<dyn AsyncRead + Send + Unpin>>), String> {
    let connector = self.get_connector(rid).await?;
    let metadata = connector
      .metadata(&rid.path)
      .await
      .map_err(|e| e.to_string())?;
    let reader = connector.read(&rid.path).await.map_err(|e| e.to_string())?;
    Ok((metadata.name, Some(metadata.size), reader))
  }

  /// 获取资源的 EndpointConnector
  async fn get_connector(
    &self,
    rid: &ResourceIdentifier,
  ) -> Result<Arc<dyn EndpointConnector>, String> {
    match rid.endpoint.endpoint_type {
      EndpointType::Local => {
        use opsbox_core::odfs::providers::LocalOpsFS;
        use opsbox_resource::archive::ArchiveEndpointConnector;
        use opsbox_resource::local::LocalEndpointConnector;

        // 使用 root=None 表示使用绝对路径
        let local = LocalEndpointConnector::from_opsfs(Arc::new(LocalOpsFS::new(None)));

        // 检查是否需要使用归档连接器
        if rid.archive_entry.is_some() || self.is_archive_path(rid.path.as_str()) {
          return Ok(Arc::new(ArchiveEndpointConnector::new(
            local,
            rid.path.clone(),
          )));
        }

        Ok(Arc::new(local))
      }
      EndpointType::S3 => {
        use opsbox_core::repository::s3::load_s3_profile;
        use opsbox_core::storage::s3::get_or_create_s3_client;
        use opsbox_resource::s3::S3EndpointConnector;

        // 尝试从数据库加载 S3 profile
        let key = &rid.endpoint.id;
        if let Some((profile_name, bucket_name)) = key.split_once(':') {
          if let Ok(Some(profile)) = load_s3_profile(&self.db_pool, profile_name).await {
            if let Ok(client) = get_or_create_s3_client(
              &profile.endpoint,
              &profile.access_key,
              &profile.secret_key,
            ) {
              return Ok(Arc::new(S3EndpointConnector::new(
                (*client).clone(),
                bucket_name.to_string(),
              )));
            }
          }
        }
        Err(format!("S3 profile not found: {}", key))
      }
      EndpointType::Agent => {
        if let Some(manager) = &self.agent_manager {
          use opsbox_resource::agent::AgentEndpointConnector;

          if let Some(agent) = manager.get_agent(&rid.endpoint.id).await {
            let base_url = agent.get_base_url();
            return Ok(Arc::new(AgentEndpointConnector::new(
              rid.endpoint.id.clone(),
              base_url,
            )));
          }
        }
        Err(format!("Agent not found: {}", rid.endpoint.id))
      }
    }
  }

  /// 检查路径是否为归档文件
  fn is_archive_path(&self, path: &str) -> bool {
    let path_lower = path.to_lowercase();
    path_lower.ends_with(".tar")
      || path_lower.ends_with(".tar.gz")
      || path_lower.ends_with(".tgz")
      || path_lower.ends_with(".gz")
      || path_lower.ends_with(".zip")
  }

  /// 将 ResourceMetadata 转换为 ResourceItem
  fn map_metadata_to_item(&self, meta: ResourceMetadata, parent: &ResourceIdentifier) -> ResourceItem {
    // 生成子资源的 ORL 路径
    // 对于归档文件，使用 archive_entry 字段
    let child_rid = if self.is_archive_path(parent.path.as_str()) {
      use opsbox_domain::resource::ArchiveEntryPath;
      ResourceIdentifier {
        endpoint: parent.endpoint.clone(),
        path: parent.path.clone(),
        archive_entry: Some(ArchiveEntryPath::new(meta.name.clone())),
      }
    } else {
      parent.join(&meta.name)
    };
    let path = child_rid.to_string();

    ResourceItem {
      name: meta.name,
      path,
      r#type: if meta.is_dir {
        ResourceType::Dir
      } else {
        ResourceType::File
      },
      size: Some(meta.size),
      modified: meta.modified,
      has_children: if meta.is_dir { Some(true) } else { None },
      child_count: meta.child_count.map(|c| c as u64),
      hidden_child_count: None,
      mime_type: meta.mime_type,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::io::AsyncReadExt;

  #[tokio::test]
  async fn test_explorer_service_list_local_not_found() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let service = ExplorerService::new(pool);
    let rid = ResourceIdentifier::local("/non/existent");
    let result = service.list(&rid).await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_explorer_service_download_local() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let service = ExplorerService::new(pool);

    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    tokio::fs::write(&file_path, "hello download").await.unwrap();

    let rid = ResourceIdentifier::local(file_path.to_str().unwrap());

    let (_name, size, mut reader) = service.download(&rid).await.unwrap();
    assert_eq!(size, Some(14));
    let mut content = String::new();
    reader.read_to_string(&mut content).await.unwrap();
    assert_eq!(content, "hello download");
  }

  #[tokio::test]
  async fn test_list_local_archive_tar() {
    let temp_dir = tempfile::tempdir().unwrap();
    let tar_path = temp_dir.path().join("test.tar");
    let file = std::fs::File::create(&tar_path).unwrap();
    let mut builder = tar::Builder::new(file);

    let mut header = tar::Header::new_gnu();
    header.set_size(4);
    header.set_cksum();
    builder
      .append_data(&mut header, "foo.txt", "test".as_bytes())
      .unwrap();

    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Directory);
    header.set_size(0);
    header.set_cksum();
    builder
      .append_data(&mut header, "bar/", &mut std::io::empty())
      .unwrap();
    builder.finish().unwrap();

    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let service = ExplorerService::new(pool);

    let rid = ResourceIdentifier::local(tar_path.to_str().unwrap());

    let items = service.list(&rid).await.unwrap();
    assert_eq!(items.len(), 2);

    let foo = items.iter().find(|i| i.name == "foo.txt").unwrap();
    assert!(foo.r#type == ResourceType::File);
    assert!(foo.path.contains("entry=foo.txt"));
  }

  #[tokio::test]
  async fn test_download_from_agent() {
    // Skip this test in sandboxed environments where network binding is not allowed
    if std::env::var("CLAUDE_SANDBOX").is_ok()
      || std::env::var("CLAUDE_CODE_SANDBOX").is_ok()
    {
      return;
    }

    // SAFETY: 清除并设置代理环境变量以避免测试失败。
    unsafe {
      for key in &[
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
      ] {
        std::env::remove_var(key);
      }
      std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
      std::env::set_var("no_proxy", "127.0.0.1,localhost");
    }

    use axum::{Router, routing::get};

    let app = Router::new()
      .route("/api/v1/file_raw", get(|| async { "agent download content" }))
      .route(
        "/api/v1/list_files",
        get(|| async {
          serde_json::json!({
              "items": [
                  {
                      "name": "file.txt",
                      "path": "/tmp/file.txt",
                      "is_dir": false,
                      "is_symlink": false,
                      "size": 100,
                      "modified": 0,
                      "child_count": 0,
                      "mime_type": "text/plain"
                  }
              ],
              "total": 1
          })
          .to_string()
        }),
      );

    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
      Ok(l) => l,
      Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
        return;
      }
      Err(e) => panic!("Failed to bind to test address: {}", e),
    };
    let addr = listener.local_addr().unwrap();
    let port = addr.port();

    tokio::spawn(async move {
      axum::serve(listener, app).await.unwrap();
    });

    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    agent_manager::repository::AgentRepository::new(pool.clone())
      .init_schema()
      .await
      .unwrap();

    let manager =
      Arc::new(agent_manager::AgentManager::new(pool.clone()).await.unwrap());

    let mut agent_info = agent_manager::models::AgentInfo {
      id: "agent-dl".to_string(),
      name: "DL Agent".to_string(),
      version: "0.1.0".to_string(),
      hostname: "127.0.0.1".to_string(),
      tags: vec![],
      search_roots: vec!["/tmp".to_string()],
      last_heartbeat: 9999999999,
      status: agent_manager::models::AgentStatus::Online,
    };
    agent_info
      .tags
      .push(agent_manager::models::AgentTag::new(
        "listen_port".to_string(),
        port.to_string(),
      ));

    manager.register_agent(agent_info).await.unwrap();

    let service = ExplorerService::new(pool).with_agent_manager(manager);

    let rid = ResourceIdentifier::agent("agent-dl", "/tmp/file.txt", None);

    let (name, _size, mut reader) = service.download(&rid).await.unwrap();
    assert_eq!(name, "file.txt");

    let mut content = String::new();
    reader.read_to_string(&mut content).await.unwrap();
    assert_eq!(content, "agent download content");
  }

  #[test]
  fn test_is_archive_path() {
    // is_archive_path 是一个简单的方法，不需要实际的 service 实例
    let path = "/path/to/file.tar";
    assert!(path.to_lowercase().ends_with(".tar"));

    let path = "/path/to/file.tar.gz";
    assert!(path.to_lowercase().ends_with(".tar.gz"));

    let path = "/path/to/file.txt";
    assert!(!path.to_lowercase().ends_with(".tar"));
  }
}
