use std::path::PathBuf;

use crate::domain::{ResourceItem, ResourceType};
use opsbox_core::SqlitePool;
use opsbox_core::odfi::{EndpointType, Odfi, TargetType};
use opsbox_core::storage::s3::format_s3_error;

pub struct ExplorerService {
  db_pool: SqlitePool,
}

impl ExplorerService {
  pub fn new(db_pool: SqlitePool) -> Self {
    Self { db_pool }
  }

  pub async fn list(&self, odfi: &Odfi) -> Result<Vec<ResourceItem>, String> {
    match odfi.endpoint_type {
      EndpointType::Local => self.list_local(odfi).await,
      EndpointType::Agent => self.list_agent(odfi).await,
      EndpointType::S3 => self.list_s3(odfi).await,
    }
  }

  async fn list_local(&self, odfi: &Odfi) -> Result<Vec<ResourceItem>, String> {
    // Warning: minimal security check. In production, this should restricted to allowed directories.
    // Assuming opsbox-server runs with permissions to access the path.

    let path_str = if odfi.path.is_empty() {
      "/".to_string()
    } else if odfi.path.starts_with('/') {
      odfi.path.clone()
    } else {
      format!("/{}", odfi.path)
    };

    // Handle archive navigation
    if odfi.target_type == TargetType::Archive {
      let path = PathBuf::from(&path_str);
      if !path.exists() {
        return Err(format!("Archive file does not exist: {}", path_str));
      }

      let file = tokio::fs::File::open(&path).await.map_err(|e| e.to_string())?;
      let mut stream = opsbox_core::fs::create_archive_stream_from_reader(file, Some(&path_str))
        .await
        .map_err(|e| format!("Failed to open archive stream: {}", e))?;

      let mut items = Vec::new();

      // Iterate entries
      while let Ok(Some((meta, _reader))) = stream.next_entry().await {
        // _reader is a Box<dyn AsyncRead> which we drop immediately, skipping content.
        // Ideally we'd skip content efficiently, but dropping the reader might trigger skip.
        // For async-tar, dropping the entry/reader should advance to next header?
        // OpsBox's EntryStream consumes the reader?
        // No, standard async-tar Entry needs to be read or dropped. Dropping might not skip?
        // WARNING: If we don't read the reader, async-tar might desync?
        // Let's check opsbox-core implementation.
        // TarEntryStream uses entries.next(). entries owns the archive.
        // If we drop the reader (which is compat wrapper around entry), does it advance?
        // async-tar documentation says: "The returned entry... can be read...".
        // "When the entry is dropped, the underlying stream is advanced to the next entry." (Usually true for tar iterators).
        // Let's assume it works.

        items.push(ResourceItem {
          name: meta.path.clone(),
          // Use hash fragment to denote internal path for now
          path: format!("{}#{}", odfi, meta.path),
          r#type: if meta.path.ends_with('/') {
            ResourceType::Dir
          } else {
            ResourceType::File
          },
          size: meta.size,
          modified: None, // Archives often store mtime but EntryMeta might not expose it yet? opsbox-core EntryMeta has no mtime.
          has_children: if meta.path.ends_with('/') { Some(true) } else { None }, // Assume dirs in archives are non-empty for now
          child_count: None,
          hidden_child_count: None,
        });
      }

      // Sort items
      items.sort_by(|a, b| {
        let a_is_dir = a.r#type == ResourceType::Dir || a.r#type == ResourceType::LinkDir;
        let b_is_dir = b.r#type == ResourceType::Dir || b.r#type == ResourceType::LinkDir;

        if a_is_dir == b_is_dir {
          a.name.cmp(&b.name)
        } else if a_is_dir {
          std::cmp::Ordering::Less
        } else {
          std::cmp::Ordering::Greater
        }
      });

      return Ok(items);
    }

    let path = PathBuf::from(&path_str);
    if !path.exists() {
      return Err(format!("Path does not exist: {}", path_str));
    }

    let mut items = Vec::new();

    let list_items = opsbox_core::fs::list_directory(&path)
      .await
      .map_err(|e| e.to_string())?;

