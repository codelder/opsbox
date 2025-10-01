use std::{io, str::FromStr as _, sync::Arc, time::Duration};

use async_trait::async_trait;
use futures::StreamExt as _;
use minio::s3::types::ToStream;
use minio::s3::{ClientBuilder, creds::StaticProvider, http::BaseUrl, types::S3Api as _};
use regex::Regex;
use thiserror::Error;
use tokio::io::AsyncRead;
use tokio::time;
use tokio_util::io::StreamReader;
use log::{debug, info, warn, error};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Error)]
pub enum StorageError {
  #[error("url:{0}不可用")]
  InvalidBaseUrl(String),
  #[error("创建MinIO客户端失败")]
  MinioBuild,
  #[error("MinIO 获取对象错误：{0}")]
  MinioGetObject(String),
  #[error("MinIO to_stream 错误：{0}")]
  MinioToStream(String),
  #[error("MinIO 列举对象错误：{0}")]
  MinioListObjects(String),
  #[error("无效正则：{0}")]
  Regex(String),
  #[error("IO错误: {0}")]
  Io(#[from] io::Error),
  #[error("连接超时")]
  ConnectionTimeout,
}

// 全局 MinIO 客户端缓存（按 url+access_key 维度缓存，避免切换配置后仍复用旧客户端）
static MINIO_CLIENT_CACHE: Lazy<Mutex<HashMap<String, Arc<minio::s3::Client>>>> = Lazy::new(|| Mutex::new(HashMap::new()));

// MinIO 操作超时配置（可由环境变量 LOGSEARCH_MINIO_TIMEOUT_SEC 覆盖，默认 60 秒）
fn minio_timeout() -> Duration {
  if let Some(t) = crate::utils::tuning::get() { return Duration::from_secs(t.minio_timeout_sec.clamp(5, 300)); }
  let secs = std::env::var("LOGSEARCH_MINIO_TIMEOUT_SEC")
    .ok()
    .and_then(|s| s.parse::<u64>().ok())
    .unwrap_or(60)
    .clamp(5, 300);
  Duration::from_secs(secs)
}

// 创建或获取缓存的 MinIO 客户端（按 url+access_key 缓存）
fn get_or_create_minio_client(
  url: &str,
  access_key: &str,
  secret_key: &str,
) -> Result<Arc<minio::s3::Client>, StorageError> {
  let key = format!("{}|{}", url, access_key);
  // 命中缓存则直接返回
  if let Some(existing) = MINIO_CLIENT_CACHE.lock().unwrap().get(&key).cloned() {
    return Ok(existing);
  }

  info!("创建 MinIO 客户端: url={}", url);

  // 记录当前 NO_PROXY，便于排查是否因代理导致连接失败
  let no_proxy_dbg = std::env::var("NO_PROXY").ok().or_else(|| std::env::var("no_proxy").ok());
  let http_proxy_dbg = std::env::var("HTTP_PROXY").ok().or_else(|| std::env::var("http_proxy").ok());
  let https_proxy_dbg = std::env::var("HTTPS_PROXY").ok().or_else(|| std::env::var("https_proxy").ok());
  debug!("网络代理环境: HTTP_PROXY={:?} HTTPS_PROXY={:?} NO_PROXY={:?}", http_proxy_dbg, https_proxy_dbg, no_proxy_dbg);
  
  // 配置基础 URL
  let base_url = BaseUrl::from_str(url).map_err(|_e| {
    error!("MinIO URL解析失败: {}", url);
    StorageError::InvalidBaseUrl(url.to_string())
  })?;

  let builder = ClientBuilder::new(base_url)
    .provider(Some(Box::new(StaticProvider::new(
      access_key,
      secret_key,
      None,
    ))));

  let client = builder
    .build()
    .map_err(|_e| {
      error!("MinIO客户端构建失败");
      StorageError::MinioBuild
    })?;

  let client = Arc::new(client);
  // 写入缓存（覆盖同 key）
  MINIO_CLIENT_CACHE.lock().unwrap().insert(key, Arc::clone(&client));
  info!("MinIO 客户端创建并缓存成功");
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
    debug!("开始打开S3对象: bucket={}, key={}, url={}", self.bucket, self.key, self.url);
    
    // 使用缓存的客户端
    let client = get_or_create_minio_client(self.url, self.access_key, self.secret_key)?;

    debug!("MinIO客户端获取成功，开始获取对象");

    // 最多重试次数（指数退避），可由环境变量 LOGSEARCH_MINIO_MAX_ATTEMPTS 覆盖，默认 5 次
    let max_attempts: u32 = if let Some(t) = crate::utils::tuning::get() {
      t.minio_max_attempts.clamp(1, 20)
    } else {
      std::env::var("LOGSEARCH_MINIO_MAX_ATTEMPTS").ok().and_then(|s| s.parse::<u32>().ok()).unwrap_or(5).clamp(1, 20)
    };

    let mut attempt: u32 = 0;
    loop {
      let timeout = minio_timeout();
      let fut = async {
        client
          .get_object(self.bucket, self.key)
          .send()
          .await
          .map_err(|e| {
            error!("获取S3对象失败: bucket={}, key={}, error={}", self.bucket, self.key, e);
            StorageError::MinioGetObject(e.to_string())
          })?
          .content
          .to_stream()
          .await
          .map_err(|e| {
            error!("S3对象转换为流失败: bucket={}, key={}, error={}", self.bucket, self.key, e);
            StorageError::MinioToStream(e.to_string())
          })
      };

      match time::timeout(timeout, fut).await {
        Ok(Ok((stream, _file_size))) => {
          info!("S3对象打开成功: bucket={}, key={}", self.bucket, self.key);
          return Ok(Box::new(StreamReader::new(stream)));
        }
        Ok(Err(e)) => {
          attempt += 1;
          if attempt >= max_attempts {
            return Err(e);
          }
          let base_ms = 100u64.saturating_mul(1u64 << attempt.min(6));
          let delay = Duration::from_millis(base_ms);
          warn!("获取S3对象失败，准备重试 第{}/{}次，延迟 {:?}", attempt, max_attempts, delay);
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
          warn!("获取S3对象超时，准备重试 第{}/{}次，延迟 {:?}", attempt, max_attempts, delay);
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
    info!("开始列举S3对象: bucket={}, prefix='{}', recursive={}, regex={:?}", 
          self.bucket, prefix, recursive, regex);
    
    // 使用缓存的客户端
    let client = get_or_create_minio_client(self.url, self.access_key, self.secret_key)?;

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
    let list_result = time::timeout(minio_timeout(), async {
      let mut stream = client
        .list_objects(self.bucket)
        .prefix(Some(prefix.to_string()))
        .recursive(recursive)
        .to_stream()
        .await;

      debug!("开始遍历S3对象列表");
      
      let mut keys = Vec::new();
      let mut processed_count = 0;
      
      while let Some(item) = stream.next().await {
        let obj = item.map_err(|e| {
          error!("列举对象出错: error={:?}", e);
          StorageError::MinioListObjects(e.to_string())
        })?;
        // 在 minio 0.3.x 中，对象键通常通过 `name` 字段提供
        let key = obj.name;
        processed_count += 1;

        if regex.as_ref().map(|r| r.is_match(&key)).unwrap_or(true) {
          debug!("对象匹配成功: {}", key);
          keys.push(key);
        } else {
          debug!("对象不匹配: {}", key);
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

    info!("S3对象列举完成: 处理{}个对象，匹配{}个结果", processed_count, keys.len());
    Ok(keys)
  }
}

pub async fn test_minio_connection(
  url: &str,
  access_key: &str,
  secret_key: &str,
  bucket: &str,
) -> Result<(), StorageError> {
  info!("测试MinIO连接: url={}, bucket={}", url, bucket);
  
  // 使用缓存的客户端
  let client = get_or_create_minio_client(url, access_key, secret_key)?;

  debug!("尝试列举桶内对象以验证连接");
  
  // 使用超时包装连接测试
  time::timeout(minio_timeout(), async {
    let mut stream = client
      .list_objects(bucket)
      .recursive(false)
      .to_stream()
      .await;

    // 触发一次迭代以验证凭证与桶可访问性；桶为空也视作成功
    if let Some(item) = stream.next().await {
      item.map_err(|e| {
        error!("MinIO连接测试失败: {:?}", e);
        StorageError::MinioListObjects(e.to_string())
      })?;
      debug!("找到至少一个对象，连接正常");
    } else {
      debug!("桶为空，但连接正常");
    }
    Ok::<(), StorageError>(())
  })
  .await
  .map_err(|_| {
    error!("MinIO连接测试超时: bucket={}", bucket);
    StorageError::ConnectionTimeout
  })??;

  info!("MinIO连接测试成功");
  Ok(())
}
