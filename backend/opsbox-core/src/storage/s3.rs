use std::{io, sync::Arc, time::Duration};

use async_trait::async_trait;
use aws_sdk_s3::{
  Client as S3Client,
  config::{Credentials, Region},
  error::SdkError,
};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Mutex;
use thiserror::Error;
use tokio::io::AsyncRead;
use tokio::time;
use tracing::{debug, error, info, warn};

#[derive(Debug, Error)]
pub enum S3Error {
  #[error("S3 URL 不可用: {url}")]
  InvalidBaseUrl { url: String },
  #[error("创建 S3 客户端失败: {reason}")]
  S3Build { reason: String },
  #[error("S3 获取对象失败: bucket={bucket}, key={key}, error={error}")]
  S3GetObject { bucket: String, key: String, error: String },
  #[error("S3 流转换失败: bucket={bucket}, key={key}, error={error}")]
  S3ToStream { bucket: String, key: String, error: String },
  #[error("S3 列举对象失败: bucket={bucket}, prefix={prefix}, error={error}")]
  S3ListObjects {
    bucket: String,
    prefix: String,
    error: String,
  },
  #[error("无效正则表达式: {pattern}, error={error}")]
  Regex { pattern: String, error: String },
  #[error("IO错误: {path}, error={error}")]
  Io { path: String, error: String },
  #[error("S3 连接超时: bucket={bucket}, operation={operation}")]
  ConnectionTimeout { bucket: String, operation: String },
}

// 为 io::Error 提供自动转换（需要提供路径上下文）
impl From<io::Error> for S3Error {
  fn from(err: io::Error) -> Self {
    S3Error::Io {
      path: "unknown".to_string(), // 如果没有路径信息，使用默认值
      error: err.to_string(),
    }
  }
}

// 全局 S3 客户端缓存（按 url+access_key 维度缓存，避免切换配置后仍复用旧客户端）
static S3_CLIENT_CACHE: Lazy<Mutex<HashMap<String, Arc<S3Client>>>> = Lazy::new(|| Mutex::new(HashMap::new()));

// IO 操作超时配置（优先使用 OPSBOX_IO_TIMEOUT_SEC，其次 LOGSEEK_IO_TIMEOUT_SEC，默认 60 秒）
fn io_timeout() -> Duration {
  let secs = std::env::var("OPSBOX_IO_TIMEOUT_SEC")
    .ok()
    .or_else(|| std::env::var("LOGSEEK_IO_TIMEOUT_SEC").ok())
    .and_then(|s| s.parse::<u64>().ok())
    .unwrap_or(60)
    .clamp(5, 300);
  Duration::from_secs(secs)
}

// 创建或获取缓存的 S3 客户端（按 url+access_key 缓存）
pub fn get_or_create_s3_client(url: &str, access_key: &str, secret_key: &str) -> Result<Arc<S3Client>, S3Error> {
  let cache_key = format!("{}|{}", url, access_key);
  // 命中缓存则直接返回
  if let Some(existing) = S3_CLIENT_CACHE.lock().unwrap().get(&cache_key).cloned() {
    return Ok(existing);
  }

  info!("创建 S3 客户端: url={}", url);

  // 记录当前代理环境变量，便于排查连接问题
  let no_proxy_dbg = std::env::var("NO_PROXY")
    .ok()
    .or_else(|| std::env::var("no_proxy").ok());
  let http_proxy_dbg = std::env::var("HTTP_PROXY")
    .ok()
    .or_else(|| std::env::var("http_proxy").ok());
  let https_proxy_dbg = std::env::var("HTTPS_PROXY")
    .ok()
    .or_else(|| std::env::var("https_proxy").ok());
  debug!(
    "网络代理环境: HTTP_PROXY={:?} HTTPS_PROXY={:?} NO_PROXY={:?}",
    http_proxy_dbg, https_proxy_dbg, no_proxy_dbg
  );

  // 创建 AWS SDK 凭据
  let credentials = Credentials::new(
    access_key, secret_key, None,     // session_token
    None,     // expiry
    "static", // provider_name
  );

  // 配置 S3 客户端
  let config = aws_sdk_s3::Config::builder()
    .endpoint_url(url) // 支持 MinIO 和其他 S3 兼容服务
    .region(Region::new("oss-cn-beijing")) // MinIO 通常不关心 region，但 SDK 需要
    .credentials_provider(credentials)
    .force_path_style(false) // 根据服务类型动态选择，aws-config 默认自动处理，这里显式 false 可能需确认? LogSeek 原代码是 false
    .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest()) // AWS SDK 要求设置 behavior version
    .build();

  let client = Arc::new(S3Client::from_conf(config));

  // 写入缓存
  S3_CLIENT_CACHE.lock().unwrap().insert(cache_key, Arc::clone(&client));
  info!("S3 客户端创建并缓存成功");
  Ok(client)
}

