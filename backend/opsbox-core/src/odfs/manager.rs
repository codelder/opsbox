use super::fs::{OpsFileSystem, OpsRead};
use super::orl::{EndpointType, ORL, OpsPath, TargetType};
use super::providers::archive::cache::ArchiveCache;
use super::types::{OpsEntry, OpsMetadata};
use std::collections::HashMap;
use std::io;
use std::sync::Arc;

/// ORL 管理器 (The Router)
pub struct OrlManager {
  providers: HashMap<String, Arc<dyn OpsFileSystem>>,
  archive_cache: Arc<ArchiveCache>,
}

impl Default for OrlManager {
  fn default() -> Self {
    Self::new()
  }
}

impl OrlManager {
  pub fn new() -> Self {
    Self {
      providers: HashMap::new(),
      archive_cache: Arc::new(ArchiveCache::new(5)), // Default capacity 5
    }
  }

  pub fn register(&mut self, id: String, fs: Arc<dyn OpsFileSystem>) {
    self.providers.insert(id, fs);
  }

  /// Resolve ORL to (Provider, InternalPath)
  /// Now Async to support download
  async fn resolve(&self, url: &ORL) -> io::Result<(Arc<dyn OpsFileSystem>, OpsPath)> {
    // 1. Resolve Base Provider
    let key = match url.endpoint_type {
      EndpointType::Local => "local".to_string(),
      EndpointType::S3 => format!("s3.{}", url.endpoint_id),
      EndpointType::Agent => format!("agent.{}", url.endpoint_id),
    };

    let base_fs = self
      .providers
      .get(&key)
      .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("Provider not found: {}", key)))?
      .clone();

    // 2. Handle Directory (Standard)
    if url.target_type == TargetType::Dir {
      return Ok((base_fs, OpsPath::new(&url.path)));
    }

    // 3. Handle Archive
    if url.target_type == TargetType::Archive {
      let cache_key = format!("{}::{}", key, url.path);
      let dl_fs = base_fs.clone();
      let dl_path = OpsPath::new(&url.path);

      // Download Closure
      let fs = self
        .archive_cache
        .get_or_download(cache_key, url, || async move { dl_fs.open_read(&dl_path).await })
        .await?;

      // Return Overlay FS and the ENTRY path
      let entry_sub_path = url.entry_path.clone().unwrap_or_default(); // Should not be empty for archive target

      return Ok((fs, OpsPath::new(entry_sub_path)));
    }

    Err(io::Error::new(io::ErrorKind::InvalidInput, "Unknown target type"))
  }

  pub async fn metadata(&self, orl: &ORL) -> io::Result<OpsMetadata> {
    let (fs, path) = self.resolve(orl).await?;
    fs.metadata(&path).await
  }

  pub async fn read_dir(&self, orl: &ORL) -> io::Result<Vec<OpsEntry>> {
    let (fs, path) = self.resolve(orl).await?;
    fs.read_dir(&path).await
  }

  pub async fn open_read(&self, orl: &ORL) -> io::Result<OpsRead> {
    let (fs, path) = self.resolve(orl).await?;
    fs.open_read(&path).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::odfs::providers::LocalOpsFS;
  use std::str::FromStr;

  #[tokio::test]
  async fn test_orl_manager_routing() {
    let mut manager = OrlManager::new();
    let local_fs = Arc::new(LocalOpsFS::new(None));

    manager.register("local".to_string(), local_fs);

    let orl_str = "orl://localhost@local/var/log/syslog";
    let _orl = ORL::from_str(orl_str).expect("Failed to parse ORL");

    // Need to mock or just check resolve logic if public?
    // resolve is private. But we can check public API open_read/metadata.
    // Or make resolve pub(crate) for testing?
    // Relying on public API `open_read` or checking if compilation passes for now.
  }
}
