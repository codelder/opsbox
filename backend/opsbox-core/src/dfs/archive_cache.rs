//! 归档缓存模块
//!
//! 为 S3/Agent 归档文件提供全局缓存，避免重复下载

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use tempfile::NamedTempFile;
use tokio::sync::RwLock;

use super::archive::ArchiveType;
use super::impls::ArchiveFileSystem;
use super::filesystem::FsError;

/// 缓存配置
const MAX_ENTRIES: usize = 20;
const MAX_TOTAL_SIZE: u64 = 1024 * 1024 * 1024; // 1GB
const ENTRY_TTL: Duration = Duration::from_secs(30 * 60); // 30 minutes

/// 全局归档缓存
static ARCHIVE_CACHE: LazyLock<RwLock<ArchiveCache>> =
    LazyLock::new(|| RwLock::new(ArchiveCache::new()));

/// 缓存 Key
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct ArchiveCacheKey {
    /// 端点标识（"local" / "s3.prod" / "agent.web-01"）
    pub endpoint_id: String,
    /// 归档文件路径
    pub path: String,
}

impl ArchiveCacheKey {
    pub fn new(endpoint_id: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            path: path.into(),
        }
    }
}

/// 缓存条目
struct CachedArchive {
    /// 归档文件系统（内部持有 Arc<NamedTempFile>，clone 时共享）
    fs: ArchiveFileSystem<crate::dfs::impls::LocalFileSystem>,
    /// 文件大小
    size: u64,
    /// 最后访问时间
    accessed_at: Instant,
}

/// 归档缓存
struct ArchiveCache {
    entries: HashMap<ArchiveCacheKey, CachedArchive>,
    access_order: Vec<ArchiveCacheKey>,
    total_size: u64,
}

impl ArchiveCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            access_order: Vec::new(),
            total_size: 0,
        }
    }

    fn get_cloned(&mut self, key: &ArchiveCacheKey) -> Option<ArchiveFileSystem<crate::dfs::impls::LocalFileSystem>> {
        if let Some(entry) = self.entries.get_mut(key) {
            // 检查过期
            if entry.accessed_at.elapsed() > ENTRY_TTL {
                return None;
            }
            entry.accessed_at = Instant::now();
            // 更新访问顺序
            if let Some(pos) = self.access_order.iter().position(|k| k == key) {
                self.access_order.remove(pos);
                self.access_order.push(key.clone());
            }
            return Some(entry.fs.clone());
        }
        None
    }

    fn put(
        &mut self,
        key: ArchiveCacheKey,
        fs: ArchiveFileSystem<crate::dfs::impls::LocalFileSystem>,
        size: u64,
    ) {
        // 空间不足时驱逐旧条目
        while self.total_size + size > MAX_TOTAL_SIZE && !self.entries.is_empty() {
            self.evict_lru();
        }

        // 条目数超限时驱逐
        while self.entries.len() >= MAX_ENTRIES {
            self.evict_lru();
        }

        self.total_size += size;
        self.access_order.push(key.clone());
        self.entries.insert(
            key,
            CachedArchive {
                fs,
                size,
                accessed_at: Instant::now(),
            },
        );
    }

    fn evict_lru(&mut self) {
        if let Some(old_key) = self.access_order.first().cloned() {
            if let Some(entry) = self.entries.remove(&old_key) {
                self.total_size -= entry.size;
                tracing::info!(
                    "Archive cache evicted: {} ({}) - freed {} bytes",
                    old_key.endpoint_id,
                    old_key.path,
                    entry.size
                );
            }
            self.access_order.remove(0);
        }
    }

    fn cleanup_expired(&mut self) {
        let now = Instant::now();
        let expired: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, e)| now.duration_since(e.accessed_at) > ENTRY_TTL)
            .map(|(k, _)| k.clone())
            .collect();

        for key in expired {
            if let Some(entry) = self.entries.remove(&key) {
                self.total_size -= entry.size;
                tracing::info!(
                    "Archive cache expired: {} ({}) - freed {} bytes",
                    key.endpoint_id,
                    key.path,
                    entry.size
                );
            }
            self.access_order.retain(|k| k != &key);
        }
    }

    #[allow(dead_code)]
    fn stats(&self) -> (usize, u64) {
        (self.entries.len(), self.total_size)
    }
}

