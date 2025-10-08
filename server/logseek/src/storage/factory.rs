// ============================================================================
// 存储源工厂 - 根据配置创建存储源实例
// ============================================================================

use super::{agent::AgentClient, local::LocalFileSystem, s3::S3Storage, StorageError, StorageSource};
use crate::repository::settings;
use log::{debug, info, warn};
use opsbox_core::SqlitePool;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

// ============================================================================
// 存储源配置
// ============================================================================

/// 存储源配置
///
/// 用于从请求参数描述需要搜索的存储源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SourceConfig {
  /// 本地文件系统配置
  Local {
    /// 根目录路径
    path: String,
    /// 是否递归搜索
    #[serde(default = "default_true")]
    recursive: bool,
  },

  /// S3 配置(使用 profile 名称)
  S3 {
    /// Profile 名称
    profile: String,
    /// Bucket 名称 (用于 FileUrl 构造)
    #[serde(skip_serializing_if = "Option::is_none")]
    bucket: Option<String>,
    /// 路径前缀(可选) - 当 key 为 None 时使用
    #[serde(skip_serializing_if = "Option::is_none")]
    prefix: Option<String>,
    /// 路径过滤正则(可选) - 当 key 为 None 时使用
    #[serde(skip_serializing_if = "Option::is_none")]
    pattern: Option<String>,
    /// 特定对象键(可选) - 当指定时，只搜索该对象，忽略 prefix 和 pattern
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
  },

  /// Agent 配置
  Agent {
    /// Agent 端点 URL (例如: "http://192.168.1.10:8090")
    endpoint: String,
  },
}

fn default_true() -> bool {
  true
}

// ============================================================================
// 存储源工厂
// ============================================================================

/// 存储源工厂
///
/// 负责根据配置创建各种类型的存储源
pub struct StorageFactory {
  db_pool: SqlitePool,
}

impl StorageFactory {
  /// 创建新的存储源工厂
  pub fn new(db_pool: SqlitePool) -> Self {
    Self { db_pool }
  }

  /// 从配置创建存储源
  ///
  /// # 参数
  ///
  /// * `config` - 存储源配置
  ///
  /// # 返回
  ///
  /// 返回 StorageSource 枚举，包含 DataSource 或 SearchService
  pub async fn create_source(&self, config: SourceConfig) -> Result<StorageSource, StorageError> {
    match config {
      SourceConfig::Local { path, recursive } => {
        info!("创建本地文件系统存储源: path={}, recursive={}", path, recursive);
        self.create_local_source(path, recursive).await
      }

      SourceConfig::S3 { profile, bucket, prefix, pattern, key } => {
        if let Some(ref k) = key {
          info!("创建 S3 存储源: profile={}, key={}", profile, k);
        } else {
          info!("创建 S3 存储源: profile={}, prefix={:?}", profile, prefix);
        }
        self.create_s3_source(profile, bucket, prefix, pattern, key).await
      }

      SourceConfig::Agent { endpoint } => {
        info!("创建 Agent 客户端: endpoint={}", endpoint);
        self.create_agent_source(endpoint).await
      }
    }
  }

  /// 创建本地文件系统存储源
  async fn create_local_source(&self, path: String, recursive: bool) -> Result<StorageSource, StorageError> {
    let path_buf = PathBuf::from(&path);

    // 验证路径存在
    if !tokio::fs::metadata(&path_buf).await.is_ok() {
      return Err(StorageError::NotFound(format!("路径不存在: {}", path)));
    }

    let source = LocalFileSystem::new(path_buf).with_recursive(recursive);

    Ok(StorageSource::Data(Arc::new(source)))
  }

