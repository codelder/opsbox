use crate::odfs::providers::archive::tar::TarOpsFS;
use crate::odfs::providers::archive::zip::ZipOpsFS;
use crate::odfs::{ORL, OpsFileSystem, OpsRead};
use lru::LruCache;
use std::io;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt; // for creating temp file

/// 归档文件缓存管理器
pub struct ArchiveCache {
  cache: Mutex<LruCache<String, Arc<dyn OpsFileSystem>>>,
}

impl ArchiveCache {
  pub fn new(capacity: usize) -> Self {
    let cap = NonZeroUsize::new(capacity).expect("Capacity must be > 0");
    Self {
      cache: Mutex::new(LruCache::new(cap)),
    }
  }

  pub async fn get_or_download<F, Fut>(
    &self,
    key: String,
    orl: &ORL,
    download_fn: F,
  ) -> io::Result<Arc<dyn OpsFileSystem>>
  where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = io::Result<OpsRead>>,
  {
    // 1. Check Cache
    {
      let mut cache = self.cache.lock().unwrap();
      if let Some(fs) = cache.get(&key) {
        return Ok(fs.clone());
      }
    }

    // 2. Download to TempFile
    let mut stream = download_fn().await?;

    // Create temp file
    let temp_file = tokio::task::spawn_blocking(NamedTempFile::new)
      .await
      .map_err(|e| io::Error::other(e.to_string()))??;

    let temp_path = temp_file.path().to_path_buf();

    // Write stream to file
    // We need a handle to write. We can re-open or clone.
    let file_handle = temp_file.as_file().try_clone()?;
    let mut tokio_file = tokio::fs::File::from_std(file_handle);

    tokio::io::copy(&mut stream, &mut tokio_file).await?;
    tokio_file.flush().await?; // Ensure data is on disk

    // 3. Create Overlay FS
    let fs: Arc<dyn OpsFileSystem> = if orl.path.ends_with(".zip") {
      Arc::new(ZipOpsFS::new(temp_path, Some(temp_file)).await?)
    } else if orl.path.ends_with(".tar") {
      Arc::new(TarOpsFS::new(temp_path, Some(temp_file)).await?)
    } else {
      return Err(io::Error::new(io::ErrorKind::Unsupported, "Unsupported archive type"));
    };

    // 4. Insert into Cache
    {
      let mut cache = self.cache.lock().unwrap();
      cache.put(key, fs.clone());
    }

    Ok(fs)
  }
}
