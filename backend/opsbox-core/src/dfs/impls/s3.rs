//! S3Storage 模块 - S3 对象存储实现
//!
//! 使用 AWS SDK for Rust 实现 S3 对象存储访问

use async_trait::async_trait;
use std::pin::Pin;
use aws_sdk_s3::{
    config::{Region, Credentials, SharedCredentialsProvider},
    types::Object,
    Client as S3Client,
};

use super::super::{
    filesystem::{DirEntry, FileMetadata, FsError, OpbxFileSystem},
    path::ResourcePath,
};
use crate::fs::{EntryMeta, EntrySource, EntryStream};

/// S3 配置
#[derive(Debug, Clone)]
pub struct S3Config {
    /// Profile 名称
    pub profile_name: String,
    /// S3 Endpoint
    pub endpoint: String,
    /// Access Key
    pub access_key: String,
    /// Secret Key
    pub secret_key: String,
    /// 默认 Bucket（可选）
    pub bucket: Option<String>,
    /// Region（可选）
    pub region: Option<String>,
}

impl S3Config {
    /// 创建新的 S3 配置
    pub fn new(
        profile_name: String,
        endpoint: String,
        access_key: String,
        secret_key: String,
    ) -> Self {
        Self {
            profile_name,
            endpoint,
            access_key,
            secret_key,
            bucket: None,
            region: None,
        }
    }

    /// 设置默认 bucket
    pub fn with_bucket(mut self, bucket: String) -> Self {
        self.bucket = Some(bucket);
        self
    }

    /// 设置 region
    pub fn with_region(mut self, region: String) -> Self {
        self.region = Some(region);
        self
    }
}

/// S3 对象存储
#[derive(Debug, Clone)]
pub struct S3Storage {
    client: S3Client,
    default_bucket: Option<String>,
}

impl S3Storage {
    /// 创建新的 S3 存储（同步版本）
    pub fn new(config: S3Config) -> Result<Self, FsError> {
        // 在单独的线程中创建运行时并执行异步初始化
        // 这样可以避免在异步运行时中嵌套创建运行时的问题
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| FsError::InvalidConfig(format!("Failed to create runtime: {}", e)))?;

            rt.block_on(async {
                Self::new_async(config).await
            })
        }).join().map_err(|_| FsError::InvalidConfig("Thread join failed".to_string()))?
    }

    /// 异步创建 S3 存储
    pub async fn new_async(config: S3Config) -> Result<Self, FsError> {
        // 解析 region
        let region = if let Some(region_str) = config.region {
            Some(Region::new(region_str))
        } else {
            Self::infer_region_from_endpoint(&config.endpoint)
        };

        // 配置 credentials
        let creds = Credentials::new(
            config.access_key,
            config.secret_key,
            None,
            None,
            "opsbox-s3",
        );
        let creds_provider = SharedCredentialsProvider::new(creds);

        // 创建 S3 配置
        let s3_config_builder = aws_sdk_s3::config::Config::builder()
            .region(region.unwrap_or(Region::new("us-east-1")))
            .credentials_provider(creds_provider);

        // 设置 endpoint（如果需要）
        let s3_config = if !config.endpoint.is_empty() {
            s3_config_builder.endpoint_url(config.endpoint).build()
        } else {
            s3_config_builder.build()
        };

        // 创建 S3 客户端
        let client = S3Client::from_conf(s3_config);

        Ok(Self {
            client,
            default_bucket: config.bucket,
        })
    }

    /// 从 endpoint 推断 region
    fn infer_region_from_endpoint(endpoint: &str) -> Option<Region> {
        if endpoint.contains("s3.amazonaws.com") {
            Some(Region::new("us-east-1"))
        } else if endpoint.contains("s3.cn-north-1.amazonaws.com") {
            Some(Region::new("cn-north-1"))
        } else {
            None
        }
    }

    /// 解析 bucket 和 key
    fn parse_bucket_and_key(&self, path: &ResourcePath) -> Result<(String, String), FsError> {
        let segments = path.segments();

        // 如果有默认 bucket，从路径中跳过 bucket 名称（如果存在）
        if let Some(ref default_bucket) = self.default_bucket {
            // 检查第一段是否是 bucket 名称
            let key = if !segments.is_empty() && segments[0] == *default_bucket {
                // 第一段是 bucket，跳过它
                if segments.len() > 1 {
                    segments[1..].join("/")
                } else {
                    String::new()
                }
            } else {
                // 第一段不是 bucket，直接使用
                segments.join("/")
            };
            return Ok((default_bucket.clone(), key));
        }

        // 否则，第一段是 bucket
        if segments.is_empty() {
            return Err(FsError::InvalidConfig("S3 path requires bucket".to_string()));
        }

        let bucket = segments[0].clone();
        let key = if segments.len() > 1 {
            segments[1..].join("/")
        } else {
            String::new()
        };

        Ok((bucket, key))
    }

    /// 将 S3 对象转换为 FileMetadata
    fn object_to_metadata(obj: &Object) -> FileMetadata {
        let size = obj.size().unwrap_or(0) as u64;
        let last_modified = obj.last_modified().and_then(|dt| {
            // 转换 DateTime 为 SystemTime
            // AWS SDK DateTime 可以通过 as_secs_f64 转换
            std::time::SystemTime::UNIX_EPOCH
                .checked_add(std::time::Duration::from_secs_f64(dt.as_secs_f64()))
        });

        // S3 中没有目录的概念，只能通过 key 是否以 '/' 结尾来判断
        let key = obj.key().unwrap_or("");
        let is_dir = key.ends_with('/');
        let is_file = !is_dir;

        FileMetadata {
            is_dir,
            is_file,
            size,
            modified: last_modified,
            created: None, // S3 不提供创建时间
        }
    }
}