  /// 创建 S3 存储源
  async fn create_s3_source(
    &self,
    profile_name: String,
    _bucket_name: Option<String>, // 不再使用，从 profile 中获取
    prefix: Option<String>,
    pattern: Option<String>,
    key: Option<String>,
  ) -> Result<StorageSource, StorageError> {
    // 从数据库加载 profile
    let profile = settings::load_s3_profile(&self.db_pool, &profile_name)
      .await
      .map_err(|e| StorageError::Other(format!("加载 S3 Profile 失败: {:?}", e)))?
      .ok_or_else(|| StorageError::NotFound(format!("S3 Profile 不存在: {}", profile_name)))?;

    debug!("加载的 S3 Profile: profile_name={}, endpoint={}, bucket={}", 
           profile.profile_name, profile.endpoint, profile.bucket);

    // 构造 S3Config
    // 当 key 被指定时，将其作为 prefix 传递，并忽略 pattern
    let (final_prefix, final_pattern) = if let Some(k) = key {
      (Some(k), None)
    } else {
      (prefix, pattern)
    };

    let s3_config = super::s3::S3Config {
      url: profile.endpoint,
      access_key: profile.access_key,
      secret_key: profile.secret_key,
      bucket: profile.bucket,
      prefix: final_prefix,
      pattern: final_pattern,
    };

    // 创建 S3Storage
    let storage = S3Storage::new(s3_config)?;

    Ok(StorageSource::Data(Arc::new(storage)))
  }

  /// 创建 Agent 客户端
  async fn create_agent_source(&self, endpoint: String) -> Result<StorageSource, StorageError> {
    // 验证 endpoint 格式
    if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
      return Err(StorageError::Other(format!("Agent endpoint 必须是有效的 HTTP URL: {}", endpoint)));
    }

    // 使用 endpoint 作为 agent_id
    let client = AgentClient::new(endpoint.clone(), endpoint);

    // 验证 Agent 是否在线
    if !client.health_check().await {
      return Err(StorageError::AgentUnavailable("Agent 健康检查失败".to_string()));
    }

    Ok(StorageSource::Service(Arc::new(client)))
  }

  /// 批量创建存储源（支持同时创建多个不同类型的存储源）
  ///
  /// # 参数
  ///
  /// * `configs` - 存储源配置列表
  ///
  /// # 返回
  ///
  /// 返回成功创建的存储源列表和失败的错误信息
  pub async fn create_sources(
    &self,
    configs: Vec<SourceConfig>,
  ) -> (Vec<StorageSource>, Vec<SourceCreationError>) {
    let mut sources = Vec::new();
    let mut errors = Vec::new();

    for (idx, config) in configs.into_iter().enumerate() {
      match self.create_source(config.clone()).await {
        Ok(source) => {
          sources.push(source);
        }
        Err(e) => {
          warn!("创建存储源失败 (索引 {}): {:?}", idx, e);
          errors.push(SourceCreationError {
            index: idx,
            config,
            error: e.to_string(),
          });
        }
      }
    }

    info!(
      "批量创建存储源完成: 成功 {}, 失败 {}",
      sources.len(),
      errors.len()
    );

    (sources, errors)
  }
}

/// 存储源创建错误
#[derive(Debug, Clone, Serialize)]
pub struct SourceCreationError {
  /// 配置在列表中的索引
  pub index: usize,
  /// 失败的配置
  #[serde(skip)]
  pub config: SourceConfig,
  /// 错误信息
  pub error: String,
}

// 手动实现 From<StorageError> 转换
impl SourceCreationError {
  fn new(index: usize, config: SourceConfig, error: StorageError) -> Self {
    Self {
      index,
      config,
      error: error.to_string(),
    }
  }
}

