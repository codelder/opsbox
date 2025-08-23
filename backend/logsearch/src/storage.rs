use std::{io, str::FromStr as _};

use async_trait::async_trait;
use minio::s3::{ClientBuilder, creds::StaticProvider, http::BaseUrl, types::S3Api as _};
use thiserror::Error;
use tokio::io::AsyncRead;
use tokio_util::io::StreamReader;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("url:{0}不可用")]
    InvalidBaseUrl(String),
    #[error("创建MinIO客户端失败")]
    MinioBuild,
    #[error("minio get_object error: {0}")]
    MinioGetObject(String),
    #[error("minio to_stream error: {0}")]
    MinioToStream(String),
    #[error("io error: {0}")]
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
    pub fn new(
        url: &'a str,
        access_key: &'a str,
        secret_key: &'a str,
        bucket: &'a str,
        key: &'a str,
    ) -> Self {
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
        let client = ClientBuilder::new(
            BaseUrl::from_str(self.url)
                .map_err(|_e| StorageError::InvalidBaseUrl(self.url.to_string()))?,
        )
        .provider(Some(Box::new(StaticProvider::new(
            self.access_key,
            self.secret_key,
            None,
        ))))
        .build()
        .map_err(|_e| StorageError::MinioBuild)?;

        let (stream, _usize) = client
            .get_object(self.bucket, self.key)
            .send()
            .await
            .map_err(|e| StorageError::MinioGetObject(e.to_string()))?
            .content
            .to_stream()
            .await
            .map_err(|e| StorageError::MinioToStream(e.to_string()))?;

        Ok(Box::new(StreamReader::new(stream)))
    }
}
