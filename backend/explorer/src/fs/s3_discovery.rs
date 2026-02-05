//! S3 发现文件系统
//!
//! 提供 S3 Profile 和 Bucket 的虚拟目录视图，使用 DFS 抽象
//!
//! levels:
//! / -> List Profiles
//! /{profile} -> List Buckets

use async_trait::async_trait;
use opsbox_core::dfs::{
    filesystem::{DirEntry, FileMetadata, OpbxFileSystem},
    path::ResourcePath,
};
use opsbox_core::repository::s3::{list_s3_profiles, load_s3_profile};
use opsbox_core::SqlitePool;

/// S3 发现文件系统
/// 提供 S3 Profile 和 Bucket 的虚拟目录视图
pub struct S3DiscoveryFileSystem {
  db_pool: SqlitePool,
}

impl S3DiscoveryFileSystem {
  pub fn new(db_pool: SqlitePool) -> Self {
    Self { db_pool }
  }

  /// 获取 S3 profile 列表
  async fn list_profiles(&self) -> Result<Vec<DirEntry>, opsbox_core::dfs::FsError> {
    let profiles = list_s3_profiles(&self.db_pool)
      .await
      .map_err(|e| opsbox_core::dfs::FsError::InvalidConfig(e.to_string()))?;

    let entries = profiles
      .into_iter()
      .map(|p| {
        let name = p.profile_name.clone();
        let path = ResourcePath::from_str(&format!("/{}", p.profile_name));

        DirEntry {
          name,
          path,
          metadata: FileMetadata::dir(0),
        }
      })
      .collect();

    Ok(entries)
  }

  /// 列出指定 profile 的 buckets
  async fn list_buckets(&self, profile_name: &str) -> Result<Vec<DirEntry>, opsbox_core::dfs::FsError> {
    use opsbox_core::storage::s3::get_or_create_s3_client;

    let profile = load_s3_profile(&self.db_pool, profile_name)
      .await
      .map_err(|e| opsbox_core::dfs::FsError::InvalidConfig(e.to_string()))?
      .ok_or_else(|| opsbox_core::dfs::FsError::NotFound(format!("Profile '{}' not found", profile_name)))?;

    let client = get_or_create_s3_client(&profile.endpoint, &profile.access_key, &profile.secret_key)
      .map_err(|e| opsbox_core::dfs::FsError::InvalidConfig(format!("Failed to create S3 client: {}", e)))?;

    let resp = client
      .list_buckets()
      .send()
      .await
      .map_err(|e| opsbox_core::dfs::FsError::InvalidConfig(format!("Failed to list buckets: {}", e)))?;

    let entries = resp
      .buckets
      .unwrap_or_default()
      .into_iter()
      .map(|b| {
        let name = b.name.unwrap_or_default();
        // NOTE: Once we select a bucket, ORL structure changes to orl://profile:bucket@s3/
        let path = ResourcePath::from_str(&format!("/{}:{}", profile_name, name));

        let modified = b.creation_date
          .map(|d| std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(d.secs() as u64));

        DirEntry {
          name,
          path,
          metadata: FileMetadata {
            is_dir: true,
            is_file: false,
            size: 0,
            modified,
            created: None,
          },
        }
      })
      .collect();

    Ok(entries)
  }
}

#[async_trait]
impl OpbxFileSystem for S3DiscoveryFileSystem {
  /// 获取元数据（根目录或 profile 目录）
  async fn metadata(&self, _path: &ResourcePath) -> Result<FileMetadata, opsbox_core::dfs::FsError> {
    // 根目录和 profile 目录都视为目录
    Ok(FileMetadata::dir(0))
  }

  /// 读取目录内容
  async fn read_dir(&self, path: &ResourcePath) -> Result<Vec<DirEntry>, opsbox_core::dfs::FsError> {
    let segments = path.segments();

    // 1. 根目录：列出 Profiles
    if segments.is_empty() || (segments.len() == 1 && segments[0].is_empty()) {
      return self.list_profiles().await;
    }

    // 2. Profile 级别：列出 Buckets
    // path 是 profile 名称或 /profile_name
    let profile_name = &segments[0];
    if segments.len() == 1 {
      return self.list_buckets(profile_name).await;
    }

    // 3. 更深层次不支持
    Err(opsbox_core::dfs::FsError::InvalidConfig(
      "S3 Discovery only supports 2 levels (root and profile)".to_string(),
    ))
  }

  /// 不支持读取 S3 根目录作为文件
  async fn open_read(
    &self,
    _path: &ResourcePath,
  ) -> Result<Box<dyn opsbox_core::dfs::filesystem::AsyncRead + Send + Unpin>, opsbox_core::dfs::FsError> {
    Err(opsbox_core::dfs::FsError::InvalidConfig(
      "Cannot read S3 root as file".to_string(),
    ))
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn test_s3_discovery_new() {
    // 基础的类型测试
  }
}
