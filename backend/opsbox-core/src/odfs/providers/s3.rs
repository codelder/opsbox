use crate::fs::{PrefixedReader, sniff_file_type};
use crate::odfs::{OpsEntry, OpsFileSystem, OpsFileType, OpsMetadata, OpsPath, OpsRead};
use async_trait::async_trait;
use aws_sdk_s3::Client;
use std::io;
use tokio::io::AsyncReadExt;

/// S3 文件系统提供者
///
/// 将 ORL 路径映射到 S3 bucket
/// 支持虚拟目录结构
pub struct S3OpsFS {
  client: Client,
  bucket: String,
}

impl S3OpsFS {
  pub fn new(client: Client, bucket: impl Into<String>) -> Self {
    Self {
      client,
      bucket: bucket.into(),
    }
  }
}

#[async_trait]
impl OpsFileSystem for S3OpsFS {
  fn name(&self) -> &str {
    "S3OpsFS"
  }

  async fn metadata(&self, path: &OpsPath) -> io::Result<OpsMetadata> {
    let key = path.as_str().trim_start_matches('/');

    // 尝试作为文件获取 HeadObject
    let head_result = self.client.head_object().bucket(&self.bucket).key(key).send().await;

    match head_result {
      Ok(head) => {
        // 这是一个文件
        Ok(OpsMetadata {
          name: key.split('/').next_back().unwrap_or(key).to_string(),
          file_type: OpsFileType::File,
          size: head.content_length.unwrap_or(0) as u64,
          modified: head
            .last_modified
            .map(|t| std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(t.secs() as u64)),
          mode: 0,
          mime_type: head.content_type,
          compression: head.content_encoding, // S3 content-encoding header
          is_archive: false,                  // TODO: S3 metadata check or extension fallback
        })
      }
      Err(_) => {
        // 如果 HeadObject 失败，尝试检查是否为“目录”（前缀）
        // 通过 ListObjectsV2 limit=1 检查
        let prefix = if key.ends_with('/') {
          key.to_string()
        } else {
          format!("{}/", key)
        };

        let list_result = self
          .client
          .list_objects_v2()
          .bucket(&self.bucket)
          .prefix(&prefix)
          .max_keys(1)
          .send()
          .await
          .map_err(|e| io::Error::other(e.to_string()))?;

        if list_result.contents().is_empty() && list_result.common_prefixes().is_empty() {
          return Err(io::Error::new(io::ErrorKind::NotFound, "Path not found"));
        }

        Ok(OpsMetadata {
          name: key.split('/').next_back().unwrap_or(key).to_string(),
          file_type: OpsFileType::Directory,
          size: 0,
          modified: None,
          mode: 0,
          mime_type: None,
          compression: None,
          is_archive: false,
        })
      }
    }
  }

  async fn read_dir(&self, path: &OpsPath) -> io::Result<Vec<OpsEntry>> {
    let prefix = path.as_str().trim_start_matches('/');
    let prefix = if prefix.is_empty() {
      "".to_string()
    } else if prefix.ends_with('/') {
      prefix.to_string()
    } else {
      format!("{}/", prefix)
    };

    // 使用 S3 delimiter='/' 来模拟目录列表
    let response = self
      .client
      .list_objects_v2()
      .bucket(&self.bucket)
      .prefix(&prefix)
      .delimiter("/")
      .send()
      .await
      .map_err(|e| io::Error::other(e.to_string()))?;

    let mut entries = Vec::new();

    // 1. 处理子目录 (CommonPrefixes)
    if let Some(prefixes) = response.common_prefixes {
      for cp in prefixes {
        if let Some(p) = cp.prefix {
          let name = p.trim_end_matches('/').split('/').next_back().unwrap_or(&p).to_string();
          entries.push(OpsEntry {
            name: name.clone(),
            path: path.join(&name).as_str().to_string(),
            metadata: OpsMetadata {
              name,
              file_type: OpsFileType::Directory,
              size: 0,
              modified: None,
              mode: 0,
              mime_type: None,
              compression: None,
              is_archive: false,
            },
          });
        }
      }
    }

    // 2. 处理文件 (Contents)
    // 注意：ListObjects 也会返回前缀本身（如果前缀是作为一个空对象存在的），需要过滤掉
    if let Some(contents) = response.contents {
      for obj in contents {
        if let Some(key) = obj.key {
          if key == prefix {
            continue;
          } // 跳过目录标记对象本身

          let name = key.split('/').next_back().unwrap_or(&key).to_string();
          let size = obj.size.unwrap_or(0) as u64;
          let modified = obj
            .last_modified
            .map(|t| std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(t.secs() as u64));

          entries.push(OpsEntry {
            name: name.clone(),
            path: path.join(&name).as_str().to_string(),
            metadata: OpsMetadata {
              name,
              file_type: OpsFileType::File,
              size,
              modified,
              mode: 0,
              mime_type: None, // ListObjects 不返回 contentType，需要单独 Head（昂贵），这里先置空
              compression: None,
              is_archive: false, // TODO: extension check
            },
          });
        }
      }
    }

    Ok(entries)
  }

