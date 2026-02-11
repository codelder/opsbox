//! Search types - 搜索相关类型定义
//!
//! 定义搜索事件、结果和内容处理器 trait。

use std::io;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt};

/// 内容处理器 trait
///
/// 定义如何处理文件内容的抽象接口。
/// LogSeek 的 SearchProcessor 实现此 trait。
#[async_trait]
pub trait ContentProcessor: Send + Sync {
    /// 处理文件内容并返回结果
    ///
    /// # Arguments
    /// * `path` - 文件路径
    /// * `reader` - 异步读取器
    ///
    /// # Returns
    /// * `Ok(Some(Vec<u8>))` - 有匹配结果，返回处理后的数据
    /// * `Ok(None)` - 无匹配结果
    /// * `Err` - 处理错误
    async fn process_content(
        &self,
        path: String,
        reader: &mut Box<dyn AsyncRead + Send + Unpin>,
    ) -> io::Result<Option<ProcessedContent>>;
}

/// 处理后的内容
#[derive(Debug, Clone)]
pub struct ProcessedContent {
    /// 文件路径
    pub path: String,
    /// 归档路径（如果来自归档内部）
    pub archive_path: Option<String>,
    /// 额外元数据（用于扩展）
    pub metadata: Vec<(String, String)>,
    /// 搜索结果（JSON 序列化，用于 LogSeek SearchResult 等）
    pub result: Option<serde_json::Value>,
}

impl ProcessedContent {
    /// 创建新的处理内容
    pub fn new(path: String) -> Self {
        Self {
            path,
            archive_path: None,
            metadata: Vec::new(),
            result: None,
        }
    }

    /// 设置归档路径
    pub fn with_archive_path(mut self, archive_path: Option<String>) -> Self {
        self.archive_path = archive_path;
        self
    }

    /// 设置搜索结果
    pub fn with_result(mut self, result: serde_json::Value) -> Self {
        self.result = Some(result);
        self
    }

    /// 添加元数据
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.push((key, value));
        self
    }
}

/// 预读结果：小文件完整内容，或大文件的已读取部分
pub(crate) enum PreloadResult {
    /// 小文件：完整内容已读取
    Complete(Vec<u8>),
    /// 大文件：已读取部分内容（reader 已被部分消费）
    Partial(Vec<u8>),
}

/// 预读缓冲区默认大小（64KB）
pub(crate) const DEFAULT_PRELOAD_BUFFER_SIZE: usize = 64 * 1024;

/// 预读文件条目到内存
///
/// 返回：
/// - Complete(content): 文件完全读取（小文件）
/// - Partial(content): 文件太大，只读取了部分（reader 已被部分消费）
pub(crate) async fn preload_entry(
    reader: &mut (dyn AsyncRead + Send + Unpin),
    max_size: usize,
) -> io::Result<PreloadResult> {
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
        let content = vec![0u8; 100];
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
        let content = vec![];
        let mut reader = &content[..];
        let res = preload_entry(&mut reader, 100).await.expect("preload failed");
        match res {
            PreloadResult::Complete(c) => {
                assert!(c.is_empty());
            }
            PreloadResult::Partial(_) => panic!("empty file should be complete"),
        }
    }

    #[test]
    fn test_processed_content() {
        let content = ProcessedContent::new("test.log".to_string());
        assert_eq!(content.path, "test.log");
        assert!(content.archive_path.is_none());
        assert!(content.metadata.is_empty());
        assert!(content.result.is_none());

        let content = content.with_archive_path(Some("archive.tar.gz".to_string()));
        assert_eq!(content.archive_path, Some("archive.tar.gz".to_string()));

        let content = content.with_result(serde_json::json!({"lines": ["test"]}));
        assert!(content.result.is_some());
    }
}