#[async_trait]
impl OpbxFileSystem for S3Storage {
    /// 获取对象元数据
    async fn metadata(&self, path: &ResourcePath) -> Result<FileMetadata, FsError> {
        let (bucket, _key) = self.parse_bucket_and_key(path)?;

        // 如果 key 是空的或以 / 结尾，尝试列出对象
        let key_str = if path.segments().is_empty() {
            String::new()
        } else {
            path.to_string().trim_start_matches('/').to_string()
        };

        if key_str.is_empty() || key_str.ends_with('/') {
            // 这是一个"目录"，列出其中的对象
            let output = self
                .client
                .list_objects_v2()
                .bucket(&bucket)
                .prefix(&key_str)
                .max_keys(1)
                .send()
                .await
                .map_err(|e| FsError::S3(format!("ListObjects failed: {}", e)))?;

            // 如果有任何对象存在，认为目录存在
            if output.contents.is_some() || output.common_prefixes.is_some() {
                Ok(FileMetadata::dir(0))
            } else {
                Err(FsError::NotFound(format!("S3 prefix not found: {}", key_str)))
            }
        } else {
            // 这是一个文件，使用 head object
            let output = self
                .client
                .head_object()
                .bucket(&bucket)
                .key(&key_str)
                .send()
                .await
                .map_err(|e| {
                    // 简化错误处理：如果是 404 错误，返回 NotFound
                    let err_str = e.to_string();
                    if err_str.contains("404") || err_str.contains("NotFound") || err_str.contains("NoSuchKey") {
                        FsError::NotFound(format!("S3 object not found: {}", key_str))
                    } else {
                        FsError::S3(format!("HeadObject failed: {}", e))
                    }
                })?;

            Ok(FileMetadata {
                is_dir: false,
                is_file: true,
                size: output.content_length().unwrap_or(0) as u64,
                modified: output.last_modified().and_then(|dt| {
                    std::time::SystemTime::UNIX_EPOCH
                        .checked_add(std::time::Duration::from_secs_f64(dt.as_secs_f64()))
                }),
                created: None,
            })
        }
    }

    /// 读取目录内容
    async fn read_dir(&self, path: &ResourcePath) -> Result<Vec<DirEntry>, FsError> {
        let (bucket, key) = self.parse_bucket_and_key(path)?;

        // 计算 prefix
        // 如果有默认 bucket，path 是相对于 bucket 的
        // 如果没有默认 bucket，path 的第一段是 bucket，剩余部分是 key
        let prefix = if self.default_bucket.is_some() {
            // 有默认 bucket：path 是相对路径
            if key.is_empty() {
                String::new()
            } else if key.ends_with('/') {
                key.to_string()
            } else {
                format!("{}/", key)
            }
        } else {
            // 没有默认 bucket：path 包含 bucket
            let segments = path.segments();
            if segments.len() <= 1 {
                String::new()
            } else {
                segments[1..].join("/")
            }
        };

        // 如果 prefix 不为空且不以 / 结尾，添加 /
        let prefix = if !prefix.is_empty() && !prefix.ends_with('/') {
            format!("{}/", prefix)
        } else {
            prefix
        };

        let output = self
            .client
            .list_objects_v2()
            .bucket(&bucket)
            .prefix(&prefix)
            .delimiter("/") // 使用 / 分隔符模拟目录
            .send()
            .await
            .map_err(|e| FsError::S3(format!("ListObjects failed: {}", e)))?;

        let mut entries = Vec::new();

        // 处理对象（文件）
        if let Some(objects) = output.contents {
            for obj in objects {
                let key = obj.key().unwrap_or("");
                // 跳过目录标记本身
                if key == &prefix || key.ends_with('/') {
                    continue;
                }

                // 提取名称（相对于 prefix 的名称）
                let name = key
                    .trim_start_matches(&prefix)
                    .trim_start_matches('/')
                    .to_string();

                if name.is_empty() {
                    continue;
                }

                // 创建路径（包含 bucket，确保 create_fs_for_resource 能正确提取 bucket）
                let entry_path = ResourcePath::from_str(&format!("/{}/{}", bucket, key));

                entries.push(DirEntry {
                    name,
                    path: entry_path,
                    metadata: Self::object_to_metadata(&obj),
                });
            }
        }

        // 处理通用前缀（子目录）
        if let Some(prefixes) = output.common_prefixes {
            for cp in prefixes {
                let prefix_str = cp.prefix().unwrap_or("");
                // 提取目录名称（相对于 prefix 的名称）
                let name = prefix_str
                    .trim_start_matches(&prefix)
                    .trim_start_matches('/')
                    .trim_end_matches('/')
                    .to_string();

                if name.is_empty() {
                    continue;
                }

                // 创建路径（包含 bucket，去除末尾斜杠）
                let entry_path = ResourcePath::from_str(&format!("/{}/{}", bucket, prefix_str.trim_end_matches('/')));

                entries.push(DirEntry {
                    name,
                    path: entry_path,
                    metadata: FileMetadata::dir(0),
                });
            }
        }

        Ok(entries)
    }

