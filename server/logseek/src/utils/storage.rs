use std::{io, sync::Arc, time::Duration};

use async_trait::async_trait;
use aws_sdk_s3::{
  Client as S3Client,
  config::{Credentials, Region},
};
use log::{debug, error, info, warn};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Mutex;
use thiserror::Error;
use tokio::io::AsyncRead;
use tokio::time;

#[derive(Debug, Error)]
pub enum StorageError {
  #[error("url:{0}不可用")]
  InvalidBaseUrl(String),
  #[error("创建 S3 客户端失败")]
  S3Build,
  #[error("S3 获取对象错误：{0}")]
  S3GetObject(String),
  #[error("S3 to_stream 错误：{0}")]
  S3ToStream(String),
  #[error("S3 列举对象错误：{0}")]
  S3ListObjects(String),
  #[error("无效正则：{0}")]
  Regex(String),
  #[error("IO错误: {0}")]
  Io(#[from] io::Error),
  #[error("连接超时")]
  ConnectionTimeout,
}

// 全局 S3 客户端缓存（按 url+access_key 维度缓存，避免切换配置后仍复用旧客户端）
static S3_CLIENT_CACHE: Lazy<Mutex<HashMap<String, Arc<S3Client>>>> = Lazy::new(|| Mutex::new(HashMap::new()));

// S3 操作超时配置（可由环境变量 LOGSEEK_S3_TIMEOUT_SEC 覆盖，默认 60 秒）
fn s3_timeout() -> Duration {
  if let Some(t) = crate::utils::tuning::get() {
    return Duration::from_secs(t.s3_timeout_sec.clamp(5, 300));
  }
  let secs = std::env::var("LOGSEEK_S3_TIMEOUT_SEC")
    .ok()
    .and_then(|s| s.parse::<u64>().ok())
    .unwrap_or(60)
    .clamp(5, 300);
  Duration::from_secs(secs)
}

// 创建或获取缓存的 S3 客户端（按 url+access_key 缓存）
pub fn get_or_create_s3_client(url: &str, access_key: &str, secret_key: &str) -> Result<Arc<S3Client>, StorageError> {
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
    .region(Region::new("us-east-1")) // MinIO 通常不关心 region，但 SDK 需要
    .credentials_provider(credentials)
    .force_path_style(true) // MinIO 需要路径风格访问（bucket-name/key 而非 bucket-name.domain/key）
    .build();

  let client = Arc::new(S3Client::from_conf(config));

  // 写入缓存
  S3_CLIENT_CACHE.lock().unwrap().insert(cache_key, Arc::clone(&client));
  info!("S3 客户端创建并缓存成功");
  Ok(client)
}

#[async_trait]
pub trait ReaderProvider {
  async fn open(&self) -> Result<Box<dyn AsyncRead + Send + Unpin>, StorageError>;
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
  async fn open(&self) -> Result<Box<dyn AsyncRead + Send + Unpin>, StorageError> {
    debug!(
      "开始打开S3对象: bucket={}, key={}, url={}",
      self.bucket, self.key, self.url
    );

    // 使用缓存的客户端
    let client = get_or_create_s3_client(self.url, self.access_key, self.secret_key)?;

    debug!("S3 客户端获取成功，开始获取对象");

    // 最多重试次数（指数退避），可由环境变量 LOGSEEK_S3_MAX_RETRIES 覆盖，默认 5 次
    let max_attempts: u32 = if let Some(t) = crate::utils::tuning::get() {
      t.s3_max_retries.clamp(1, 20)
    } else {
      std::env::var("LOGSEEK_S3_MAX_RETRIES")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(5)
        .clamp(1, 20)
    };

    let mut attempt: u32 = 0;
    loop {
      let timeout = s3_timeout();
      let fut = async {
        let response = client
          .get_object()
          .bucket(self.bucket)
          .key(self.key)
          .send()
          .await
          .map_err(|e| {
            error!("获取S3对象失败: bucket={}, key={}, error={}", self.bucket, self.key, e);
            StorageError::S3GetObject(e.to_string())
          })?;

        // AWS SDK 返回 ByteStream，可以直接转换为兼容的流
        Ok::<_, StorageError>(response.body.into_async_read())
      };

      match time::timeout(timeout, fut).await {
        Ok(Ok(stream)) => {
          info!("S3对象打开成功: bucket={}, key={}", self.bucket, self.key);
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
            return Err(StorageError::ConnectionTimeout);
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
  ///
  /// - prefix：仅列出以此前缀开头的对象键。
  /// - regex：可选的正则表达式用于进一步过滤；为 None 时返回该前缀下的所有对象。
  /// - recursive：是否递归遍历子路径（true）或仅列出当前层级（false）。
  ///
  /// 返回符合过滤条件的对象键（完整路径）列表。
  pub async fn list_objects(
    &self,
    prefix: &str,
    regex: Option<&str>,
    recursive: bool,
  ) -> Result<Vec<String>, StorageError> {
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
        StorageError::Regex(e.to_string())
      })?)
    } else {
      None
    };

    // 使用超时包装列举操作
    let list_result = time::timeout(s3_timeout(), async {
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
          error!("列举S3对象失败: error={:?}", e);
          StorageError::S3ListObjects(e.to_string())
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

      Ok::<(Vec<String>, usize), StorageError>((keys, processed_count))
    })
    .await
    .map_err(|_| {
      error!("S3对象列举超时: bucket={}, prefix={}", self.bucket, prefix);
      StorageError::ConnectionTimeout
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

pub async fn test_s3_connection(
  url: &str,
  access_key: &str,
  secret_key: &str,
  bucket: &str,
) -> Result<(), StorageError> {
  info!("测试 S3 连接: url={}, bucket={}", url, bucket);

  // 使用缓存的客户端
  let client = get_or_create_s3_client(url, access_key, secret_key)?;

  debug!("尝试列举桶内对象以验证连接");

  // 使用超时包装连接测试
  time::timeout(s3_timeout(), async {
    // 尝试列举桶内对象（最多1个）以验证凭证和桶可访问性
    let response = client
      .list_objects_v2()
      .bucket(bucket)
      .max_keys(1)
      .send()
      .await
      .map_err(|e| {
        error!("S3 连接测试失败: {:?}", e);
        StorageError::S3ListObjects(e.to_string())
      })?;

    // 无论是否有对象，只要请求成功就说明连接正常
    if response.contents.as_ref().map(|c| !c.is_empty()).unwrap_or(false) {
      debug!("找到对象，连接正常");
    } else {
      debug!("桶为空或无权限列举，但连接正常");
    }

    Ok::<(), StorageError>(())
  })
  .await
  .map_err(|_| {
    error!("S3 连接测试超时: bucket={}", bucket);
    StorageError::ConnectionTimeout
  })??;

  info!("S3 连接测试成功");
  Ok(())
}
