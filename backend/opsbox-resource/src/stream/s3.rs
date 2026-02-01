//! S3 条目流
//!
//! 用于 S3 存储桶的文件流式遍历。

use std::collections::VecDeque;
use std::io;

use async_trait::async_trait;
use aws_sdk_s3::Client;
use tokio::io::{AsyncRead, AsyncReadExt, BufReader};

use super::utils::{sniff_archive_kind, ArchiveKind};
use super::{EntryMeta, EntryStream, EntrySource};

/// S3 条目流
pub struct S3EntryStream {
    client: Client,
    bucket: String,
    keys: VecDeque<String>,
}

impl S3EntryStream {
    pub fn new(client: Client, bucket: String, keys: Vec<String>) -> Self {
        Self {
            client,
            bucket,
            keys: keys.into(),
        }
    }
}

#[async_trait]
impl EntryStream for S3EntryStream {
    async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
        if let Some(key) = self.keys.pop_front() {
            let resp = self
                .client
                .get_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| io::Error::other(e.to_string()))?;

            let size = resp.content_length.map(|s| s as u64);
            let stream = resp.body.into_async_read();

            // 预读取头部进行类型探测
            let mut buf_reader = BufReader::new(stream);
            let mut head = vec![0u8; 1024];
            let mut n = 0;
            while n < head.len() {
                let read_n = buf_reader
                    .read(&mut head[n..])
                    .await
                    .map_err(|e| io::Error::other(e.to_string()))?;
                if read_n == 0 {
                    break;
                }
                n += read_n;
            }
            head.truncate(n);

            let kind = sniff_archive_kind(&head, Some(&key));
            let is_compressed = matches!(kind, ArchiveKind::Gzip);

            // 重构流
            let prefixed = crate::stream::utils::PrefixedReader::new(head, buf_reader);

            let reader: Box<dyn AsyncRead + Send + Unpin> = if is_compressed {
                use async_compression::tokio::bufread::GzipDecoder;
                let gz = GzipDecoder::new(BufReader::new(prefixed));
                Box::new(gz)
            } else {
                Box::new(prefixed)
            };

            let meta = EntryMeta {
                path: key.clone(),
                container_path: None,
                size,
                is_compressed,
                source: EntrySource::File,
            };

            Ok(Some((meta, reader)))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_s3_entry_stream_new() {
        let config = aws_sdk_s3::Config::builder()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .build();
        let client = Client::from_conf(config);
        let stream = S3EntryStream::new(
            client,
            "my-bucket".to_string(),
            vec!["k1".to_string(), "k2".to_string()],
        );

        assert_eq!(stream.bucket, "my-bucket");
        assert_eq!(stream.keys.len(), 2);
    }
}
