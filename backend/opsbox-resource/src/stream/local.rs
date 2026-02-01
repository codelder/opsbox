//! 本地文件系统条目流
//!
//! 基于 jwalk 并行遍历的文件流。

use std::io;
use std::path::PathBuf;

use async_trait::async_trait;
use tokio::io::AsyncRead;

use super::utils::open_file_with_compression_detection;
use super::{EntryMeta, EntryStream};

/// 目录条目流（基于 jwalk 并行遍历）
pub struct FsEntryStream {
    rx: tokio::sync::mpsc::Receiver<io::Result<(PathBuf, std::fs::Metadata)>>,
}

impl FsEntryStream {
    /// 从根目录创建并行遍历条目流
    pub async fn new(root: PathBuf, recursive: bool) -> io::Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::channel(256);

        // 判断 root 是否是文件
        if root.is_file() {
            // 如果根就是文件，直接发送并结束
            let _ = tx.send(Ok((root.clone(), root.metadata()?))).await;
            return Ok(Self { rx });
        }

        // 在 blocking thread 中运行 jwalk
        std::thread::spawn(move || {
            use jwalk::WalkDir;
            let walk = WalkDir::new(&root)
                .follow_links(false)
                .max_depth(if recursive { usize::MAX } else { 1 })
                .skip_hidden(false);

            for entry in walk {
                match entry {
                    Ok(e) => {
                        // 只处理文件
                        if e.file_type().is_file() {
                            if let Ok(meta) = e.metadata() {
                                if tx.blocking_send(Ok((e.path(), meta))).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let io_err = io::Error::other(e.to_string());
                        if tx.blocking_send(Err(io_err)).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Ok(Self { rx })
    }
}

#[async_trait]
impl EntryStream for FsEntryStream {
    async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
        loop {
            match self.rx.recv().await {
                Some(Ok((path, _meta))) => {
                    match open_file_with_compression_detection(&path.to_string_lossy()).await {
                        Ok((mut meta, reader)) => {
                            meta.path = path.to_string_lossy().to_string();
                            return Ok(Some((meta, reader)));
                        }
                        Err(e) => {
                            tracing::warn!("无法打开文件 {}: {}", path.display(), e);
                            continue;
                        }
                    }
                }
                Some(Err(e)) => {
                    tracing::warn!("jwalk 遍历错误: {}", e);
                    continue;
                }
                None => return Ok(None),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fs_entry_stream_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        tokio::fs::write(&file_path, b"hello world").await.unwrap();

        let mut stream = FsEntryStream::new(file_path.clone(), false).await.unwrap();
        let result = stream.next_entry().await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_fs_entry_stream_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        tokio::fs::write(temp_dir.path().join("file1.txt"), b"content1").await.unwrap();
        tokio::fs::write(temp_dir.path().join("file2.txt"), b"content2").await.unwrap();

        let mut stream = FsEntryStream::new(temp_dir.path().to_path_buf(), true).await.unwrap();
        let mut count = 0;
        while let Some(_) = stream.next_entry().await.unwrap() {
            count += 1;
        }
        assert_eq!(count, 2);
    }
}
