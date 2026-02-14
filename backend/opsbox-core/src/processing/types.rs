//! Processing types - 内容处理相关类型定义
//!
//! 定义内容处理器 trait 和处理后的内容结构。

use std::io;

use async_trait::async_trait;
use tokio::io::AsyncRead;

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

#[cfg(test)]
mod tests {
  use super::*;

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
