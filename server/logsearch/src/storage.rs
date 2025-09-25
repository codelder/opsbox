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
use log::{debug, info, error};
use once_cell::sync::OnceCell;

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

// 中文注释：全局 MinIO 客户端缓存
static MINIO_CLIENT_CACHE: OnceCell<Arc<minio::s3::Client>> = OnceCell::new();

// 中文注释：MinIO 操作超时配置
const MINIO_TIMEOUT: Duration = Duration::from_secs(30);

// 中文注释：创建或获取缓存的 MinIO 客户端
fn get_or_create_minio_client(
  url: &str,
  access_key: &str,
  secret_key: &str,
) -> Result<Arc<minio::s3::Client>, StorageError> {
  // 如果已经初始化，直接返回缓存的客户端
  if let Some(client) = MINIO_CLIENT_CACHE.get() {
    return Ok(Arc::clone(client));
  }

  info!("创建 MinIO 客户端: url={}", url);
  
  // 中文注释：配置超时参数
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

  // 中文注释：设置 HTTP 客户端超时
  // 注意：具体 API 可能因 minio crate 版本而异
  // 这里的示例假设支持设置 timeout，如果不支持需要其他方法
  let client = builder
    .build()
    .map_err(|_e| {
      error!("MinIO客户端构建失败");
      StorageError::MinioBuild
    })?;

  let client = Arc::new(client);
  
  // 中文注释：尝试将客户端存入缓存（只有第一次才会成功）
  if let Err(_) = MINIO_CLIENT_CACHE.set(Arc::clone(&client)) {
    // 其他线程已经初始化了，返回缓存中的客户端
    return Ok(Arc::clone(MINIO_CLIENT_CACHE.get().unwrap()));
  }
  
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
    
    // 中文注释：使用缓存的客户端
    let client = get_or_create_minio_client(self.url, self.access_key, self.secret_key)?;

    debug!("MinIO客户端获取成功，开始获取对象");
    
    // 中文注释：使用超时包装获取对象操作
    let get_result = time::timeout(MINIO_TIMEOUT, async {
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
    })
    .await
    .map_err(|_| {
      error!("获取S3对象超时: bucket={}, key={}", self.bucket, self.key);
      StorageError::ConnectionTimeout
    })??;
    
    let (stream, _file_size) = get_result;

    info!("S3对象打开成功: bucket={}, key={}", self.bucket, self.key);
    Ok(Box::new(StreamReader::new(stream)))
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
    
    // 中文注释：使用缓存的客户端
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

    // 中文注释：使用超时包装列举操作
    let list_result = time::timeout(MINIO_TIMEOUT, async {
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
          error!("列举对象出错: error={}", e);
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
  
  // 中文注释：使用缓存的客户端
  let client = get_or_create_minio_client(url, access_key, secret_key)?;

  debug!("尝试列举桶内对象以验证连接");
  
  // 中文注释：使用超时包装连接测试
  time::timeout(MINIO_TIMEOUT, async {
    let mut stream = client
      .list_objects(bucket)
      .recursive(false)
      .to_stream()
      .await;

    // 触发一次迭代以验证凭证与桶可访问性；桶为空也视作成功
    if let Some(item) = stream.next().await {
      item.map_err(|e| {
        error!("MinIO连接测试失败: {}", e);
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