/// 格式化 AWS SDK 错误，提取详细的错误信息
fn format_s3_error<E: std::fmt::Display + std::fmt::Debug>(err: &SdkError<E>) -> String {
  let mut parts = Vec::new();

  // 提取错误消息（基本消息）
  let message = err.to_string();
  if !message.is_empty() {
    parts.push(format!("message={}", message));
  }

  // 尝试从错误类型中提取更多信息
  match err {
    SdkError::ServiceError(service_err) => {
      // 服务错误：尝试提取错误类型
      let err_kind = service_err.err();
      let err_kind_str = format!("{:?}", err_kind);
      if !err_kind_str.is_empty() && err_kind_str != message {
        parts.push(format!("kind={}", err_kind_str));
      }

      // 尝试从响应中提取状态码
      let raw_response = service_err.raw();
      let status = raw_response.status();
      parts.push(format!("status={}", status.as_u16()));
    }
    SdkError::ConstructionFailure(err) => {
      parts.push(format!("construction_error={:?}", err));
    }
    SdkError::TimeoutError(err) => {
      parts.push(format!("timeout_error={:?}", err));
    }
    SdkError::DispatchFailure(err) => {
      parts.push(format!("dispatch_error={:?}", err));
    }
    SdkError::ResponseError(err) => {
      parts.push(format!("response_error={:?}", err));
    }
    _ => {
      // 对于其他类型的错误，使用Debug格式
      let debug_str = format!("{:?}", err);
      if debug_str != message && !debug_str.is_empty() {
        parts.push(format!("debug={}", debug_str));
      }
    }
  }

  if parts.is_empty() {
    "unknown error".to_string()
  } else {
    parts.join(", ")
  }
}

#[async_trait]
pub trait ReaderProvider {
  async fn open(&self) -> Result<Box<dyn AsyncRead + Send + Unpin>, S3Error>;
}

pub struct S3ReaderProvider<'a> {
  url: &'a str,
  access_key: &'a str,
  secret_key: &'a str,
  bucket: &'a str,
  key: &'a str,
}

impl<'a> S3ReaderProvider<'a> {
  pub fn new(url: &'a str, access_key: &'a str, secret_key: &'a str, bucket: &'a str, key: &'a str) -> Self {
    Self {
      url,
      access_key,
      secret_key,
      bucket,
      key,
    }
  }
}

#[async_trait]
impl<'a> ReaderProvider for S3ReaderProvider<'a> {
  async fn open(&self) -> Result<Box<dyn AsyncRead + Send + Unpin>, S3Error> {
    debug!(
      "开始打开S3对象: bucket={}, key={}, url={}",
      self.bucket, self.key, self.url
    );

    // 使用缓存的客户端
    let client = get_or_create_s3_client(self.url, self.access_key, self.secret_key)?;

    debug!("S3 客户端获取成功，开始获取对象");

    // 最多重试次数（指数退避），可由环境变量 LOGSEEK_IO_MAX_RETRIES 覆盖，默认 5 次
    let max_attempts: u32 = std::env::var("LOGSEEK_IO_MAX_RETRIES")
      .ok()
      .and_then(|s| s.parse::<u32>().ok())
      .unwrap_or(5)
      .clamp(1, 20);

    let mut attempt: u32 = 0;
    loop {
      let timeout = io_timeout();
      let fut = async {
        let response = client
          .get_object()
          .bucket(self.bucket)
          .key(self.key)
          .send()
          .await
          .map_err(|e| {
            let error_detail = format_s3_error(&e);
            error!(
              "获取S3对象失败: bucket={}, key={}, {}",
              self.bucket, self.key, error_detail
            );
            S3Error::S3GetObject {
              bucket: self.bucket.to_string(),
              key: self.key.to_string(),
              error: error_detail,
            }
          })?;

        // AWS SDK 返回 ByteStream，可以直接转换为兼容的流
        Ok::<_, S3Error>(response.body.into_async_read())
      };

      match time::timeout(timeout, fut).await {
        Ok(Ok(stream)) => {
          debug!("S3对象打开成功: bucket={}, key={}", self.bucket, self.key);
          return Ok(Box::new(stream));
        }
        Ok(Err(e)) => {
          attempt += 1;
          if attempt >= max_attempts {
            return Err(e);
          }
          let base_ms = 100u64.saturating_mul(1u64 << attempt.min(6));
          let delay = Duration::from_millis(base_ms);
          warn!(
            "获取S3对象失败，准备重试 第{}/{}次，延迟 {:?}",
            attempt, max_attempts, delay
          );
          time::sleep(delay).await;
        }
        Err(_) => {
          attempt += 1;
          if attempt >= max_attempts {
            error!("获取S3对象超时(重试到达上限): bucket={}, key={}", self.bucket, self.key);
            return Err(S3Error::ConnectionTimeout {
              bucket: self.bucket.to_string(),
              operation: "get_object".to_string(),
            });
          }
          let base_ms = 100u64.saturating_mul(1u64 << attempt.min(6));
          let delay = Duration::from_millis(base_ms);
          warn!(
            "获取S3对象超时，准备重试 第{}/{}次，延迟 {:?}",
            attempt, max_attempts, delay
          );
          time::sleep(delay).await;
        }
      }
    }
  }
}