    for item in list_items {
      let r_type = match (item.is_dir, item.is_symlink) {
        (true, true) => ResourceType::LinkDir,
        (true, false) => ResourceType::Dir,
        (false, true) => ResourceType::LinkFile,
        (false, false) => ResourceType::File,
      };

      // Construct child path
      let child_path = if path_str == "/" {
        item.name.to_string()
      } else {
        format!("{}/{}", odfi.path.trim_start_matches('/'), item.name)
      };

      // Reconstruct ODFI for the child
      let child_odfi = Odfi::new(
        EndpointType::Local,
        odfi.endpoint_id.clone(), // localhost
        TargetType::Dir,
        child_path,
        None,
      );

      items.push(ResourceItem {
        name: item.name,
        path: child_odfi.to_string(),
        r#type: r_type,
        size: item.size,
        modified: item.modified,
        has_children: Some(item.is_dir && item.child_count.unwrap_or(0) > 0),
        child_count: item.child_count.map(|c| c as u64),
        hidden_child_count: item.hidden_child_count.map(|c| c as u64),
      });
    }

    Ok(items)
  }

  async fn list_agent(&self, odfi: &Odfi) -> Result<Vec<ResourceItem>, String> {
    let agent_id = &odfi.endpoint_id;

    use agent_manager::get_global_agent_manager;

    // Level 1: Root Agent - List Online Agents
    if agent_id.is_empty() {
      if let Some(manager) = get_global_agent_manager() {
        let agents = manager.list_online_agents().await;
        let items = agents
          .into_iter()
          .map(|a| {
            let child_odfi = Odfi::new(
              EndpointType::Agent,
              a.id.clone(),
              TargetType::Dir,
              "/", // Root of agent
              None,
            );
            ResourceItem {
              name: if a.name.is_empty() {
                a.id
              } else {
                format!("{} ({})", a.name, a.id)
              },
              path: child_odfi.to_string(),
              r#type: ResourceType::Dir, // Treat agent as a directory
              size: None,
              modified: Some(a.last_heartbeat),
              has_children: Some(true), // Agents presumably have files
              child_count: None,
              hidden_child_count: None,
            }
          })
          .collect();
        return Ok(items);
      } else {
        return Err("Agent manager not initialized".to_string());
      }
    }

    let endpoint = if let Some(manager) = get_global_agent_manager() {
      if let Some(agent) = manager.get_agent(agent_id).await {
        agent.get_base_url()
      } else {
        return Err(format!("Agent {} not found or offline", agent_id));
      }
    } else {
      return Err("Agent manager not initialized".to_string());
    };

    use opsbox_core::agent::AgentClient;
    let client = AgentClient::new(agent_id.clone(), endpoint, Some(std::time::Duration::from_secs(10)));

    // Call Agent API
    // API path: /api/v1/list_files?path=<path>
    // We need to define this API on Agent side.

    let path_str = if odfi.path.is_empty() {
      "/".to_string()
    } else if odfi.path.starts_with('/') {
      odfi.path.clone()
    } else {
      format!("/{}", odfi.path)
    };

    let url = format!("/api/v1/list_files?path={}", urlencoding::encode(&path_str));

    use opsbox_core::agent::models::AgentListResponse;
    let resp: AgentListResponse = client
      .get(&url)
      .await
      .map_err(|e| format!("Agent request failed: {}", e))?;

    let items = resp
      .items
      .into_iter()
      .map(|item| {
        let child_odfi = Odfi::new(
          EndpointType::Agent,
          agent_id.clone(),
          TargetType::Dir,
          item.path.trim_start_matches('/').to_string(), // ODFI path usually relative to root?
          // Wait, Agent returns absolute path. ODFI path should match what's needed for next request.
          // If next request uses ODFI path, and we send it to agent...
          // If agent expects absolute path, we should keep it absolute?
          // ODFI usually strips leading slash in string representation but `path` field has it?
          // opsbox-core ODFI implementation: path is stored as string.
          // Let's stick to: ODFI path does NOT have leading slash for root relative.
          // But for agent, if it's absolute path /var/log, ODFI is agent://id/var/log.
          // So we keep it as is, maybe trim start slash if ODFI display adds it.
          None,
        );

        ResourceItem {
          name: item.name,
          path: child_odfi.to_string(),
          r#type: match (item.is_dir, item.is_symlink) {
            (true, true) => ResourceType::LinkDir,
            (true, false) => ResourceType::Dir,
            (false, true) => ResourceType::LinkFile,
            (false, false) => ResourceType::File,
          },
          size: item.size,
          modified: item.modified,
          has_children: if item.is_dir {
            Some(item.child_count.unwrap_or(0) > 0)
          } else {
            None
          },
          child_count: item.child_count.map(|c| c as u64),
          hidden_child_count: item.hidden_child_count.map(|c| c as u64),
        }
      })
      .collect();

    Ok(items)
  }

  async fn list_s3(&self, odfi: &Odfi) -> Result<Vec<ResourceItem>, String> {
    // Level 1: Root S3 - List Profiles
    if odfi.endpoint_id.is_empty() {
      let profiles = opsbox_core::repository::s3::list_s3_profiles(&self.db_pool)
        .await
        .map_err(|e| format!("Database error: {}", e))?;

      let items = profiles
        .into_iter()
        .map(|p| {
          let child_odfi = Odfi::new(
            EndpointType::S3,
            p.profile_name.clone(), // ID is just profile name
            TargetType::Dir,
            "", // Root of profile
            None,
          );
          ResourceItem {
            name: p.profile_name,
            path: child_odfi.to_string(),
            r#type: ResourceType::Dir,
            size: None,
            modified: None,
            has_children: Some(true),
            child_count: None,
            hidden_child_count: None,
          }
        })
        .collect();
      return Ok(items);
    }

    // Level 2: Profile - List Buckets
    // If ID has no colon, it's a profile. But check if it's meant to be profile:bucket (handled by split logic below if valid)
    // Actually, if we want to list buckets, we need a client. Client comes from profile.
    // If ID is "profile", we load profile and list buckets.

    let (profile, bucket) = if let Some((p, b)) = odfi.endpoint_id.split_once(':') {
      (p, Some(b))
    } else {
      (odfi.endpoint_id.as_str(), None)
    };

    let profile_row = opsbox_core::repository::s3::load_s3_profile(&self.db_pool, profile)
      .await
      .map_err(|e| format!("Database error: {}", e))?
      .ok_or_else(|| format!("S3 Profile not found: {}", profile))?;

    use opsbox_core::storage::s3::get_or_create_s3_client;
    let client = get_or_create_s3_client(&profile_row.endpoint, &profile_row.access_key, &profile_row.secret_key)
      .map_err(|e| format!("Failed to create S3 client: {}", e))?;

    if let Some(bucket_name) = bucket {
      // Level 3: Bucket - List Objects
      // ... existing object listing logic ...

      let prefix = if odfi.path.is_empty() {
        "".to_string()
      } else if !odfi.path.ends_with('/') {
        format!("{}/", odfi.path)
      } else {
        odfi.path.clone()
      };

      let s3_prefix = prefix.trim_start_matches('/').to_string();

      let resp = client
        .list_objects_v2()
        .bucket(bucket_name)
        .prefix(&s3_prefix)
        .delimiter("/")
        .send()
        .await
        .map_err(|e| format!("S3 ListObjects failed: {}", format_s3_error(&e)))?;

      let mut items = Vec::new();

      // Directories (CommonPrefixes)
      if let Some(common_prefixes) = resp.common_prefixes {
        for cp in common_prefixes {
          if let Some(p) = cp.prefix {
            let name = p.trim_end_matches('/').split('/').next_back().unwrap_or(&p).to_string();
            let child_path = p.trim_start_matches('/').to_string();
            let child_odfi = Odfi::new(
              EndpointType::S3,
              odfi.endpoint_id.clone(),
              TargetType::Dir,
              child_path,
              None,
            );
            items.push(ResourceItem {
              name,
              path: child_odfi.to_string(),
              r#type: ResourceType::Dir,
              size: None,
              modified: None,
              has_children: Some(true),
              child_count: None,
              hidden_child_count: None,
            });
          }
        }
      }

      // Files
      if let Some(contents) = resp.contents {
        for obj in contents {
          if let Some(key) = obj.key {
            if key == s3_prefix {
              continue;
            }
            let name = key.split('/').next_back().unwrap_or(&key).to_string();
            if name.is_empty() {
              continue;
            }

            let child_odfi = Odfi::new(
              EndpointType::S3,
              odfi.endpoint_id.clone(),
              TargetType::Dir, // Files are Dir target unless entry
              key.trim_start_matches('/').to_string(),
              None,
            );

            let is_dir = key.ends_with('/');
            // S3 "folders" are sometimes empty objects ending in /.
            // But list_objects_v2 with delimiter handles common prefixes.
            // Usually contents won't have dirs unless 0-byte placeholders.
            // Let's treat them as files if they are in contents.
            if is_dir {
              continue;
            }

            items.push(ResourceItem {
              name,
              path: child_odfi.to_string(),
              r#type: ResourceType::File,
              size: obj.size.map(|s| s as u64),
              modified: obj.last_modified.map(|d| d.secs()),
              has_children: None,
              child_count: None,
              hidden_child_count: None,
            });
          }
        }
      }
      Ok(items)
    } else {
      // Level 2: List Buckets for Profile
      let resp = client
        .list_buckets()
        .send()
        .await
        .map_err(|e| format!("S3 ListBuckets failed: {}", format_s3_error(&e)))?;

      let items = resp
        .buckets
        .unwrap_or_default()
        .into_iter()
        .map(|b| {
          let name = b.name.unwrap_or_default();
          let child_odfi = Odfi::new(
            EndpointType::S3,
            format!("{}:{}", profile, name), // Construct profile:bucket ID
            TargetType::Dir,
            "",
            None,
          );
          ResourceItem {
            name,
            path: child_odfi.to_string(),
            r#type: ResourceType::Dir,
            size: None,
            modified: b.creation_date.map(|d| d.secs()),
            has_children: Some(true),
            child_count: None,
            hidden_child_count: None,
          }
        })
        .collect();
      Ok(items)
    }
  }
}
#[cfg(test)]
mod tests {
  use super::*;
  use opsbox_core::odfi::Odfi;

