use crate::domain::{ResourceItem, ResourceType};
use crate::fs::{AgentDiscoveryFileSystem, S3DiscoveryFileSystem};
use opsbox_core::SqlitePool;
use opsbox_core::odfs::manager::OrlManager;
use opsbox_core::odfs::orl::{ORL, TargetType};
use opsbox_core::odfs::providers::LocalOpsFS;
use opsbox_core::odfs::types::OpsFileType;
use tokio::io::AsyncRead;

use agent_manager::AgentManager;
use futures_util::future::BoxFuture;
use opsbox_core::odfs::fs::OpsFileSystem;
use opsbox_core::odfs::manager::OpsFileSystemResolver;
use opsbox_core::odfs::providers::{AgentOpsFS, S3OpsFS};
use std::sync::Arc;

pub struct ExplorerService {
  orl_manager: Arc<OrlManager>,
  // 保留旧字段以支持遗留/动态功能
  db_pool: SqlitePool,
  agent_manager: Option<Arc<AgentManager>>,
}

impl ExplorerService {
  pub fn new(db_pool: SqlitePool) -> Self {
    let mut manager = OrlManager::new();

    // Register Default Providers
    manager.register("local".to_string(), Arc::new(LocalOpsFS::new(None)));
    manager.register(
      "s3.root".to_string(),
      Arc::new(S3DiscoveryFileSystem::new(db_pool.clone())),
    );

    // S3 Resolver
    let pool_clone = db_pool.clone();
    let s3_resolver: OpsFileSystemResolver = Box::new(
      move |key: String| -> BoxFuture<'static, Option<Arc<dyn OpsFileSystem>>> {
        let pool = pool_clone.clone();
        Box::pin(async move { Self::resolve_s3_static(&pool, &key).await })
      },
    );
    manager.set_resolver(s3_resolver);

