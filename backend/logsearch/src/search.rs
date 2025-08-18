use std::{
    io::{self},
    str::FromStr as _,
};

use async_trait::async_trait;
use minio::s3::{ClientBuilder, creds::StaticProvider, http::BaseUrl, types::S3Api as _};
use thiserror::Error;
use tokio::io::AsyncRead;
use tokio_util::io::{StreamReader, SyncIoBridge};

pub enum Source {
    S3 { bucket: String, key: String },
    Local { path: String },
    Http { url: String },
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("invalid base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("minio client build error: {0}")]
    MinioBuild(String),
    #[error("minio get_object error: {0}")]
    MinioGetObject(String),
    #[error("minio to_stream error: {0}")]
    MinioToStream(String),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

#[async_trait]
pub trait ReaderProvider {
    async fn open(&self) -> Result<Box<dyn AsyncRead + Send>, SearchError>;
}

pub struct S3ReaderProvider {
    url: String,
    access_key: String,
    secret_key: String,
    bucket: String,
    key: String,
}

impl S3ReaderProvider {
    pub fn new(
        url: String,
        access_key: String,
        secret_key: String,
        bucket: String,
        key: String,
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
impl ReaderProvider for S3ReaderProvider {
    async fn open(&self) -> Result<Box<dyn AsyncRead + Send>, SearchError> {
        let client = ClientBuilder::new(
            BaseUrl::from_str(&self.url).map_err(|e| SearchError::InvalidBaseUrl(e.to_string()))?,
        )
        .provider(Some(Box::new(StaticProvider::new(
            &self.access_key,
            &self.secret_key,
            None,
        ))))
        .build()
        .map_err(|e| SearchError::MinioBuild(e.to_string()))?;

        let (stream, _usize) = client
            .get_object(&self.bucket, &self.key)
            .send()
            .await
            .map_err(|e| SearchError::MinioGetObject(e.to_string()))?
            .content
            .to_stream()
            .await
            .map_err(|e| SearchError::MinioToStream(e.to_string()))?;

        Ok(Box::new(StreamReader::new(stream)))
    }
}