impl<'a> S3ReaderProvider<'a> {
  /// 列出当前桶中满足前缀与可选正则过滤条件的对象键。
  pub async fn list_objects(&self, prefix: &str, regex: Option<&str>, recursive: bool) -> Result<Vec<String>, S3Error> {
    info!(
      "开始列举S3对象: bucket={}, prefix='{}', recursive={}, regex={:?}",
      self.bucket, prefix, recursive, regex
    );

    // 使用缓存的客户端
    let client = get_or_create_s3_client(self.url, self.access_key, self.secret_key)?;

    let regex = if let Some(pat) = regex {
      debug!("编译正则表达式: {}", pat);
      Some(Regex::new(pat).map_err(|e| {
        error!("正则表达式编译失败: {}, error: {}", pat, e);
        S3Error::Regex {
          pattern: pat.to_string(),
          error: e.to_string(),
        }
      })?)
    } else {
      None
    };

    // 使用超时包装列举操作
    let list_result = time::timeout(io_timeout(), async {
      debug!("开始列举S3对象");

      let mut keys = Vec::new();
      let mut processed_count = 0;
      let mut continuation_token: Option<String> = None;

      // AWS SDK 使用分页 API，需要循环处理
      loop {
        let mut request = client.list_objects_v2().bucket(self.bucket).prefix(prefix);

        // 如果不是递归，设置分隔符（只列举当前层级）
        if !recursive {
          request = request.delimiter("/");
        }

        // 处理分页
        if let Some(token) = continuation_token {
          request = request.continuation_token(token);
        }

        let response = request.send().await.map_err(|e| {
          let error_detail = format_s3_error(&e);
          error!(
            "列举S3对象失败: bucket={}, prefix={}, {}",
            self.bucket, prefix, error_detail
          );
          S3Error::S3ListObjects {
            bucket: self.bucket.to_string(),
            prefix: prefix.to_string(),
            error: error_detail,
          }
        })?;

        // 处理返回的对象
        if let Some(contents) = response.contents {
          for object in contents {
            if let Some(key) = object.key {
              processed_count += 1;

              if regex.as_ref().map(|r| r.is_match(&key)).unwrap_or(true) {
                debug!("对象匹配成功: {}", key);
                keys.push(key);
              } else {
                debug!("对象不匹配: {}", key);
              }
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

      Ok::<(Vec<String>, usize), S3Error>((keys, processed_count))
    })
    .await
    .map_err(|_| {
      error!("S3对象列举超时: bucket={}, prefix={}", self.bucket, prefix);
      S3Error::ConnectionTimeout {
        bucket: self.bucket.to_string(),
        operation: "list_objects_v2".to_string(),
      }
    })??;

    let (keys, processed_count) = list_result;

    info!(
      "S3对象列举完成: 处理{}个对象，匹配{}个结果",
      processed_count,
      keys.len()
    );
    Ok(keys)
  }
}

pub async fn test_s3_connection(url: &str, access_key: &str, secret_key: &str, bucket: &str) -> Result<(), S3Error> {
  info!("测试 S3 连接: url={}, bucket={}", url, bucket);

  // 使用缓存的客户端
  let client = get_or_create_s3_client(url, access_key, secret_key)?;

  debug!("尝试列举桶内对象以验证连接");

  // 使用超时包装连接测试
  time::timeout(io_timeout(), async {
    // 使用标准的 list_objects_v2 操作测试连接
    let _response = client
      .list_objects_v2()
      .bucket(bucket)
      .max_keys(1)
      .send()
      .await
      .map_err(|e| {
        let error_detail = format_s3_error(&e);
        error!("S3 连接测试失败: bucket={}, {}", bucket, error_detail);
        S3Error::S3ListObjects {
          bucket: bucket.to_string(),
          prefix: "".to_string(),
          error: error_detail,
        }
      })?;

    // 无论是否有对象，只要请求成功就说明连接正常
    debug!("S3连接测试成功，桶可访问");

    Ok::<(), S3Error>(())
  })
  .await
  .map_err(|_| {
    error!("S3 连接测试超时: bucket={}", bucket);
    S3Error::ConnectionTimeout {
      bucket: bucket.to_string(),
      operation: "test_connection".to_string(),
    }
  })??;

  info!("S3 连接测试成功");
  Ok(())
}