    /// 打开对象用于读取
    async fn open_read(
        &self,
        path: &ResourcePath,
    ) -> Result<Pin<Box<dyn tokio::io::AsyncRead + Send + Unpin>>, FsError> {
        let (bucket, key) = self.parse_bucket_and_key(path)?;

        let output = self
            .client
            .get_object()
            .bucket(&bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| FsError::S3(format!("GetObject failed: {}", e)))?;

        // 使用流式读取：边下载边处理
        let stream = output.body.into_async_read();
        Ok(Box::pin(stream))
    }

    /// 获取条目流（用于批量处理/搜索）
    ///
    /// 对于 S3：
    /// - 单文件：返回单文件流
    /// - 目录：列出所有对象并逐个返回
    async fn as_entry_stream(&self, path: &ResourcePath, recursive: bool)
        -> Result<Box<dyn EntryStream>, FsError>
    {
        let (bucket, key) = self.parse_bucket_and_key(path)?;

        // 检查是否是文件（有 key 且不是目录）
        if !key.is_empty() && !key.ends_with('/') {
            // 单文件：检查是否存在
            let meta = self.metadata(path).await?;
            if meta.is_file {
                return Ok(Box::new(S3SingleFileEntryStream::new(
                    self.client.clone(),
                    bucket,
                    key,
                )));
            }
        }

        // 目录：创建 S3 目录遍历流
        Ok(Box::new(S3DirectoryEntryStream::new(
            self.client.clone(),
            bucket,
            key,
            recursive,
        )))
    }
}

/// S3 单文件条目流
pub struct S3SingleFileEntryStream {
    client: S3Client,
    bucket: String,
    key: String,
    consumed: bool,
}

impl S3SingleFileEntryStream {
    fn new(client: S3Client, bucket: String, key: String) -> Self {
        Self {
            client,
            bucket,
            key,
            consumed: false,
        }
    }
}

#[async_trait]
impl EntryStream for S3SingleFileEntryStream {
    async fn next_entry(&mut self) -> std::io::Result<Option<(EntryMeta, Box<dyn tokio::io::AsyncRead + Send + Unpin>)>> {
        if self.consumed {
            return Ok(None);
        }
        self.consumed = true;

        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&self.key)
            .send()
            .await
            .map_err(|e| std::io::Error::other(format!("GetObject failed: {}", e)))?;

        let size = output.content_length().unwrap_or(0) as u64;
        let reader: Box<dyn tokio::io::AsyncRead + Send + Unpin> = Box::new(output.body.into_async_read());

        let meta = EntryMeta {
            path: self.key.clone(),
            container_path: None,
            size: Some(size),
            is_compressed: false,
            source: EntrySource::File,
        };

        Ok(Some((meta, reader)))
    }
}

/// S3 目录遍历条目流
pub struct S3DirectoryEntryStream {
    client: S3Client,
    bucket: String,
    prefix: String,
    recursive: bool,
    continuation_token: Option<String>,
    buffer: Vec<(String, u64)>,  // (key, size) 缓冲
    buffer_idx: usize,
    exhausted: bool,
}

impl S3DirectoryEntryStream {
    fn new(client: S3Client, bucket: String, prefix: String, recursive: bool) -> Self {
        let prefix = if prefix.is_empty() || prefix.ends_with('/') {
            prefix
        } else {
            format!("{}/", prefix)
        };
        Self {
            client,
            bucket,
            prefix,
            recursive,
            continuation_token: None,
            buffer: Vec::new(),
            buffer_idx: 0,
            exhausted: false,
        }
    }