// 为 create_sources 提供辅助方法
impl StorageFactory {
  /// 批量创建存储源（返回详细错误信息）
  pub async fn try_create_sources(
    &self,
    configs: Vec<SourceConfig>,
  ) -> Result<Vec<StorageSource>, Vec<SourceCreationError>> {
    let (sources, errors) = self.create_sources(configs).await;

    if errors.is_empty() {
      Ok(sources)
    } else {
      Err(errors)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_source_config_serde_local() {
    let local_config = SourceConfig::Local {
      path: "/var/log".to_string(),
      recursive: true,
    };
    let json = serde_json::to_string(&local_config).unwrap();
    assert!(json.contains("\"type\":\"local\""));
    assert!(json.contains("/var/log"));

    // 测试反序列化
    let deserialized: SourceConfig = serde_json::from_str(&json).unwrap();
    match deserialized {
      SourceConfig::Local { path, recursive } => {
        assert_eq!(path, "/var/log");
        assert_eq!(recursive, true);
      }
      _ => panic!("Expected Local config"),
    }
  }

  #[test]
  fn test_source_config_serde_s3() {
    let s3_config = SourceConfig::S3 {
      profile: "default".to_string(),
      bucket: Some("my-bucket".to_string()),
      prefix: Some("logs/".to_string()),
      pattern: Some(r"\.log$".to_string()),
      key: None,
    };
    let json = serde_json::to_string(&s3_config).unwrap();
    assert!(json.contains("\"type\":\"s3\""));
    assert!(json.contains("default"));

    // 测试反序列化
    let deserialized: SourceConfig = serde_json::from_str(&json).unwrap();
    match deserialized {
      SourceConfig::S3 { profile, bucket, prefix, pattern, key } => {
        assert_eq!(profile, "default");
        assert_eq!(bucket, Some("my-bucket".to_string()));
        assert_eq!(prefix, Some("logs/".to_string()));
        assert_eq!(pattern, Some(r"\.log$".to_string()));
        assert_eq!(key, None);
      }
      _ => panic!("Expected S3 config"),
    }
  }

  #[test]
  fn test_source_config_serde_agent() {
    let agent_config = SourceConfig::Agent {
      endpoint: "http://192.168.1.10:8090".to_string(),
    };
    let json = serde_json::to_string(&agent_config).unwrap();
    assert!(json.contains("\"type\":\"agent\""));
    assert!(json.contains("192.168.1.10"));

    // 测试反序列化
    let deserialized: SourceConfig = serde_json::from_str(&json).unwrap();
    match deserialized {
      SourceConfig::Agent { endpoint } => {
        assert_eq!(endpoint, "http://192.168.1.10:8090");
      }
      _ => panic!("Expected Agent config"),
    }
  }

  #[test]
  fn test_source_config_s3_optional_fields() {
    // 测试可选字段被省略时的序列化
    let s3_config = SourceConfig::S3 {
      profile: "production".to_string(),
      bucket: None,
      prefix: None,
      pattern: None,
      key: None,
    };
    let json = serde_json::to_string(&s3_config).unwrap();

    // 确保 None 值不会出现在 JSON 中
    assert!(!json.contains("bucket"));
    assert!(!json.contains("prefix"));
    assert!(!json.contains("pattern"));
    assert!(!json.contains("key"));
  }

  #[test]
  fn test_multiple_sources_config() {
    // 测试多个存储源的配置
    let configs = vec![
      SourceConfig::S3 {
        profile: "s3-prod-1".to_string(),
        bucket: Some("bucket-1".to_string()),
        prefix: Some("logs/".to_string()),
        pattern: None,
        key: None,
      },
      SourceConfig::S3 {
        profile: "s3-prod-2".to_string(),
        bucket: Some("bucket-2".to_string()),
        prefix: Some("metrics/".to_string()),
        pattern: None,
        key: None,
      },
      SourceConfig::Agent {
        endpoint: "http://agent1:8090".to_string(),
      },
      SourceConfig::Agent {
        endpoint: "http://agent2:8090".to_string(),
      },
      SourceConfig::Agent {
        endpoint: "http://agent3:8090".to_string(),
      },
      SourceConfig::Agent {
        endpoint: "http://agent4:8090".to_string(),
      },
      SourceConfig::Local {
        path: "/var/log".to_string(),
        recursive: true,
      },
    ];

    // 测试序列化
    let json = serde_json::to_string(&configs).unwrap();
    assert!(json.contains("s3-prod-1"));
    assert!(json.contains("s3-prod-2"));
    assert!(json.contains("agent1"));
    assert!(json.contains("agent4"));

    // 测试反序列化
    let deserialized: Vec<SourceConfig> = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.len(), 7);
  }
}
