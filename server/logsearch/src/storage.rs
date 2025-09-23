use std::{io, str::FromStr as _};

use async_trait::async_trait;
use futures::StreamExt as _;
use minio::s3::types::ToStream;
use minio::s3::{ClientBuilder, creds::StaticProvider, http::BaseUrl, types::S3Api as _};
use regex::Regex;
use thiserror::Error;
use tokio::io::AsyncRead;
use tokio_util::io::StreamReader;
use log::{debug, info, error};

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
    
    let client = ClientBuilder::new(
      BaseUrl::from_str(self.url).map_err(|_e| {
        error!("S3 URL解析失败: {}", self.url);
        StorageError::InvalidBaseUrl(self.url.to_string())
      })?)
      .provider(Some(Box::new(StaticProvider::new(
        self.access_key,
        self.secret_key,
        None,
      ))))
      .build()
      .map_err(|_e| {
        error!("MinIO客户端构建失败: bucket={}, key={}", self.bucket, self.key);
        StorageError::MinioBuild
      })?;

    debug!("MinIO客户端创建成功，开始获取对象");
    
    let (stream, _file_size) = client
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
      })?;

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
    
    // 构建客户端（与 open() 中一致）。
    let client = ClientBuilder::new(
      BaseUrl::from_str(self.url).map_err(|_e| {
        error!("S3 URL解析失败: {}", self.url);
        StorageError::InvalidBaseUrl(self.url.to_string())
      })?)
      .provider(Some(Box::new(StaticProvider::new(
        self.access_key,
        self.secret_key,
        None,
      ))))
      .build()
      .map_err(|_e| {
        error!("构建MinIO客户端失败用于列举操作");
        StorageError::MinioBuild
      })?;

    let regex = if let Some(pat) = regex {
      debug!("编译正则表达式: {}", pat);
      Some(Regex::new(pat).map_err(|e| {
        error!("正则表达式编译失败: {}, error: {}", pat, e);
        StorageError::Regex(e.to_string())
      })?)
    } else {
      None
    };

    let mut keys = Vec::new();
    let mut processed_count = 0;

    // 从 MinIO 持续拉取列举结果，分页由客户端处理。
    let mut stream = client
      .list_objects(self.bucket)
      .prefix(Some(prefix.to_string()))
      .recursive(recursive)
      .to_stream()
      .await;

    debug!("开始遍历S3对象列表");
    
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
  
  let client = ClientBuilder::new(
    BaseUrl::from_str(url).map_err(|_e| {
      error!("MinIO URL解析失败: {}", url);
      StorageError::InvalidBaseUrl(url.to_string())
    })?)
    .provider(Some(Box::new(StaticProvider::new(access_key, secret_key, None))))
    .build()
    .map_err(|_e| {
      error!("MinIO客户端构建失败用于连接测试");
      StorageError::MinioBuild
    })?;

  debug!("尝试列举桶内对象以验证连接");
  
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

  info!("MinIO连接测试成功");
  Ok(())
}
