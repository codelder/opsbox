use super::fs::{OpsFileSystem, OpsRead};
use super::orl::{EndpointType, ORL, OpsPath, TargetType};
use super::providers::archive::cache::ArchiveCache;
use super::types::{OpsEntry, OpsMetadata};
use std::io;
use std::sync::Arc;
use std::pin::Pin;
use std::future::Future;

pub type OpsFileSystemResolver = Box<dyn Fn(String) -> Pin<Box<dyn Future<Output = Option<Arc<dyn OpsFileSystem>>> + Send>> + Send + Sync>;

/// ORL 管理器 (The Router)
pub struct OrlManager {
  providers: std::collections::HashMap<String, Arc<dyn OpsFileSystem>>,
  archive_cache: ArchiveCache,
  resolver: Option<OpsFileSystemResolver>,
}

impl Default for OrlManager {
  fn default() -> Self {
    Self::new()
  }
}

impl OrlManager {
  pub fn new() -> Self {
    Self {
      providers: std::collections::HashMap::new(),
      archive_cache: ArchiveCache::new(5), // Default capacity 5
      resolver: None,
    }
  }

  pub fn with_resolver(mut self, resolver: OpsFileSystemResolver) -> Self {
    self.resolver = Some(resolver);
    self
  }

  pub fn set_resolver(&mut self, resolver: OpsFileSystemResolver) {
    self.resolver = Some(resolver);
  }

  pub fn register(&mut self, key: String, fs: Arc<dyn OpsFileSystem>) {
    self.providers.insert(key, fs);
  }

  async fn get_provider(&self, key: &str) -> io::Result<Arc<dyn OpsFileSystem>> {
    if let Some(fs) = self.providers.get(key) {
        Ok(fs.clone())
    } else if let Some(resolver) = &self.resolver {
        resolver(key.to_string()).await
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("Provider not found: {} (resolved)", key)))
    } else {
        Err(io::Error::new(io::ErrorKind::NotFound, format!("Provider not found: {}", key)))
    }
  }

  /// Resolve ORL to (Provider, InternalPath)
  /// Now Async to support download
  async fn resolve(&self, url: &ORL) -> io::Result<(Arc<dyn OpsFileSystem>, OpsPath)> {
    // 1. Resolve Base Provider
    // ORL methods now return values or Results
    let endpoint_type = url.endpoint_type().map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let endpoint_id = url.endpoint_id(); // Use endpoint_id directly, not effective_id

    let key = match endpoint_type {
      EndpointType::Local => "local".to_string(),
      EndpointType::S3 => {
          match endpoint_id {
              Some(id) if !id.is_empty() => format!("s3.{}", id),
              _ => "s3.root".to_string(),
          }
      },
      EndpointType::Agent => {
          match endpoint_id {
              Some(id) if !id.is_empty() => format!("agent.{}", id),
              _ => "agent.root".to_string(),
          }
      },
    };

    let base_fs = self.get_provider(&key).await?;

    // Decode path
    let decoded_path = percent_encoding::percent_decode_str(url.path())
        .decode_utf8_lossy()
        .into_owned();

    // 2. Handle Directory (Standard)
    if url.target_type() == TargetType::Dir {
      return Ok((base_fs, OpsPath::new(decoded_path)));
    }

    // 3. Handle Archive
    if url.target_type() == TargetType::Archive {
      let cache_key = format!("{}::{}", key, decoded_path);
      let dl_fs = base_fs.clone();
      let dl_path = OpsPath::new(&decoded_path);

      // Download Closure
      // Need to clone url because it's used in async move block but passed by reference
      // Actually cache needs owned ORL or reference?
      // ArchiveCache::get_or_download takes &ORL.
      // So we can pass url reference.
      let fs = self
        .archive_cache
        .get_or_download(cache_key, url, || async move { dl_fs.open_read(&dl_path).await })
        .await?;

      // Return Overlay FS and the ENTRY path
      let entry_sub_path = url.entry_path().map(|c| c.into_owned()).unwrap_or_default();
      // Entry path implies inner path, usually encoded? Need check.
      // Usually URL query params are encoded. entry=foo%2Fbar.
      // ORL.entry_path() likely decodes?
      // "entry_path" returns Cow<str>. If it's from query pair decoding, it's decoded.

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

  /// 获取统一的条目流（核心方法）
  /// 根据 ORL 的 target_type 智能决定是递归遍历目录、还是展开归档、还是读取单文件
  pub async fn get_entry_stream(&self, orl: &ORL, recursive: bool) -> io::Result<Box<dyn crate::fs::EntryStream>> {
    // 1. 手动解析 Provider，绕过 resolve 方法对 Archive 的自动挂载逻辑
    // 因为我们需要获取归档文件本身的流，而不是挂载后的视图
    let endpoint_type = orl.endpoint_type().map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let endpoint_id = orl.effective_id();

    let key = match endpoint_type {
      EndpointType::Local => "local".to_string(),
      EndpointType::S3 => {
          if endpoint_id.is_empty() {
              "s3.root".to_string()
          } else {
              format!("s3.{}", endpoint_id)
          }
      },
      EndpointType::Agent => {
          if endpoint_id.is_empty() {
              "agent.root".to_string()
          } else {
              format!("agent.{}", endpoint_id)
          }
      },
    };

    let base_fs = self.get_provider(&key).await?;

    let decoded_path = percent_encoding::percent_decode_str(orl.path())
        .decode_utf8_lossy()
        .into_owned();
    let path = OpsPath::new(decoded_path);

    // S3 特殊处理：ORL 路径包含 bucket，但 S3OpsFS 已绑定 bucket，需剥离
    let adjusted_path = if matches!(endpoint_type, EndpointType::S3) {
        let p = path.as_str().trim_start_matches('/');
        let key = p.split_once('/').map(|(_, k)| k).unwrap_or("");
        OpsPath::new(key)
    } else {
        path.clone()
    };

    let path_str = adjusted_path.as_str().to_lowercase();

    // 2. 探测是否为归档
    let is_archive_ext = path_str.ends_with(".tar")
        || path_str.ends_with(".tar.gz")
        || path_str.ends_with(".tgz")
        || path_str.ends_with(".gz")
        || path_str.ends_with(".zip");

    tracing::info!(
        "OrlManager::get_entry_stream orl={}, raw_path={}, adjusted_path={}, is_archive_ext={}, target_type={:?}",
        orl, path.as_str(), adjusted_path.as_str(), is_archive_ext, orl.target_type()
    );

    if orl.target_type() == TargetType::Archive || is_archive_ext {
        // 获取归档文件原始流
        let reader = base_fs.open_read(&adjusted_path).await?;

        // 包装为流式解压流
        crate::fs::create_archive_stream_from_reader(reader, Some(adjusted_path.as_str())).await
            .map_err(|e| io::Error::other(e))
    } else {
        // 普通文件或目录，交给 Provider 处理
        base_fs.as_entry_stream(&adjusted_path, recursive).await
    }
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
