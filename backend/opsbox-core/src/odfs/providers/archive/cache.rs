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
    let fs: Arc<dyn OpsFileSystem> = if orl.path().ends_with(".zip") {
      Arc::new(ZipOpsFS::new(temp_path, Some(temp_file)).await?)
    } else if orl.path().ends_with(".tar") {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_archive_cache_hit_and_miss() {
        let cache = ArchiveCache::new(2);
        let key = "test-key".to_string();
        let orl = ORL::parse("orl://local/tmp/test.tar").unwrap();

        // 模拟下载函数：创建一个简单的空 tar
        let download_fn = || async {
            let tar_data = vec![0u8; 1024]; // 模拟数据
            Ok(Box::pin(io::Cursor::new(tar_data)) as OpsRead)
        };

        // 第一次：Miss
        let fs1 = cache.get_or_download(key.clone(), &orl, download_fn).await.unwrap();
        assert_eq!(fs1.name(), "TarOpsFS");

        // 第二次：Hit
        let fs2 = cache.get_or_download(key, &orl, || async { unreachable!() }).await.unwrap();
        assert!(Arc::ptr_eq(&fs1, &fs2));
    }

    #[tokio::test]
    async fn test_archive_cache_zip() {
        let cache = ArchiveCache::new(2);
        let orl = ORL::parse("orl://local/tmp/test.zip").unwrap();
        let key = "zip-key".to_string();

        let fs = cache.get_or_download(key, &orl, || async {
            let zip_data = vec![0u8; 1024]; // Basic buffer
            Ok(Box::pin(io::Cursor::new(zip_data)) as OpsRead)
        }).await.unwrap();

        assert_eq!(fs.name(), "ZipOpsFS");
    }

    #[tokio::test]
    async fn test_archive_cache_unsupported_type() {
        let cache = ArchiveCache::new(2);
        let orl = ORL::parse("orl://local/tmp/test.txt").unwrap();

        let res = cache.get_or_download("key".into(), &orl, || async {
            Ok(Box::pin(io::Cursor::new(vec![])) as OpsRead)
        }).await;

        match res {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::Unsupported),
            Ok(_) => panic!("Should have failed"),
        }
    }
}
