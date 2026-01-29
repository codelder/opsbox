use async_trait::async_trait;
use opsbox_core::SqlitePool;
use opsbox_core::odfs::fs::{OpsFileSystem, OpsRead};
use opsbox_core::odfs::orl::OpsPath;
use opsbox_core::odfs::types::{OpsEntry, OpsFileType, OpsMetadata};
use opsbox_core::repository::s3::{list_s3_profiles, load_s3_profile};
use opsbox_core::storage::s3::get_or_create_s3_client;
use std::io;

/// S3 发现文件系统
/// 提供 S3 Profile 和 Bucket 的虚拟目录视图
///
/// levels:
/// / -> List Profiles
/// /{profile} -> List Buckets
pub struct S3DiscoveryFileSystem {
  db_pool: SqlitePool,
}

impl S3DiscoveryFileSystem {
  pub fn new(db_pool: SqlitePool) -> Self {
    Self { db_pool }
  }
}

#[async_trait]
impl OpsFileSystem for S3DiscoveryFileSystem {
  async fn metadata(&self, path: &OpsPath) -> io::Result<OpsMetadata> {
    let path_str = path.as_str();

    // Root or Profile level acts as Directory
    // But we should probably check if profile exists if path is /{profile}
    // For simplicity, we assume directory for navigation

    Ok(OpsMetadata {
      name: if path_str == "/" {
        "s3_root".to_string()
      } else {
        path_str.to_string()
      },
      file_type: OpsFileType::Directory,
      size: 0,
      modified: None,
      mode: 0755,
      mime_type: None,
      compression: None,
      is_archive: false,
    })
  }

  async fn read_dir(&self, path: &OpsPath) -> io::Result<Vec<OpsEntry>> {
    let path_str = path.as_str();

    // 1. Root: List Profiles
    if path_str == "/" {
      let profiles = list_s3_profiles(&self.db_pool)
        .await
        .map_err(|e| io::Error::other(e.to_string()))?;

      let entries = profiles
        .into_iter()
        .map(|p| {
          let name = p.profile_name.clone();
          let path = format!("orl://s3/{}", p.profile_name);

          let metadata = OpsMetadata {
            name: name.clone(),
            file_type: OpsFileType::Directory,
            size: 0,
            modified: None,
            mode: 0755,
            mime_type: None,
            compression: None,
            is_archive: false,
          };

          OpsEntry { name, path, metadata }
        })
        .collect();

      return Ok(entries);
    }

    // 2. Profile Level: List Buckets
    // path is key (profile name), or /key
    let profile_name = path_str.trim_matches('/');
    if profile_name.contains('/') {
      return Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "S3 Discovery only supports 2 levels (root and profile)",
      ));
    }

    let profile = load_s3_profile(&self.db_pool, profile_name)
      .await
      .map_err(|e| io::Error::other(e.to_string()))?
      .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Profile not found"))?;

    let client = get_or_create_s3_client(&profile.endpoint, &profile.access_key, &profile.secret_key)
      .map_err(|e| io::Error::other(e.to_string()))?;

    let resp = client
      .list_buckets()
      .send()
      .await
      .map_err(|e| io::Error::other(e.to_string()))?;

    let entries = resp
      .buckets
      .unwrap_or_default()
      .into_iter()
      .map(|b| {
        let name = b.name.unwrap_or_default();
        // NOTE: Once we select a bucket, ORL structure changes to orl://profile:bucket@s3/
        // The discovery FS leads us to this point.
        let path = format!("orl://{}:{}@s3/", profile_name, name);
        let created = b
          .creation_date
          .map(|d| std::time::UNIX_EPOCH + std::time::Duration::from_secs(d.secs() as u64));

        let metadata = OpsMetadata {
          name: name.clone(),
          file_type: OpsFileType::Directory,
          size: 0,
          modified: created,
          mode: 0755,
          mime_type: None,
          compression: None,
          is_archive: false,
        };

        OpsEntry { name, path, metadata }
      })
      .collect();

    Ok(entries)
  }

  async fn open_read(&self, _path: &OpsPath) -> io::Result<OpsRead> {
    Err(io::Error::new(
      io::ErrorKind::PermissionDenied,
      "Cannot read S3 root as file",
    ))
  }

  fn name(&self) -> &str {
    "s3_discovery"
  }
}
