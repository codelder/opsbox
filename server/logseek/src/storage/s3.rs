// ============================================================================
// S3 兼容对象存储源（MinIO 实现）
// ============================================================================

use super::{DataSource, FileEntry, FileIterator, FileMetadata, FileReader, StorageError};
use crate::utils::storage as legacy;
use async_stream::stream;
use async_trait::async_trait;
use aws_sdk_s3::Client as S3Client;
use log::{debug, warn};
use std::sync::Arc;

/// S3 兼容对象存储配置（支持 MinIO、AWS S3 等）
#[derive(Debug, Clone)]
pub struct S3Config {
  /// S3 服务器 URL（例如：http://minio.example.com:9000 或 https://s3.amazonaws.com）
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

/// S3 兼容对象存储源
///
/// 提供对 S3 兼容对象存储的访问，支持 MinIO、AWS S3、阿里云 OSS 等
/// 搜索逻辑由 Server 端执行
pub struct S3Storage {
  config: S3Config,
  client: Arc<S3Client>,
}

impl S3Storage {
  /// 创建新的 S3 兼容存储源
  ///
  /// # 参数
  ///
  /// * `config` - S3 存储配置
  pub fn new(config: S3Config) -> Result<Self, StorageError> {
    debug!(
      "创建 S3 存储源: url={}, bucket={}, prefix={:?}",
      config.url, config.bucket, config.prefix
    );

    // 复用现有的客户端创建逻辑
    let client = legacy::get_or_create_s3_client(&config.url, &config.access_key, &config.secret_key)?;

    Ok(Self { config, client })
  }
}

#[async_trait]
impl DataSource for S3Storage {
  fn source_type(&self) -> &'static str {
    "S3Storage"
  }

  async fn list_files(&self) -> Result<FileIterator, StorageError> {
    let client = Arc::clone(&self.client);
    let bucket = self.config.bucket.clone();
    let prefix = self.config.prefix.clone();
    let pattern = self.config.pattern.clone();

    debug!("列举 S3 对象: bucket={}, prefix={:?}", bucket, prefix);

    let stream = stream! {
      let mut count = 0;
      let mut continuation_token: Option<String> = None;

      // AWS SDK 使用分页 API，需要循环处理
      loop {
        let mut request = client.list_objects_v2().bucket(&bucket);

        // 设置前缀
        if let Some(ref pfx) = prefix {
          request = request.prefix(pfx);
        }

        // 处理分页
        if let Some(token) = continuation_token {
          request = request.continuation_token(token);
        }

        let response = match request.send().await {
          Ok(resp) => resp,
          Err(e) => {
            warn!("S3 列举对象失败: {}", e);
            yield Err(legacy::StorageError::S3ListObjects(e.to_string()).into());
            break;
          }
        };

        // 处理返回的对象
        if let Some(contents) = response.contents {
          for object in contents {
            if let Some(key) = object.key {
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
                  size: object.size.map(|s| s as u64),
                  modified: object.last_modified.map(|dt| dt.secs()),
                  content_type: None,
                },
              };

              yield Ok(entry);
            }
          }
        }

        // 检查是否还有更多结果
        if response.is_truncated == Some(true) {
          continuation_token = response.next_continuation_token;
        } else {
          break;
        }
      }

      debug!("S3 对象列举完成: {} 个文件", count);
    };

    Ok(Box::new(Box::pin(stream)))
  }

  async fn open_file(&self, entry: &FileEntry) -> Result<FileReader, StorageError> {
    debug!("打开 S3 对象: bucket={}, key={}", self.config.bucket, entry.path);

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
  fn test_s3_config() {
    let config = S3Config {
      url: "http://localhost:9000".to_string(),
      access_key: "test_access_key".to_string(),
      secret_key: "test_secret_key".to_string(),
      bucket: "logs".to_string(),
      prefix: Some("2024/".to_string()),
      pattern: Some(r"\.log$".to_string()),
    };

    assert_eq!(config.bucket, "logs");
    assert_eq!(config.prefix, Some("2024/".to_string()));
  }

  #[test]
  fn test_matches_filter_prefix() {
    let _config = S3Config {
      url: "http://localhost:9000".to_string(),
      access_key: "test".to_string(),
      secret_key: "test".to_string(),
      bucket: "test".to_string(),
      prefix: Some("logs/".to_string()),
      pattern: None,
    };

    // 这个测试需要 S3 客户端，但我们只测试配置
    // 实际的集成测试应该在集成测试中进行
  }

  #[test]
  fn test_matches_filter_pattern() {
    let _config = S3Config {
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

  // 注意：S3 的实际集成测试需要运行的 S3 兼容服务器
  // 这些测试应该在 tests/ 目录下的集成测试中进行
  // 这里只测试配置和基本逻辑
}