  async fn open_read(&self, path: &OpsPath) -> io::Result<OpsRead> {
    let key = path.as_str().trim_start_matches('/');

    let resp = self
      .client
      .get_object()
      .bucket(&self.bucket)
      .key(key)
      .send()
      .await
      .map_err(|e| io::Error::other(e.to_string()))?;

    // 转换为 AsyncRead 流
    let stream = resp.body.into_async_read();
    Ok(Box::pin(stream))
  }

  async fn as_entry_stream(&self, path: &OpsPath, recursive: bool) -> io::Result<Box<dyn crate::fs::EntryStream>> {
    let prefix = path.as_str().trim_start_matches('/').to_string();

    // 如果看起来像文件（不以 / 结尾），或者我们通过 HeadObject 确认它是文件
    // 这里为了性能，先假设：
    // 1. 如果 recursive=false 且不以 / 结尾 -> 单文件
    // 2. 否则 -> 目录遍历

    let is_dir_like = prefix.ends_with('/') || prefix.is_empty();

    if !is_dir_like && !recursive {
        // 单文件模式
        Ok(Box::new(S3EntryStream::new(self.client.clone(), self.bucket.clone(), vec![prefix])))
    } else {
        // 目录模式：先列出所有 Key（暂不通过 Stream Lazily List，因为 S3 List 分页处理较繁琐，
        // 这里先一次性 List 出所有 Keys，类似 FsEntryStream 的 jwalk）
        // 注意：生产环境如果 Bucket 巨大，应使用 Paginator

        let mut keys = Vec::new();
        let mut stream = self.client.list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&prefix)
            .into_paginator()
            .send();

        while let Some(res) = stream.next().await {
            let page = res.map_err(|e| io::Error::other(e.to_string()))?;
            for obj in page.contents.unwrap_or_default() {
                if let Some(k) = obj.key {
                    if k.ends_with('/') { continue; } // 跳过目录占位符
                    keys.push(k);
                }
            }
        }

        Ok(Box::new(S3EntryStream::new(self.client.clone(), self.bucket.clone(), keys)))
    }
  }
}

pub struct S3EntryStream {
    client: Client,
    bucket: String,
    keys: std::collections::VecDeque<String>,
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
impl crate::fs::EntryStream for S3EntryStream {
    async fn next_entry(&mut self) -> io::Result<Option<(crate::fs::EntryMeta, Box<dyn tokio::io::AsyncRead + Send + Unpin>)>> {
        if let Some(key) = self.keys.pop_front() {
            let resp = self.client.get_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| io::Error::other(e.to_string()))?;

            let size = resp.content_length.map(|s| s as u64);
            let stream = resp.body.into_async_read();

            // 预读取头部进行类型探测
            let mut buf_reader = tokio::io::BufReader::new(stream);
            let mut head = vec![0u8; 1024];
            let mut n = 0;
            // 尽力读取最多 1024 字节
            while n < head.len() {
                 let read_n = buf_reader.read(&mut head[n..]).await.map_err(|e| io::Error::other(e.to_string()))?;
                 if read_n == 0 { break; }
                 n += read_n;
            }
            head.truncate(n);

            let kind = sniff_file_type(&head);
            let is_compressed = kind.is_gzip();

            // 重构流（因为头部已被读取）
            let prefixed = PrefixedReader::new(head, buf_reader);

            let reader: Box<dyn tokio::io::AsyncRead + Send + Unpin> = if is_compressed {
                 // 使用 tokio 版本的 GzipDecoder，无需 compat (PrefixedReader 实现了 AsyncRead)
                 let gz = async_compression::tokio::bufread::GzipDecoder::new(tokio::io::BufReader::new(prefixed));
                 Box::new(gz)
            } else {
                 Box::new(prefixed)
            };

            let meta = crate::fs::EntryMeta {
                path: key.clone(),
                container_path: None,
                size,
                is_compressed: true,
                source: crate::fs::EntrySource::File,
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
    fn test_s3_ops_fs_new() {
        let config = aws_sdk_s3::Config::builder()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .build();
        let client = Client::from_conf(config);
        let fs = S3OpsFS::new(client, "my-bucket");
        assert_eq!(fs.bucket, "my-bucket");
        assert_eq!(fs.name(), "S3OpsFS");
    }

    #[test]
    fn test_s3_entry_stream_new() {
        let config = aws_sdk_s3::Config::builder()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .build();
        let client = Client::from_conf(config);
        let stream = S3EntryStream::new(client, "my-bucket".to_string(), vec!["k1".to_string(), "k2".to_string()]);

        assert_eq!(stream.bucket, "my-bucket");
        assert_eq!(stream.keys.len(), 2);
    }
}
