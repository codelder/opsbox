//! 流处理工具函数
//!
//! 文件类型检测、压缩处理等。

use std::io;
use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncReadExt, BufReader};

use async_compression::tokio::bufread::GzipDecoder;

use super::{EntryMeta, EntrySource};

/// 归档类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Tar,
    Gzip,
    Zip,
    Unknown,
}

/// PrefixedReader - 组合已读取的前缀和剩余流
pub struct PrefixedReader<R> {
    prefix: std::io::Cursor<Vec<u8>>,
    inner: R,
}

impl<R> PrefixedReader<R> {
    pub fn new(prefix: Vec<u8>, inner: R) -> Self {
        Self {
            prefix: std::io::Cursor::new(prefix),
            inner,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for PrefixedReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        if self.prefix.position() < self.prefix.get_ref().len() as u64 {
            let mut tmp = vec![0u8; buf.remaining()];
            let read = std::io::Read::read(&mut self.prefix, &mut tmp).unwrap_or(0);
            if read > 0 {
                buf.put_slice(&tmp[..read]);
                return std::task::Poll::Ready(Ok(()));
            }
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

/// 检测归档类型
pub fn sniff_archive_kind(head: &[u8], _path_hint: Option<&str>) -> ArchiveKind {
    if head.len() >= 4 {
        let sig = &head[..4];
        if sig == [0x50, 0x4B, 0x03, 0x04] || sig == [0x50, 0x4B, 0x05, 0x06] || sig == [0x50, 0x4B, 0x07, 0x08] {
            return ArchiveKind::Zip;
        }
    }
    // 优先检查 tar（tar 头在固定位置 257-262，更可靠）
    if head.len() >= 512 && &head[257..257 + 5] == b"ustar" {
        return ArchiveKind::Tar;
    }
    // 然后检查 gzip（前2字节）
    if head.len() >= 2 && head[0] == 0x1F && head[1] == 0x8B {
        return ArchiveKind::Gzip;
    }
    ArchiveKind::Unknown
}

/// 检查是否为 tar 头
fn is_tar_header(head: &[u8]) -> bool {
    head.len() >= 512 && &head[257..257 + 5] == b"ustar"
}

/// 检测文件类型并返回适当的 Reader 和 Metadata
///
/// 注意：不使用 'static 生命周期以匹配 opsbox_core::fs::EntryStream 的签名。
pub async fn open_file_with_compression_detection(
    path: &str,
) -> io::Result<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)> {
    // 1. 打开文件并读取头部
    let mut file = tokio::fs::File::open(path).await?;
    let mut head = vec![0u8; 560];
    let n = file.read(&mut head).await?;
    head.truncate(n);

    // 2. 检测文件类型
    let kind = sniff_archive_kind(&head, Some(path));

    // 3. 处理 gzip
    match kind {
        ArchiveKind::Gzip => {
            // 重新打开文件进行 tar 头部嗅探
            let is_tar = match tokio::fs::File::open(path).await {
                Ok(f) => {
                    let mut gz = GzipDecoder::new(BufReader::new(f));
                    let mut inner_head = vec![0u8; 512];
                    match gz.read_exact(&mut inner_head).await {
                        Ok(_) => is_tar_header(&inner_head),
                        _ => false,
                    }
                }
                _ => false,
            };

            if is_tar {
                // tar.gz 文件：按普通文件处理
                create_regular_file_reader(path).await
            } else {
                // 纯 gzip 文件：解压
                let file = tokio::fs::File::open(path).await?;
                let gz = GzipDecoder::new(BufReader::new(file));
                let meta = EntryMeta {
                    path: path.to_string(),
                    container_path: None,
                    size: None,
                    is_compressed: true,
                    source: EntrySource::Gz,
                };
                Ok((meta, Box::new(gz)))
            }
        }
        _ => {
            // 默认：按普通文件打开
            create_regular_file_reader(path).await
        }
    }
}

/// 创建普通文件读取器
async fn create_regular_file_reader(
    path: &str,
) -> io::Result<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)> {
    let file = tokio::fs::File::open(path).await?;
    let reader = BufReader::new(file);
    let meta = EntryMeta {
        path: path.to_string(),
        container_path: None,
        size: None,
        is_compressed: false,
        source: EntrySource::File,
    };
    Ok((meta, Box::new(reader)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sniff_archive_kind_tar() {
        // tar header with "ustar" at position 257
        let mut head = vec![0u8; 512];
        head[257..262].copy_from_slice(b"ustar");
        assert_eq!(sniff_archive_kind(&head, None), ArchiveKind::Tar);
    }

    #[test]
    fn test_sniff_archive_kind_gzip() {
        let head = vec![0x1f, 0x8b, 0x08];
        assert_eq!(sniff_archive_kind(&head, None), ArchiveKind::Gzip);
    }

    #[test]
    fn test_sniff_archive_kind_zip() {
        let head = vec![0x50, 0x4B, 0x03, 0x04];
        assert_eq!(sniff_archive_kind(&head, None), ArchiveKind::Zip);
    }

    #[test]
    fn test_sniff_archive_kind_unknown() {
        let head = vec![0x00, 0x00, 0x00, 0x00];
        assert_eq!(sniff_archive_kind(&head, None), ArchiveKind::Unknown);
    }

    #[tokio::test]
    async fn test_open_regular_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        tokio::fs::write(&file_path, b"hello world").await.unwrap();

        let (meta, _reader) = open_file_with_compression_detection(
            file_path.to_str().unwrap()
        ).await.unwrap();

        assert_eq!(meta.path, file_path.to_str().unwrap());
        assert!(!meta.is_compressed);
        assert!(matches!(meta.source, EntrySource::File));
    }

    #[tokio::test]
    async fn test_prefixed_reader() {
        // 测试 PrefixedReader 能正确组合前缀数据和内部流
        let prefix = vec![1, 2, 3];
        let inner = std::io::Cursor::new(vec![4, 5, 6]);
        let mut reader = PrefixedReader::new(prefix, inner);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, vec![1, 2, 3, 4, 5, 6]);
    }

    #[tokio::test]
    async fn test_prefixed_reader_empty_prefix() {
        // 测试空前缀的情况
        let prefix = vec![];
        let inner = std::io::Cursor::new(vec![1, 2, 3]);
        let mut reader = PrefixedReader::new(prefix, inner);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_prefixed_reader_only_prefix() {
        // 测试只有前缀没有内部数据的情况
        let prefix = vec![1, 2, 3];
        let inner = std::io::Cursor::new(vec![]);
        let mut reader = PrefixedReader::new(prefix, inner);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, vec![1, 2, 3]);
    }
}