    Self {
      orl_manager: Arc::new(manager),
      db_pool,
      agent_manager: None,
    }
  }

  pub fn with_agent_manager(mut self, manager: Arc<AgentManager>) -> Self {
    self.agent_manager = Some(manager.clone());

    // Combined Resolver (S3 + Agent)
    let pool = self.db_pool.clone();
    let am = manager.clone();

    let resolver: OpsFileSystemResolver = Box::new(
      move |key: String| -> BoxFuture<'static, Option<Arc<dyn OpsFileSystem>>> {
        let pool = pool.clone();
        let am = am.clone();
        Box::pin(async move {
          if let Some(fs) = Self::resolve_s3_static(&pool, &key).await {
            return Some(fs);
          }

          if key.starts_with("agent.") {
            let id_part = key.trim_start_matches("agent.");
            if let Some(agent) = am.get_agent(id_part).await {
              let base_url = agent.get_base_url();
              return Some(Arc::new(AgentOpsFS::new(id_part, base_url)) as Arc<dyn OpsFileSystem>);
            }
          }
          None
        })
      },
    );

    // Take ownership of the Arc, unwrap it, modify, and put it back
    let temp_arc = std::mem::replace(&mut self.orl_manager, Arc::new(OrlManager::new()));
    let mut orl_manager = match Arc::try_unwrap(temp_arc) {
      Ok(manager) => manager,
      Err(_) => panic!("OrlManager Arc should have only one reference"),
    };
    orl_manager.register(
      "agent.root".to_string(),
      Arc::new(AgentDiscoveryFileSystem::new(manager.clone())),
    );
    orl_manager.set_resolver(resolver);
    self.orl_manager = Arc::new(orl_manager);

    self
  }

  pub async fn list(&self, orl: &ORL) -> Result<Vec<ResourceItem>, String> {
    let mut use_orl = orl.clone();

    // Auto-detect archive
    if use_orl.target_type() != TargetType::Archive {
      let path_str = use_orl.path().to_lowercase();
      let is_archive_ext = path_str.ends_with(".tar")
        || path_str.ends_with(".tar.gz")
        || path_str.ends_with(".tgz")
        || path_str.ends_with(".gz")
        || path_str.ends_with(".zip");

      if is_archive_ext {
        // Reconstruct ORL with target=archive
        let base = use_orl.as_str();
        let separator = if base.contains('?') { "&" } else { "?" };
        let new_orl_str = format!("{}{}{}={}", base, separator, "target", "archive");
        if let Ok(new_orl) = ORL::parse(new_orl_str) {
          use_orl = new_orl;
        }
      }
    }

    self
      .orl_manager
      .read_dir(&use_orl)
      .await
      .map(|entries| entries.into_iter().map(|e| map_entry(e, &use_orl)).collect())
      .map_err(|e| e.to_string())
  }

  pub async fn download(&self, orl: &ORL) -> Result<(String, Option<u64>, Box<dyn AsyncRead + Send + Unpin>), String> {
    let meta = self.orl_manager.metadata(orl).await.map_err(|e| e.to_string())?;
    let reader = self.orl_manager.open_read(orl).await.map_err(|e| e.to_string())?;
    Ok((meta.name, Some(meta.size), Box::new(reader)))
  }

  // Static helper for S3 resolution to reduce duplication
  async fn resolve_s3_static(pool: &SqlitePool, key: &str) -> Option<Arc<dyn OpsFileSystem>> {
    if !key.starts_with("s3.") {
      return None;
    }
    let id_part = key.trim_start_matches("s3.");
    use opsbox_core::repository::s3::load_s3_profile;
    use opsbox_core::storage::s3::get_or_create_s3_client;

    if let Some((profile_name, bucket_name)) = id_part.split_once(':') {
      if let Ok(Some(profile)) = load_s3_profile(pool, profile_name).await
        && let Ok(client) = get_or_create_s3_client(&profile.endpoint, &profile.access_key, &profile.secret_key)
      {
        return Some(Arc::new(S3OpsFS::new((*client).clone(), bucket_name)) as Arc<dyn OpsFileSystem>);
      }
    } else if let Ok(Some(_)) = load_s3_profile(pool, id_part).await {
      return Some(Arc::new(S3DiscoveryFileSystem::new(pool.clone())) as Arc<dyn OpsFileSystem>);
    }
    None
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use opsbox_core::odfs::orl::ORL;
  use tokio::io::AsyncReadExt;

  #[tokio::test]
  async fn test_explorer_service_list_local_not_found() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let service = ExplorerService::new(pool);
    let orl = ORL::parse("orl://local/non/existent").unwrap();
    let result = service.list(&orl).await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_explorer_service_download_local() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let service = ExplorerService::new(pool);

    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    tokio::fs::write(&file_path, "hello download").await.unwrap();

    let encoded_path = urlencoding::encode(file_path.to_str().unwrap());
    let orl = ORL::parse(format!("orl://local/{}", encoded_path)).unwrap();

    let (_name, size, mut reader) = service.download(&orl).await.unwrap();
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
    builder.append_data(&mut header, "foo.txt", "test".as_bytes()).unwrap();

    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Directory);
    header.set_size(0);
    header.set_cksum();
    builder.append_data(&mut header, "bar/", &mut std::io::empty()).unwrap();
    builder.finish().unwrap();

    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let service = ExplorerService::new(pool);

    let encoded_path = urlencoding::encode(tar_path.to_str().unwrap());
    let orl = ORL::parse(format!("orl://local/{}", encoded_path)).unwrap();

    let items = service.list(&orl).await.unwrap();
    assert_eq!(items.len(), 2);

    let foo = items.iter().find(|i| i.name == "foo.txt").unwrap();
    assert!(foo.r#type == ResourceType::File);
    assert!(foo.path.contains("entry=foo.txt"));
  }

  #[tokio::test]
  async fn test_download_from_agent() {
    for key in &[
      "http_proxy",
      "https_proxy",
      "all_proxy",
      "HTTP_PROXY",
      "HTTPS_PROXY",
      "ALL_PROXY",
    ] {
      unsafe {
        std::env::remove_var(key);
      }
    }
    unsafe {
      std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
    }
    unsafe {
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

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
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

    let manager = std::sync::Arc::new(agent_manager::AgentManager::new(pool.clone()).await.unwrap());

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
    agent_info.tags.push(agent_manager::models::AgentTag::new(
      "listen_port".to_string(),
      port.to_string(),
    ));

    manager.register_agent(agent_info).await.unwrap();

    let service = ExplorerService::new(pool).with_agent_manager(manager);

    let orl = ORL::parse("orl://agent-dl@agent/tmp/file.txt").unwrap();

    let (name, _size, mut reader) = service.download(&orl).await.unwrap();
    assert_eq!(name, "file.txt");

    let mut content = String::new();
    reader.read_to_string(&mut content).await.unwrap();
    assert_eq!(content, "agent download content");
  }
}
fn map_entry(entry: opsbox_core::odfs::types::OpsEntry, parent_orl: &ORL) -> ResourceItem {
  let is_orl = entry.path.starts_with("orl://");

  let path = if is_orl {
    entry.path
  } else if parent_orl.target_type() == TargetType::Archive {
    let base = parent_orl.as_str().split('?').next().unwrap_or(parent_orl.as_str());

    // Ensure we don't double encode if entry.path is already encoded?
    // ArchiveOpsFS usually returns decoded/raw paths.
    let encoded_entry = urlencoding::encode(&entry.path);
    format!("{}?target=archive&entry={}", base, encoded_entry)
  } else {
    // Standard directory traversal
    if entry.path.starts_with('/') {
      // If the entry already provides an absolute path, use it with the same endpoint
      let auth = parent_orl.uri().authority().map(|a| a.as_str()).unwrap_or("local");
      let encoded_path = entry
        .path
        .split('/')
        .map(|s| urlencoding::encode(s).into_owned())
        .collect::<Vec<_>>()
        .join("/");
      format!("orl://{}{}", auth, encoded_path)
    } else {
      // Fallback to name-based joining: Append name to parent path
      // Remove query params from base
      let base = parent_orl.as_str().split('?').next().unwrap_or(parent_orl.as_str());
      let separator = if base.ends_with('/') { "" } else { "/" };
      let encoded_name = urlencoding::encode(&entry.name);
      format!("{}{}{}", base, separator, encoded_name)
    }
  };

  ResourceItem {
    name: entry.name,
    path,
    r#type: match entry.metadata.file_type {
      OpsFileType::Directory => ResourceType::Dir,
      OpsFileType::File => ResourceType::File,
      OpsFileType::Symlink => ResourceType::LinkFile,
      OpsFileType::Unknown => ResourceType::File,
    },
    size: Some(entry.metadata.size),
    modified: entry
      .metadata
      .modified
      .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64),
    has_children: if entry.metadata.is_dir() { Some(true) } else { None },
    child_count: None,
    hidden_child_count: None,
    mime_type: entry.metadata.mime_type,
  }
}