    async fn fetch_next_batch(&mut self) -> std::io::Result<bool> {
        if self.exhausted {
            return Ok(false);
        }

        let mut request = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&self.prefix);

        if !self.recursive {
            request = request.delimiter("/");
        }

        if let Some(token) = &self.continuation_token {
            request = request.continuation_token(token);
        }

        let output = request
            .send()
            .await
            .map_err(|e| std::io::Error::other(format!("ListObjectsV2 failed: {}", e)))?;

        // 更新 continuation token
        self.continuation_token = output.next_continuation_token().map(|s| s.to_string());
        self.exhausted = self.continuation_token.is_none();

        // 收集对象
        if let Some(objects) = output.contents {
            for obj in objects {
                if let Some(key) = obj.key() {
                    // 跳过目录标记
                    if key.ends_with('/') || key == &self.prefix {
                        continue;
                    }
                    let size = obj.size().unwrap_or(0) as u64;
                    self.buffer.push((key.to_string(), size));
                }
            }
        }

        Ok(!self.buffer.is_empty())
    }
}

#[async_trait]
impl EntryStream for S3DirectoryEntryStream {
    async fn next_entry(&mut self) -> std::io::Result<Option<(EntryMeta, Box<dyn tokio::io::AsyncRead + Send + Unpin>)>> {
        loop {
            // 如果缓冲区还有数据
            if self.buffer_idx < self.buffer.len() {
                let (key, size) = self.buffer[self.buffer_idx].clone();
                self.buffer_idx += 1;

                // 下载对象
                let output = self
                    .client
                    .get_object()
                    .bucket(&self.bucket)
                    .key(&key)
                    .send()
                    .await
                    .map_err(|e| {
                        tracing::warn!("Failed to get S3 object {}: {}", key, e);
                        std::io::Error::other(format!("GetObject failed: {}", e))
                    })?;

                let reader: Box<dyn tokio::io::AsyncRead + Send + Unpin> = Box::new(output.body.into_async_read());

                let meta = EntryMeta {
                    path: key,
                    container_path: None,
                    size: Some(size),
                    is_compressed: false,
                    source: EntrySource::File,
                };

                return Ok(Some((meta, reader)));
            }

            // 缓冲区空了，尝试获取下一批
            if !self.fetch_next_batch().await? {
                return Ok(None);
            }
            self.buffer_idx = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dfs::filesystem::MemoryReader;

    #[test]
    fn test_s3_config_new() {
        let config = S3Config::new(
            "test".to_string(),
            "https://s3.amazonaws.com".to_string(),
            "key".to_string(),
            "secret".to_string(),
        );
        assert_eq!(config.profile_name, "test");
        assert!(config.bucket.is_none());
        assert!(config.region.is_none());
    }

    #[test]
    fn test_s3_config_with_bucket() {
        let config = S3Config::new(
            "test".to_string(),
            "https://s3.amazonaws.com".to_string(),
            "key".to_string(),
            "secret".to_string(),
        )
        .with_bucket("my-bucket".to_string());

        assert_eq!(config.bucket, Some("my-bucket".to_string()));
    }

    #[test]
    fn test_s3_config_with_region() {
        let config = S3Config::new(
            "test".to_string(),
            "https://s3.amazonaws.com".to_string(),
            "key".to_string(),
            "secret".to_string(),
        )
        .with_region("eu-west-1".to_string());

        assert_eq!(config.region, Some("eu-west-1".to_string()));
    }

    #[test]
    fn test_infer_region_from_endpoint() {
        // 测试 region 推断
        assert!(S3Storage::infer_region_from_endpoint("https://s3.amazonaws.com").is_some());
        assert!(S3Storage::infer_region_from_endpoint("https://s3.cn-north-1.amazonaws.com").is_some());
        assert!(S3Storage::infer_region_from_endpoint("https://other.com").is_none());
    }

    #[test]
    fn test_s3_file_reader() {
        let reader = MemoryReader::new(vec![1, 2, 3, 4, 5]);
        assert_eq!(reader.as_bytes().len(), 5);
        assert!(!reader.as_bytes().is_empty());
        assert_eq!(reader.as_bytes(), &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_s3_file_reader_empty() {
        let reader = MemoryReader::new(vec![]);
        assert_eq!(reader.as_bytes().len(), 0);
        assert!(reader.as_bytes().is_empty());
    }

    #[test]
    fn test_object_to_metadata() {
        // 测试基本文件元数据
        let file_metadata = FileMetadata::file(1024);
        assert!(file_metadata.is_file);
        assert!(!file_metadata.is_dir);
        assert_eq!(file_metadata.size, 1024);
    }
}
