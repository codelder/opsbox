use std::path::PathBuf;

use crate::domain::{ResourceItem, ResourceType};
use futures_util::TryStreamExt;
use opsbox_core::SqlitePool;
use opsbox_core::odfi::{EndpointType, Odfi, TargetType};
use opsbox_core::storage::s3::format_s3_error;
use tokio_util::io::StreamReader;

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

    let mut is_archive_target = odfi.target_type == TargetType::Archive;

    // Auto-detect archive if pointing to a local file
    if !is_archive_target {
      let path = PathBuf::from(&path_str);
      if path.is_file() {
        // Simple extension check to decide if we should treat as archive navigation
        let lower = path_str.to_lowercase();
        if lower.ends_with(".tar") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") || lower.ends_with(".gz") {
          is_archive_target = true;
        }
      }
    }

    // Handle archive navigation
    if is_archive_target {
      let path = PathBuf::from(&path_str);
      if !path.exists() {
        return Err(format!("Archive file does not exist: {}", path_str));
      }

      let file = tokio::fs::File::open(&path).await.map_err(|e| e.to_string())?;
      let mut stream = opsbox_core::fs::create_archive_stream_from_reader(file, Some(&path_str))
        .await
        .map_err(|e| format!("Failed to open archive stream: {}", e))?;

      let mut items = Vec::new();

      let entry_prefix = odfi.entry_path.clone().unwrap_or_default();
      // Ensure prefix ends with / if not empty to match directories correctly
      let filter_prefix = if entry_prefix.is_empty() {
        "".to_string()
      } else if entry_prefix.ends_with('/') {
        entry_prefix.clone()
      } else {
        format!("{}/", entry_prefix)
      };

      let mut synthetic_dirs = std::collections::HashSet::new();

      // Iterate entries
      while let Ok(Some((meta, _reader))) = stream.next_entry().await {
        let path = meta.path.clone();

        if !path.starts_with(&filter_prefix) {
          continue;
        }

        // Get relative path
        let rel_path = &path[filter_prefix.len()..];
        if rel_path.is_empty() {
          continue; // Directory itself
        }

        // Check if it's a direct child or subdirectory
        let parts: Vec<&str> = rel_path.splitn(2, '/').collect();
        // If it has a slash (parts > 1) OR it ends with slash (parts=1 but split result might vary depending on trailing slash, safer to check logic)
        // If "subdir/file", parts=["subdir", "file"]
        // If "subdir/", parts=["subdir", ""]

        let is_subdir = parts.len() > 1;

        if is_subdir {
          let dir_name = parts[0];
          if synthetic_dirs.contains(dir_name) {
            continue;
          }
          synthetic_dirs.insert(dir_name.to_string());

          let mut child_odfi = odfi.clone();
          let child_entry = format!("{}{}/", filter_prefix, dir_name);
          child_odfi.entry_path = Some(child_entry);
          child_odfi.target_type = TargetType::Archive;

          items.push(ResourceItem {
            name: dir_name.to_string(),
            path: child_odfi.to_string(),
            r#type: ResourceType::Dir,
            size: None,
            modified: None,
            has_children: Some(true),
            child_count: None,
            hidden_child_count: None,
            mime_type: None,
          });
        } else {
          // Direct file
          let mut child_odfi = odfi.clone();
          child_odfi.entry_path = Some(path.clone());
          child_odfi.target_type = TargetType::Archive;

          items.push(ResourceItem {
            name: rel_path.to_string(),
            path: child_odfi.to_string(),
            r#type: ResourceType::File,
            size: meta.size,
            modified: None,
            has_children: None,
            child_count: None,
            hidden_child_count: None,
            mime_type: None,
          });
        }
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
        mime_type: item.mime_type,
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
              mime_type: None,
            }
          })
          .collect();
        return Ok(items);
      } else {
        return Err("Agent manager not initialized".to_string());
      }
    }

    let (agent, endpoint) = if let Some(manager) = get_global_agent_manager() {
      if let Some(agent) = manager.get_agent(agent_id).await {
        (agent.clone(), agent.get_base_url())
      } else {
        return Err(format!("Agent {} not found or offline", agent_id));
      }
    } else {
      return Err("Agent manager not initialized".to_string());
    };

    use opsbox_core::agent::AgentClient;
    // Note: timeout is optional
    let client = AgentClient::new(agent_id.clone(), endpoint, Some(std::time::Duration::from_secs(10)));

    let odfi_path = odfi.path.clone();
    let odfi_entry = odfi.entry_path.clone();
    tracing::debug!(
      "list_agent: agent_id={}, odfi.path={}, odfi.entry_path={:?}, odfi.target_type={:?}",
      agent_id,
      odfi_path,
      odfi_entry,
      odfi.target_type
    );

    let path_str = if odfi_path.is_empty() {
      "/".to_string()
    } else if odfi_path.starts_with('/') {
      odfi_path.clone()
    } else {
      format!("/{}", odfi_path)
    };
    tracing::debug!("list_agent: path_str={}", path_str);

    // If listing root of the agent, return search roots instead of calling agent list API
    // (Agent list API might fail if / is not in whitelist)
    if path_str == "/" {
      let items = agent
        .search_roots
        .into_iter()
        .map(|root| {
          // Ensure root has leading slash for ODFI consistency if needed, but usually search_roots are absolute
          let name = root.clone();
          let child_odfi = Odfi::new(
            EndpointType::Agent,
            agent_id.clone(),
            TargetType::Dir,
            root, // Use root path as is
            None,
          );
          ResourceItem {
            name,
            path: child_odfi.to_string(),
            r#type: ResourceType::Dir,
            size: None,
            modified: None,
            has_children: Some(true),
            child_count: None,
            hidden_child_count: None,
            mime_type: None,
          }
        })
        .collect();
      return Ok(items);
    }

    let url = format!("/api/v1/list_files?path={}", urlencoding::encode(&path_str));

    // Check if path points to an archive file (auto-detect)
    let lower_path = path_str.to_lowercase();
    let is_archive = odfi.target_type == TargetType::Archive
      || lower_path.ends_with(".tar")
      || lower_path.ends_with(".tar.gz")
      || lower_path.ends_with(".tgz")
      || lower_path.ends_with(".gz");

    if is_archive && !path_str.is_empty() && path_str != "/" {
      // Handle agent archive browsing - need to download the archive and list its contents
      return self
        .list_agent_archive(&client, agent_id, &path_str, odfi.entry_path.as_deref())
        .await;
    }

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
          // We keep path consistent with what agent returns
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
          mime_type: item.mime_type,
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
            mime_type: None,
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
      // Check if path points to an archive file (auto-detect)
      let lower_path = odfi.path.to_lowercase();
      let is_archive = odfi.target_type == TargetType::Archive
        || lower_path.ends_with(".tar")
        || lower_path.ends_with(".tar.gz")
        || lower_path.ends_with(".tgz")
        || lower_path.ends_with(".gz");

      if is_archive && !odfi.path.is_empty() {
        // Handle S3 archive browsing
        return self
          .list_s3_archive(&profile_row, bucket_name, &odfi.path, odfi.entry_path.as_deref())
          .await;
      }

      // Level 3: Bucket - List Objects (directory mode)

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
              mime_type: None,
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
              mime_type: None, // We don't sniff S3 contents here
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
            mime_type: None,
          }
        })
        .collect();
      Ok(items)
    }
  }

  /// List contents of an archive file stored in S3
  async fn list_s3_archive(
    &self,
    profile: &opsbox_core::repository::s3::S3Profile,
    bucket: &str,
    key: &str,
    entry_path: Option<&str>,
  ) -> Result<Vec<ResourceItem>, String> {
    use opsbox_core::storage::s3::get_or_create_s3_client;

    let client = get_or_create_s3_client(&profile.endpoint, &profile.access_key, &profile.secret_key)
      .map_err(|e| format!("Failed to create S3 client: {}", e))?;

    // Download the archive from S3
    let resp = client
      .get_object()
      .bucket(bucket)
      .key(key)
      .send()
      .await
      .map_err(|e| format!("Failed to get S3 object: {}", format_s3_error(&e)))?;

    // into_async_read() returns impl tokio::io::AsyncBufRead which implements AsyncRead
    let reader = resp.body.into_async_read();

    // Create archive stream
    let mut stream = opsbox_core::fs::create_archive_stream_from_reader(reader, Some(key))
      .await
      .map_err(|e| format!("Failed to open archive stream: {}", e))?;

    let mut items = Vec::new();

    let entry_prefix = entry_path.unwrap_or_default().to_string();
    let filter_prefix = if entry_prefix.is_empty() {
      "".to_string()
    } else if entry_prefix.ends_with('/') {
      entry_prefix.clone()
    } else {
      format!("{}/", entry_prefix)
    };

    let mut synthetic_dirs = std::collections::HashSet::new();

    // Iterate entries
    while let Ok(Some((meta, _reader))) = stream.next_entry().await {
      let raw_entry_path = meta.path.clone();
      let trimmed = raw_entry_path.trim_start_matches("./");
      let archive_entry_path = if trimmed.is_empty() {
        raw_entry_path.clone()
      } else {
        trimmed.to_string()
      };

      if !filter_prefix.is_empty() && !archive_entry_path.starts_with(&filter_prefix) {
        continue;
      }

      let rel_path = &archive_entry_path[filter_prefix.len()..];
      if rel_path.is_empty() {
        continue;
      }

      let parts: Vec<&str> = rel_path.splitn(2, '/').collect();
      let is_subdir = parts.len() > 1;

      if is_subdir {
        let dir_name = parts[0];
        if synthetic_dirs.contains(dir_name) {
          continue;
        }
        synthetic_dirs.insert(dir_name.to_string());

        let child_odfi = Odfi::new(
          EndpointType::S3,
          format!("{}:{}", profile.profile_name, bucket),
          TargetType::Archive,
          key.to_string(),
          Some(format!("{}{}/", filter_prefix, dir_name)),
        );

        items.push(ResourceItem {
          name: dir_name.to_string(),
          path: child_odfi.to_string(),
          r#type: ResourceType::Dir,
          size: None,
          modified: None,
          has_children: Some(true),
          child_count: None,
          hidden_child_count: None,
          mime_type: None,
        });
      } else {
        let child_odfi = Odfi::new(
          EndpointType::S3,
          format!("{}:{}", profile.profile_name, bucket),
          TargetType::Archive,
          key.to_string(),                  // Use S3 object key as archive path
          Some(archive_entry_path.clone()), // Use entry path inside the archive
        );

        items.push(ResourceItem {
          name: rel_path.to_string(),
          path: child_odfi.to_string(),
          r#type: ResourceType::File,
          size: meta.size,
          modified: None,
          has_children: None,
          child_count: None,
          hidden_child_count: None,
          mime_type: None,
        });
      }
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

    Ok(items)
  }

  /// List contents of an archive file from an Agent
  async fn list_agent_archive(
    &self,
    client: &opsbox_core::agent::AgentClient,
    agent_id: &str,
    archive_path: &str,
    filter_entry: Option<&str>,
  ) -> Result<Vec<ResourceItem>, String> {
    tracing::debug!(
      "list_agent_archive: agent_id={}, archive_path={}, filter_entry={:?}",
      agent_id,
      archive_path,
      filter_entry
    );

    // Download the archive from agent
    let url = format!("/api/v1/file_raw?path={}", urlencoding::encode(archive_path));
    let response = client
      .get_raw(&url)
      .await
      .map_err(|e| format!("Failed to download archive from agent: {}", e))?;

    tracing::debug!("list_agent_archive: downloaded archive, status={}", response.status());

    // Convert response body stream to AsyncRead using StreamReader
    let stream = response.bytes_stream().map_err(std::io::Error::other);
    let reader = StreamReader::new(stream);

    // Create archive stream
    let mut stream = opsbox_core::fs::create_archive_stream_from_reader(reader, Some(archive_path))
      .await
      .map_err(|e| format!("Failed to open archive stream: {}", e))?;

    tracing::debug!("list_agent_archive: archive stream created successfully");

    let mut items = Vec::new();

    let entry_prefix = filter_entry.unwrap_or_default().to_string();
    let filter_prefix = if entry_prefix.is_empty() {
      "".to_string()
    } else if entry_prefix.ends_with('/') {
      entry_prefix.clone()
    } else {
      format!("{}/", entry_prefix)
    };

    let mut synthetic_dirs = std::collections::HashSet::new();

    tracing::debug!(
      "list_agent_archive: starting to iterate entries, filter_prefix={}",
      filter_prefix
    );

    // Iterate entries
    while let Ok(Some((meta, _reader))) = stream.next_entry().await {
      tracing::debug!("list_agent_archive: found entry path={}", meta.path);
      // Remove leading "./" that tar sometimes includes
      let raw_entry_path = meta.path.clone();
      let cleaned_entry_path = raw_entry_path.trim_start_matches("./");
      let entry_path = if cleaned_entry_path.is_empty() {
        raw_entry_path.clone()
      } else {
        cleaned_entry_path.to_string()
      };

      if !filter_prefix.is_empty() && !entry_path.starts_with(&filter_prefix) {
        tracing::debug!("list_agent_archive: skipping entry, doesn't match filter_prefix");
        continue;
      }

      // Get relative path after filter_prefix
      let rel_path = if filter_prefix.is_empty() {
        entry_path.clone()
      } else {
        entry_path[filter_prefix.len()..].to_string()
      };

      tracing::debug!("list_agent_archive: rel_path={}", rel_path);

      if rel_path.is_empty() {
        continue;
      }

      let parts: Vec<&str> = rel_path.splitn(2, '/').collect();
      let is_subdir = parts.len() > 1;

      if is_subdir {
        let dir_name = parts[0];
        if synthetic_dirs.contains(dir_name) {
          continue;
        }
        synthetic_dirs.insert(dir_name.to_string());

        let child_odfi = Odfi::new(
          EndpointType::Agent,
          agent_id.to_string(),
          TargetType::Archive,
          archive_path.to_string(), // Original archive path
          Some(format!("{}{}/", filter_prefix, dir_name)),
        );

        items.push(ResourceItem {
          name: dir_name.to_string(),
          path: child_odfi.to_string(),
          r#type: ResourceType::Dir,
          size: None,
          modified: None,
          has_children: Some(true),
          child_count: None,
          hidden_child_count: None,
          mime_type: None,
        });
      } else {
        let child_odfi = Odfi::new(
          EndpointType::Agent,
          agent_id.to_string(),
          TargetType::Archive,
          archive_path.to_string(), // Original archive path - NOT the entry path!
          Some(entry_path.clone()), // Entry path inside the archive
        );

        items.push(ResourceItem {
          name: rel_path.to_string(),
          path: child_odfi.to_string(),
          r#type: ResourceType::File,
          size: meta.size,
          modified: None,
          has_children: None,
          child_count: None,
          hidden_child_count: None,
          mime_type: None,
        });
      }
    }

    tracing::debug!("list_agent_archive: found {} items", items.len());

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

    Ok(items)
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

    let bar = items.iter().find(|i| i.name == "bar").unwrap();
    assert_eq!(bar.r#type, ResourceType::Dir);
  }

  #[tokio::test]
  async fn test_list_local_archive_auto_detect() {
    // Create a temporary tar file
    let temp_dir = tempfile::tempdir().unwrap();
    let archive_path = temp_dir.path().join("autodetect.tar");
    let file = std::fs::File::create(&archive_path).unwrap();
    let mut builder = tar::Builder::new(file);

    // Add file
    let mut header = tar::Header::new_gnu();
    header.set_size(4);
    header.set_cksum();
    builder.append_data(&mut header, "auto.txt", "test".as_bytes()).unwrap();

    builder.finish().unwrap();

    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let service = ExplorerService::new(pool);

    // Create ODFI with TargetType::Dir (simulating user input or default traversal)
    let odfi = Odfi::new(
      EndpointType::Local,
      "localhost".to_string(),
      TargetType::Dir, // Intentionally not Archive
      archive_path.to_string_lossy().to_string(),
      None,
    );

    // List
    let items = service.list(&odfi).await.unwrap();

    // Verify it listed content inside tar
    assert_eq!(items.len(), 1);
    let auto = items.iter().find(|i| i.name == "auto.txt").unwrap();
    assert_eq!(auto.r#type, ResourceType::File);
  }

  #[tokio::test]
  async fn test_list_local_archive_targz_auto_detect() {
    use flate2::Compression;
    use flate2::write::GzEncoder;

    // Create a temporary tar.gz file
    let temp_dir = tempfile::tempdir().unwrap();
    let archive_path = temp_dir.path().join("autodetect.tar.gz");
    let file = std::fs::File::create(&archive_path).unwrap();
    let enc = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(enc);

    // Add file
    let mut header = tar::Header::new_gnu();
    header.set_size(4);
    header.set_cksum();
    builder
      .append_data(&mut header, "inner_gz.txt", "test".as_bytes())
      .unwrap();

    let enc = builder.into_inner().unwrap();
    enc.finish().unwrap();

    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let service = ExplorerService::new(pool);

    // Create ODFI with TargetType::Dir
    let odfi = Odfi::new(
      EndpointType::Local,
      "localhost".to_string(),
      TargetType::Dir,
      archive_path.to_string_lossy().to_string(),
      None,
    );

    // List
    let items = service.list(&odfi).await.unwrap();

    // Verify it listed content
    assert_eq!(items.len(), 1);
    let item = items.iter().find(|i| i.name == "inner_gz.txt").unwrap();
    assert_eq!(item.r#type, ResourceType::File);
  }
}
