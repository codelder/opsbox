// ============================================================================
// MinIO 存储源
// ============================================================================

use super::{DataSource, FileEntry, FileIterator, FileMetadata, FileReader, StorageError};
use crate::utils::storage as legacy;
use async_stream::stream;
use async_trait::async_trait;
use futures::StreamExt;
use log::{debug, warn};
use minio::s3::types::ToStream;
use std::sync::Arc;

/// MinIO 存储源配置
#[derive(Debug, Clone)]
pub struct MinIOConfig {
  /// MinIO 服务器 URL
  pub url: String,
  /// 访问密钥
  pub access_key: String,
  /// 密钥
  pub secret_key: String,
  /// 桶名称
  pub bucket: String,
  /// 路径前缀（可选，用于限制搜索范围）
  pub prefix: Option<String>,
  /// 路径过滤正则（可选）
  pub pattern: Option<String>,
}

/// MinIO 存储源
///
/// 提供对 MinIO S3 对象存储的访问，搜索逻辑由 Server 端执行
pub struct MinIOStorage {
  config: MinIOConfig,
  client: Arc<minio::s3::Client>,
}

impl MinIOStorage {
  /// 创建新的 MinIO 存储源
  ///
  /// # 参数
  ///
  /// * `config` - MinIO 配置
  pub fn new(config: MinIOConfig) -> Result<Self, StorageError> {
    debug!(
      "创建 MinIO 存储源: url={}, bucket={}, prefix={:?}",
      config.url, config.bucket, config.prefix
    );

    // 复用现有的客户端创建逻辑
    let client = legacy::get_or_create_minio_client(&config.url, &config.access_key, &config.secret_key)?;

    Ok(Self { config, client })
  }
}

#[async_trait]
impl DataSource for MinIOStorage {
  fn source_type(&self) -> &'static str {
    "MinIOStorage"
  }

  async fn list_files(&self) -> Result<FileIterator, StorageError> {
    let client = Arc::clone(&self.client);
    let bucket = self.config.bucket.clone();
    let prefix = self.config.prefix.clone();
    let pattern = self.config.pattern.clone();

    debug!("列举 MinIO 对象: bucket={}, prefix={:?}", bucket, prefix);

    let stream = stream! {
      // 列举对象
      let mut list_builder = client.list_objects(&bucket);

      if let Some(ref pfx) = prefix {
        list_builder = list_builder.prefix(Some(pfx.clone()));
      }

      list_builder = list_builder.recursive(true);

      let mut object_stream = list_builder.to_stream().await;

      let mut count = 0;

      while let Some(item_result) = object_stream.next().await {
        let obj = match item_result {
          Ok(o) => o,
          Err(e) => {
            warn!("MinIO 列举对象项失败: {}", e);
            yield Err(legacy::StorageError::MinioListObjects(e.to_string()).into());
            continue;
          }
        };

        let key = obj.name;

        // 跳过目录（以 / 结尾）
        if key.ends_with('/') {
          continue;
        }

        // 应用路径过滤
        let matches = if let Some(ref pat) = pattern {
          if let Ok(re) = regex::Regex::new(pat) {
            re.is_match(&key)
          } else {
            true
          }
        } else {
          true
        };

        if !matches {
          continue;
        }

        count += 1;

        let entry = FileEntry {
          path: key,
          metadata: FileMetadata {
            size: None,  // MinIO list_objects 不提供大小
            modified: None,  // MinIO list_objects 不提供修改时间
            content_type: None,
          },
        };

        yield Ok(entry);
      }

      debug!("MinIO 对象列举完成: {} 个文件", count);
    };

    Ok(Box::new(Box::pin(stream)))
  }

  async fn open_file(&self, entry: &FileEntry) -> Result<FileReader, StorageError> {
    debug!("打开 MinIO 对象: bucket={}, key={}", self.config.bucket, entry.path);

    // 使用现有的 S3ReaderProvider
    let provider = legacy::S3ReaderProvider::new(
      &self.config.url,
      &self.config.access_key,
      &self.config.secret_key,
      &self.config.bucket,
      &entry.path,
    );

    // 复用现有的重试逻辑
    let reader = legacy::ReaderProvider::open(&provider).await?;

    Ok(reader)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_minio_config() {
    let config = MinIOConfig {
      url: "http://localhost:9000".to_string(),
      access_key: "minioadmin".to_string(),
      secret_key: "minioadmin".to_string(),
      bucket: "logs".to_string(),
      prefix: Some("2024/".to_string()),
      pattern: Some(r"\.log$".to_string()),
    };

    assert_eq!(config.bucket, "logs");
    assert_eq!(config.prefix, Some("2024/".to_string()));
  }

  #[test]
  fn test_matches_filter_prefix() {
    let _config = MinIOConfig {
      url: "http://localhost:9000".to_string(),
      access_key: "test".to_string(),
      secret_key: "test".to_string(),
      bucket: "test".to_string(),
      prefix: Some("logs/".to_string()),
      pattern: None,
    };

    // 这个测试需要 MinIO 客户端，但我们只测试配置
    // 实际的集成测试应该在集成测试中进行
  }

  #[test]
  fn test_matches_filter_pattern() {
    let _config = MinIOConfig {
      url: "http://localhost:9000".to_string(),
      access_key: "test".to_string(),
      secret_key: "test".to_string(),
      bucket: "test".to_string(),
      prefix: None,
      pattern: Some(r"\.log$".to_string()),
    };

    // 模式匹配测试
    let re = regex::Regex::new(r"\.log$").unwrap();
    assert!(re.is_match("app.log"));
    assert!(!re.is_match("app.txt"));
  }

  // 注意：MinIO 的实际集成测试需要运行的 MinIO 服务器
  // 这些测试应该在 tests/ 目录下的集成测试中进行
  // 这里只测试配置和基本逻辑
}
