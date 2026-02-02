//! 归档条目流
//!
//! 支持 tar.gz 等归档格式的流式解析。

use std::io;

use async_compression::tokio::bufread::GzipDecoder;
use async_trait::async_trait;
use futures::{AsyncReadExt, StreamExt};
use tokio::io::{AsyncRead, BufReader};
use tokio_util::compat::TokioAsyncReadCompatExt;
use tracing::trace;

use super::{EntryMeta, EntryStream, EntrySource};
use super::utils::PrefixedReader;

/// tar.gz 条目流（基于 AsyncRead 输入）
#[allow(dead_code)]
pub struct TarGzEntryStream<R: AsyncRead + Send + Unpin + 'static> {
    entries: async_tar::Entries<tokio_util::compat::Compat<GzipDecoder<BufReader<R>>>>,
    container_path: Option<String>,
    consecutive_errors: usize,
    next_entry_index: usize,
    last_ok_entry_path: Option<String>,
}

impl<R: AsyncRead + Send + Unpin + 'static> TarGzEntryStream<R> {
    pub async fn new(reader: R, container_path: Option<String>) -> io::Result<Self> {
        let gz = GzipDecoder::new(BufReader::new(reader));
        let archive = async_tar::Archive::new(gz.compat());
        let entries = archive.entries()?;
        Ok(Self {
            entries,
            container_path,
            consecutive_errors: 0,
            next_entry_index: 0,
            last_ok_entry_path: None,
        })
    }
}

#[async_trait]
impl<R: AsyncRead + Send + Unpin + 'static> EntryStream for TarGzEntryStream<R> {
    async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
        loop {
            if self.consecutive_errors > 10 {
                return Err(io::Error::other("过多的连续 tar 错误"));
            }

            let entry = self.entries.next().await;
            match entry {
                Some(Ok(mut entry)) => {
                    self.consecutive_errors = 0;
                    self.next_entry_index += 1;

                    let entry_path = entry
                        .path()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| format!("entry_{}", self.next_entry_index));

                    let entry_size = entry.header().size().unwrap_or(0);
                    trace!("[TarGzStream] 条目 #{}: path={} size={}", self.next_entry_index, entry_path, entry_size);

                    self.last_ok_entry_path = Some(entry_path.clone());

                    let meta = EntryMeta {
                        path: entry_path.clone(),
                        container_path: self.container_path.clone(),
                        size: Some(entry_size),
                        is_compressed: false,
                        source: EntrySource::TarGz,
                    };

                    // 将 tar entry 读入内存
                    let mut buf = Vec::new();
                    entry.read_to_end(&mut buf).await?;

                    return Ok(Some((meta, Box::new(std::io::Cursor::new(buf)))));
                }
                Some(Err(e)) => {
                    self.consecutive_errors += 1;
                    trace!(
                        "[TarGzStream] 错误 #{}, last_ok={:?}, error={}",
                        self.consecutive_errors, self.last_ok_entry_path, e
                    );
                    continue;
                }
                None => return Ok(None),
            }
        }
    }
}

/// 通用的归档条目流
///
/// 内部使用 futures::AsyncRead (async_tar要求)
pub struct ArchiveEntryStream {
    inner: ArchiveEntryStreamInner,
}

enum ArchiveEntryStreamInner {
    /// tar.gz 格式 - 存储具体的entries类型
    TarGz {
        entries: async_tar::Entries<tokio_util::compat::Compat<GzipDecoder<BufReader<PrefixedReader<Box<dyn AsyncRead + Send + Unpin>>>>>>,
        consecutive_errors: usize,
        last_ok_entry_path: Option<String>,
    },
    /// 普通 tar 格式（无压缩）- 存储预读取的条目列表
    Tar {
        entries: Vec<(String, Vec<u8>)>,
        index: usize,
        container_path: Option<String>,
    },
    /// 纯 gzip (单个文件)
    Gz(Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>),
}

impl ArchiveEntryStream {
    /// 从已知类型创建 tar.gz 条目流
    pub async fn new_tar_gz(
        decoder: GzipDecoder<BufReader<PrefixedReader<Box<dyn AsyncRead + Send + Unpin>>>>,
        _container_path: Option<String>,
    ) -> io::Result<Self> {
        // 转换为 futures::AsyncRead
        let compat = decoder.compat();
        let archive = async_tar::Archive::new(compat);
        let entries = archive.entries()?;

        Ok(Self {
            inner: ArchiveEntryStreamInner::TarGz {
                entries,
                consecutive_errors: 0,
                last_ok_entry_path: None,
            },
        })
    }

