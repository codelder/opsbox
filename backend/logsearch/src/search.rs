use std::{
    io::{self, BufReader, Read},
    pin::Pin,
    str::FromStr as _,
};

use async_trait::async_trait;
use flate2::read::GzDecoder;
use minio::s3::{ClientBuilder, creds::StaticProvider, http::BaseUrl, types::S3Api as _};
use thiserror::Error;
use tokio::io::AsyncRead;
use tokio_util::io::{StreamReader, SyncIoBridge};

use crate::log_storage::grep_context_from_reader;

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
    async fn open(&self) -> Result<Box<dyn AsyncRead + Send>, SearchError> {
        let client = ClientBuilder::new(
            BaseUrl::from_str(self.url).map_err(|e| SearchError::InvalidBaseUrl(e.to_string()))?,
        )
        .provider(Some(Box::new(StaticProvider::new(
            self.access_key,
            self.secret_key,
            None,
        ))))
        .build()
        .map_err(|e| SearchError::MinioBuild(e.to_string()))?;

        let (stream, _usize) = client
            .get_object(self.bucket, self.key)
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

pub trait Search {
    fn search(
        self,
        keyword: &str,
        context_lines: usize,
        emit: impl FnMut(String),
        renderer: impl FnMut(String, Vec<String>, Vec<(usize, usize)>) -> String,
    ) -> Result<(), SearchError>;
}

impl Search for Box<dyn Read + Send> {
    fn search(
        self,
        keyword: &str,
        context_lines: usize,
        mut emit: impl FnMut(String),
        mut renderer: impl FnMut(String, Vec<String>, Vec<(usize, usize)>) -> String,
    ) -> Result<(), SearchError> {
        let mut archive = tar::Archive::new(GzDecoder::new(self));
        for entry in archive.entries()?.flatten() {
            let path = entry
                .path()
                .ok()
                .map(|p| p.into_owned().display().to_string()) // 拿到 owned String
                .unwrap_or_default();
            let mut reader = BufReader::with_capacity(8192, entry);
            if let Ok(Some((lines, merged))) =
                grep_context_from_reader(&mut reader, keyword, context_lines)
            {
                emit(renderer(path, lines, merged));
            }
        }
        Ok(())
    }
}