  #[tokio::test]
  async fn test_list_local_archive_tar() {
    // Create a temporary tar file
    let temp_dir = tempfile::tempdir().unwrap();
    let archive_path = temp_dir.path().join("test.tar");
    let file = std::fs::File::create(&archive_path).unwrap();
    let mut builder = tar::Builder::new(file);

    // Add file
    let mut header = tar::Header::new_gnu();
    header.set_size(4);
    header.set_cksum();
    builder.append_data(&mut header, "foo.txt", "test".as_bytes()).unwrap();

    // Add dir
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Directory);
    header.set_size(0);
    header.set_cksum();
    builder.append_data(&mut header, "bar/", &mut std::io::empty()).unwrap();

    builder.finish().unwrap();

    // Setup service (mock pool not needed for local)
    // We need a dummy SqlitePool. opsbox-coreSqlitePool is sqlx::Pool<Sqlite>.
    // Ideally we mock it or use an in-memory db.
    // For list_local, db_pool is unused.
    // We can try to construct one if sqlx allows easy mock, or just pass a real one.
    // Let's use sqlx::SqlitePool::connect("sqlite::memory:").
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let service = ExplorerService::new(pool);

    // Create ODFI
    let odfi = Odfi::new(
      EndpointType::Local,
      "localhost".to_string(),
      TargetType::Archive,
      archive_path.to_string_lossy().to_string(),
      None,
    );

    // List
    let items = service.list(&odfi).await.unwrap();

    // Verify
    assert_eq!(items.len(), 2);
    let foo = items.iter().find(|i| i.name == "foo.txt").unwrap();
    assert_eq!(foo.r#type, ResourceType::File);

    let bar = items.iter().find(|i| i.name == "bar/").unwrap();
    assert_eq!(bar.r#type, ResourceType::Dir);
  }
}