    /// 创建普通 tar 条目流（无压缩）
    ///
    /// 注意：这个方法会预读取所有条目到内存，适合小到中型的 tar 文件
    pub async fn new_tar<R: AsyncRead + Send + Unpin + 'static>(
        reader: R,
        container_path: Option<String>,
    ) -> io::Result<Self> {
        use tokio_util::compat::TokioAsyncReadCompatExt;

        let compat = reader.compat();
        let archive = async_tar::Archive::new(compat);
        let mut entries = archive.entries()?;

        use futures::StreamExt;

        // 预读取所有条目到内存
        let mut entries_vec = Vec::new();

        while let Some(result) = entries.next().await {
            match result {
                Ok(mut entry) => {
                    let entry_path = entry
                        .path()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| format!("entry_{}", entries_vec.len()));

                    let entry_size = entry.header().size().unwrap_or(0);
                    trace!("[TarStream] 条目: path={} size={}", entry_path, entry_size);

                    // 将 tar entry 读入内存
                    let mut buf = Vec::new();
                    entry.read_to_end(&mut buf).await?;

                    entries_vec.push((entry_path, buf));
                }
                Err(e) => {
                    return Err(io::Error::other(format!("Tar条目错误: {}", e)));
                }
            }
        }

        // 创建内部存储
        let inner = ArchiveEntryStreamInner::Tar {
            entries: entries_vec,
            index: 0,
            container_path,
        };

        Ok(Self { inner })
    }

    /// 创建纯 gzip 单条目流
    pub fn new_gz(meta: EntryMeta, decoder: Box<dyn AsyncRead + Send + Unpin>) -> Self {
        Self {
            inner: ArchiveEntryStreamInner::Gz(Some((meta, decoder))),
        }
    }
}

#[async_trait]
impl EntryStream for ArchiveEntryStream {
    async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
        match &mut self.inner {
            ArchiveEntryStreamInner::TarGz { entries, consecutive_errors, last_ok_entry_path } => {
                loop {
                    if *consecutive_errors > 10 {
                        return Err(io::Error::other("过多的连续 tar 错误"));
                    }

                    let entry = entries.next().await;
                    match entry {
                        Some(Ok(mut entry)) => {
                            *consecutive_errors = 0;

                            let entry_path = entry
                                .path()
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_else(|_| "unknown".to_string());

                            let entry_size = entry.header().size().unwrap_or(0);

                            *last_ok_entry_path = Some(entry_path.clone());

                            let meta = EntryMeta {
                                path: entry_path.clone(),
                                container_path: None,
                                size: Some(entry_size),
                                is_compressed: false,
                                source: EntrySource::TarGz,
                            };

                            // 将 tar entry 读入内存
                            let mut buf = Vec::new();
                            entry.read_to_end(&mut buf).await?;

                            return Ok(Some((meta, Box::new(std::io::Cursor::new(buf)))));
                        }
                        Some(Err(e)) => {
                            *consecutive_errors += 1;
                            trace!(
                                "[TarGzStream] 错误 #{}, last_ok={:?}, error={}",
                                consecutive_errors, last_ok_entry_path, e
                            );
                            continue;
                        }
                        None => return Ok(None),
                    }
                }
            }
            ArchiveEntryStreamInner::Tar { entries, index, container_path } => {
                if *index >= entries.len() {
                    return Ok(None);
                }
                let (path, data) = entries[*index].clone();
                *index += 1;

                let meta = EntryMeta {
                    path,
                    container_path: container_path.clone(),
                    size: Some(data.len() as u64),
                    is_compressed: false,
                    source: EntrySource::TarGz,
                };

                Ok(Some((meta, Box::new(std::io::Cursor::new(data)))))
            }
            ArchiveEntryStreamInner::Gz(data) => {
                Ok(data.take())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::fs::File;

    #[tokio::test]
    async fn test_tar_gz_entry_stream() {
        // 创建一个临时的 tar.gz 文件用于测试
        let temp_dir = tempfile::tempdir().unwrap();
        let tar_path = temp_dir.path().join("test.tar.gz");

        // 创建 tar.gz 文件
        {
            let file = std::fs::File::create(&tar_path).unwrap();
            let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut ar = tar::Builder::new(enc);
            let mut header = tar::Header::new_gnu();
            header.set_path("test.txt").unwrap();
            header.set_size(11);
            header.set_cksum();
            ar.append_data(&mut header, "test.txt", b"hello world".as_slice()).unwrap();
            ar.finish().unwrap();
        }

        // 测试读取
        let file = File::open(&tar_path).await.unwrap();
        let mut stream = TarGzEntryStream::new(file, Some(tar_path.to_str().unwrap().to_string())).await.unwrap();

        let result = stream.next_entry().await.unwrap();
        assert!(result.is_some());

        let (meta, _reader) = result.unwrap();
        assert_eq!(meta.path, "test.txt");
        assert!(matches!(meta.source, EntrySource::TarGz));

        // 第二次调用应该返回 None
        let result = stream.next_entry().await.unwrap();
        assert!(result.is_none());
    }
}
