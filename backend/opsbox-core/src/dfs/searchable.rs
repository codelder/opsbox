//! Streamable trait - 可流式化的文件系统接口
//!
//! 为文件系统提供者提供条目流（EntryStream）的能力。
//! 扩展 OpbxFileSystem trait，添加 as_entry_stream 方法用于获取文件条目流。

use std::time::Duration;

use async_trait::async_trait;

use super::filesystem::{FsError, OpbxFileSystem};
use super::path::ResourcePath;
use crate::fs::EntryStream;

/// 搜索配置
#[derive(Debug, Clone)]
pub struct SearchConfig {
  /// 最大并发数（默认使用 CPU 核心数的 2 倍）
  pub max_concurrency: usize,
  /// 内容处理超时时间
  pub content_timeout: Duration,
  /// 预读阈值（小于此值的文件预读到内存）
  pub preload_threshold: usize,
}

impl Default for SearchConfig {
  fn default() -> Self {
    // 根据 CPU 核心数动态计算并发度
    let cpu_count = num_cpus::get();
    let default_concurrency = (cpu_count * 2).clamp(8, 32);

    Self {
      max_concurrency: default_concurrency,
      content_timeout: Duration::from_secs(60),
      preload_threshold: 120 * 1024 * 1024, // 120MB
    }
  }
}

impl SearchConfig {
  /// 创建新的搜索配置
  pub fn new() -> Self {
    Self::default()
  }

  /// 设置最大并发数
  pub fn with_max_concurrency(mut self, concurrency: usize) -> Self {
    self.max_concurrency = concurrency.clamp(1, 128);
    self
  }

  /// 设置内容处理超时时间
  pub fn with_content_timeout(mut self, timeout: Duration) -> Self {
    self.content_timeout = timeout;
    self
  }

  /// 设置预读阈值
  pub fn with_preload_threshold(mut self, threshold: usize) -> Self {
    self.preload_threshold = threshold;
    self
  }
}

/// 可流式化文件系统 trait
///
/// 扩展 OpbxFileSystem，提供获取 EntryStream 的能力。
/// 所有支持条目流访问的文件系统提供者都应实现此 trait。
#[async_trait]
pub trait Streamable: OpbxFileSystem {
  /// 获取条目流用于搜索
  ///
  /// # Arguments
  /// * `path` - 资源路径
  /// * `recursive` - 是否递归遍历
  /// * `config` - 搜索配置
  ///
  /// # Returns
  /// 返回一个 EntryStream，可以迭代获取文件条目
  async fn as_entry_stream(
    &self,
    path: &ResourcePath,
    recursive: bool,
    config: &SearchConfig,
  ) -> Result<Box<dyn EntryStream>, FsError>;

  /// 检查是否支持流式搜索
  ///
  /// 某些文件系统（如 S3）可能需要先下载再搜索
  fn supports_streaming_search(&self) -> bool {
    true
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_search_config_default() {
    let config = SearchConfig::default();
    assert!(config.max_concurrency >= 8);
    assert!(config.max_concurrency <= 32);
    assert_eq!(config.content_timeout, Duration::from_secs(60));
    assert_eq!(config.preload_threshold, 120 * 1024 * 1024);
  }

  #[test]
  fn test_search_config_with_max_concurrency() {
    let config = SearchConfig::new().with_max_concurrency(64);
    assert_eq!(config.max_concurrency, 64);
  }

  #[test]
  fn test_search_config_with_max_concurrency_clamped() {
    let config = SearchConfig::new().with_max_concurrency(200);
    assert_eq!(config.max_concurrency, 128); // clamped to max

    let config = SearchConfig::new().with_max_concurrency(0);
    assert_eq!(config.max_concurrency, 1); // clamped to min
  }

  #[test]
  fn test_search_config_with_content_timeout() {
    let config = SearchConfig::new().with_content_timeout(Duration::from_secs(120));
    assert_eq!(config.content_timeout, Duration::from_secs(120));
  }

  #[test]
  fn test_search_config_with_preload_threshold() {
    let config = SearchConfig::new().with_preload_threshold(64 * 1024 * 1024);
    assert_eq!(config.preload_threshold, 64 * 1024 * 1024);
  }
}