/// 公开 API：获取缓存的归档
pub async fn get_cached_archive(key: &ArchiveCacheKey) -> Option<ArchiveFileSystem<crate::dfs::impls::LocalFileSystem>> {
    let mut cache = ARCHIVE_CACHE.write().await;
    cache.get_cloned(key)
}

/// 公开 API：缓存归档
pub async fn cache_archive(
    key: ArchiveCacheKey,
    fs: ArchiveFileSystem<crate::dfs::impls::LocalFileSystem>,
    size: u64,
) {
    let mut cache = ARCHIVE_CACHE.write().await;
    cache.put(key, fs, size);
}

/// 公开 API：清理过期缓存
pub async fn cleanup_expired_cache() {
    let mut cache = ARCHIVE_CACHE.write().await;
    cache.cleanup_expired();
}

/// 启动后台清理任务
pub fn start_cleanup_task() -> tokio::task::JoinHandle<()> {
    tokio::spawn(async {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            cleanup_expired_cache().await;
        }
    })
}

/// 从路径检测归档类型
fn detect_archive_type(path: &std::path::Path) -> ArchiveType {
    let path_str = path.to_string_lossy().to_lowercase();

    if path_str.ends_with(".tar.gz") || path_str.ends_with(".tgz") {
        ArchiveType::TarGz
    } else if path_str.ends_with(".tar") {
        ArchiveType::Tar
    } else if path_str.ends_with(".gz") {
        ArchiveType::Gz
    } else if path_str.ends_with(".zip") {
        ArchiveType::Zip
    } else {
        ArchiveType::Unknown
    }
}

/// 下载归档到临时文件并缓存
///
/// 用于 S3/Agent 归档文件的下载和缓存
pub async fn download_and_cache_archive<F>(
    key: ArchiveCacheKey,
    download_fn: F,
) -> Result<ArchiveFileSystem<crate::dfs::impls::LocalFileSystem>, FsError>
where
    F: futures::Future<Output = Result<(NamedTempFile, u64), FsError>>,
{
    // 1. 检查缓存
    if let Some(cached) = get_cached_archive(&key).await {
        tracing::debug!("Archive cache hit: {} ({})", key.endpoint_id, key.path);
        return Ok(cached);
    }

    // 2. 下载
    tracing::info!("Archive cache miss, downloading: {} ({})", key.endpoint_id, key.path);
    let (temp_file, size) = download_fn.await?;

    // 3. 创建 ArchiveFileSystem
    let temp_path = temp_file.path().to_path_buf();
    let archive_type = detect_archive_type(&temp_path);
    let local_fs = crate::dfs::impls::LocalFileSystem::new(
        temp_path.parent().unwrap_or(std::path::Path::new("/")).to_path_buf()
    ).map_err(|e| FsError::Io(std::io::Error::other(e.to_string())))?;

    // 创建文件系统（temp_file 被 move 进去，内部包装成 Arc）
    let fs = ArchiveFileSystem::with_temp_file(local_fs, archive_type, temp_path, temp_file);

    // 4. 缓存（fs.clone() 会共享内部的 Arc<NamedTempFile>）
    cache_archive(key, fs.clone(), size).await;

    Ok(fs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_equality() {
        let key1 = ArchiveCacheKey::new("s3.prod", "/logs/2024.tar.gz");
        let key2 = ArchiveCacheKey::new("s3.prod", "/logs/2024.tar.gz");
        let key3 = ArchiveCacheKey::new("s3.prod", "/logs/2023.tar.gz");

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[tokio::test]
    async fn test_cache_get_miss() {
        let key = ArchiveCacheKey::new("local", "/nonexistent.tar");
        let result = get_cached_archive(&key).await;
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_archive_type() {
        assert_eq!(detect_archive_type(std::path::Path::new("test.tar.gz")), ArchiveType::TarGz);
        assert_eq!(detect_archive_type(std::path::Path::new("test.tgz")), ArchiveType::TarGz);
        assert_eq!(detect_archive_type(std::path::Path::new("test.tar")), ArchiveType::Tar);
        assert_eq!(detect_archive_type(std::path::Path::new("test.gz")), ArchiveType::Gz);
        assert_eq!(detect_archive_type(std::path::Path::new("test.zip")), ArchiveType::Zip);
        assert_eq!(detect_archive_type(std::path::Path::new("test.txt")), ArchiveType::Unknown);
    }
}