#[cfg(test)]
mod map_entry_tests {
  use super::*;
  use opsbox_core::odfs::types::{OpsEntry, OpsFileType, OpsMetadata};

  #[test]
  fn test_map_entry_file() {
    let entry = OpsEntry {
      name: "test.log".to_string(),
      path: "/var/log/test.log".to_string(),
      metadata: OpsMetadata {
        name: "test.log".to_string(),
        file_type: OpsFileType::File,
        size: 1024,
        modified: Some(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1234567890)),
        mode: 0o644,
        mime_type: Some("text/plain".to_string()),
        compression: None,
        is_archive: false,
      },
    };

    let orl = ORL::parse("orl://local/var/log").unwrap();
    let item = map_entry(entry, &orl);

    assert_eq!(item.name, "test.log");
    assert_eq!(item.r#type, ResourceType::File);
    assert_eq!(item.size, Some(1024));
    assert_eq!(item.mime_type, Some("text/plain".to_string()));
  }

  #[test]
  fn test_map_entry_directory() {
    let entry = OpsEntry {
      name: "logs".to_string(),
      path: "/var/logs".to_string(),
      metadata: OpsMetadata {
        name: "logs".to_string(),
        file_type: OpsFileType::Directory,
        size: 0,
        modified: None,
        mode: 0o755,
        mime_type: None,
        compression: None,
        is_archive: false,
      },
    };

    let orl = ORL::parse("orl://local/var").unwrap();
    let item = map_entry(entry, &orl);

    assert_eq!(item.name, "logs");
    assert_eq!(item.r#type, ResourceType::Dir);
    assert_eq!(item.has_children, Some(true));
  }
}
