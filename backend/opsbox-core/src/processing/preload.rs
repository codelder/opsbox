//! Preload utilities - 预读工具
//!
//! 提供文件条目预读到内存的功能，用于优化小文件处理。

use std::io;

use tokio::io::{AsyncRead, AsyncReadExt};

/// 预读结果：小文件完整内容，或大文件的已读取部分
pub enum PreloadResult {
  /// 小文件：完整内容已读取
  Complete(Vec<u8>),
  /// 大文件：已读取部分内容（reader 已被部分消费）
  Partial(Vec<u8>),
}

/// 预读缓冲区默认大小（64KB）
pub const DEFAULT_PRELOAD_BUFFER_SIZE: usize = 64 * 1024;

/// 预读文件条目到内存
///
/// 返回：
/// - Complete(content): 文件完全读取（小文件）
/// - Partial(content): 文件太大，只读取了部分（reader 已被部分消费）
pub async fn preload_entry(reader: &mut (dyn AsyncRead + Send + Unpin), max_size: usize) -> io::Result<PreloadResult> {
  let mut buffer = Vec::with_capacity(DEFAULT_PRELOAD_BUFFER_SIZE);
  let mut temp = vec![0u8; DEFAULT_PRELOAD_BUFFER_SIZE];

  loop {
    let n = reader.read(&mut temp).await?;
    if n == 0 {
      // EOF，文件完全读取
      return Ok(PreloadResult::Complete(buffer));
    }
    buffer.extend_from_slice(&temp[..n]);

    // 如果超过最大大小，返回已读取的部分
    if buffer.len() > max_size {
      return Ok(PreloadResult::Partial(buffer));
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_preload_entry_small() {
    let content = b"hello world";
    let mut reader = &content[..];
    // max size larger than content
    let res = preload_entry(&mut reader, 100).await.expect("preload failed");
    match res {
      PreloadResult::Complete(c) => assert_eq!(c, content),
      PreloadResult::Partial(_) => panic!("should be complete"),
    }
  }

  #[tokio::test]
  async fn test_preload_entry_large() {
    // Create content slightly larger than our max check
    let content = [0u8; 100];
    let mut reader = &content[..];
    // max size smaller than content
    let res = preload_entry(&mut reader, 50).await.expect("preload failed");
    match res {
      PreloadResult::Partial(c) => {
        // It reads in chunks of 64KB. So the first read will read all 100 bytes.
        // Then buffer.len() is 100. 100 > 50. Returns Partial(100 bytes).
        assert_eq!(c.len(), 100);
      }
      PreloadResult::Complete(_) => panic!("should be partial"),
    }
  }

  #[tokio::test]
  async fn test_preload_entry_empty() {
    let content: [u8; 0] = [];
    let mut reader = &content[..];
    let res = preload_entry(&mut reader, 100).await.expect("preload failed");
    match res {
      PreloadResult::Complete(c) => {
        assert!(c.is_empty());
      }
      PreloadResult::Partial(_) => panic!("empty file should be complete"),
    }
  }
}
