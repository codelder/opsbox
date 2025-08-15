use std::pin::Pin;
use std::{io, sync::Arc};

use async_trait::async_trait;
use bytes::Bytes;
use futures_core::stream::Stream;
use futures_util::stream::StreamExt;
use minio::s3::{types::S3Api, Client as MinioClient};
use tokio_util::io::ReaderStream;

/// 统一的字节流类型，兼容 `tokio_util::io::StreamReader`
pub type BoxByteTryStream = Pin<Box<dyn Stream<Item = Result<Bytes, io::Error>> + Send + 'static>>;

/// 日志存储后端接口：
/// - 约定返回一个以 `Bytes` 为元素的 `TryStream`，错误为 `io::Error`
/// - `locator` 可以是文件路径、对象 key 等
#[async_trait]
pub trait LogStorageBackend: Send + Sync {
    async fn open_archive_stream(&self, locator: &str) -> Result<BoxByteTryStream, io::Error>;
}

/// 文件系统实现
#[derive(Debug, Clone, Default)]
pub struct FsLogStorage;

#[async_trait]
impl LogStorageBackend for FsLogStorage {
    async fn open_archive_stream(&self, locator: &str) -> Result<BoxByteTryStream, io::Error> {
        let file = tokio::fs::File::open(locator).await?;
        let reader_stream: ReaderStream<tokio::fs::File> = ReaderStream::new(file);
        Ok(Box::pin(reader_stream))
    }
}

/// MinIO 实现
#[derive(Clone)]
pub struct MinioLogStorage {
    client: Arc<MinioClient>,
    bucket: String,
}

impl MinioLogStorage {
    pub fn new(client: MinioClient, bucket: impl Into<String>) -> Self {
        Self { client: Arc::new(client), bucket: bucket.into() }
    }
}

#[async_trait]
impl LogStorageBackend for MinioLogStorage {
    async fn open_archive_stream(&self, locator: &str) -> Result<BoxByteTryStream, io::Error> {
        // locator 作为对象 key 使用
        let object = self
            .client
            .get_object(&self.bucket, locator)
            .send()
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("minio get_object error: {e}")))?;

        let (stream, _size) = object
            .content
            .to_stream()
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("minio to_stream error: {e}")))?;

        // 将后端错误统一映射为 io::Error，便于上层使用 StreamReader
        let io_stream = stream.map(|res| res.map_err(|e| io::Error::new(io::ErrorKind::Other, e)));
        Ok(Box::pin(io_stream))
    }
}

/// 一个简单的泛型管理器，持有任意实现了 `LogStorageBackend` 的后端
pub struct LogStorage<B: LogStorageBackend> {
    backend: B,
}

impl<B: LogStorageBackend> LogStorage<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// 打开归档（例如 .tar.gz）对应的字节 TryStream
    pub async fn open_archive_stream(&self, locator: &str) -> Result<BoxByteTryStream, io::Error> {
        self.backend.open_archive_stream(locator).await
    }
}