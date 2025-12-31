use super::orl::OpsPath;
use super::types::{OpsEntry, OpsMetadata};
use async_trait::async_trait;
use std::pin::Pin;
use tokio::io::AsyncRead;

/// 异步读取流类型别名
pub type OpsRead = Pin<Box<dyn AsyncRead + Send + Unpin>>;

/// OpsBox 文件系统接口
///
/// 所有存储提供者（Local, S3, Agent）都必须实现此接口
#[async_trait]
pub trait OpsFileSystem: Send + Sync {
  /// 获取资源元数据
  async fn metadata(&self, path: &OpsPath) -> std::io::Result<OpsMetadata>;

  /// 列出目录内容
  async fn read_dir(&self, path: &OpsPath) -> std::io::Result<Vec<OpsEntry>>;

  /// 打开资源进行读取
  /// 返回一个实现了 AsyncRead 的流
  /// 对于压缩文件，实现者应根据情况返回原始流或解压流（通常由上层决策，这里建议返回原始流，除非明确设计为透明解压层）
  async fn open_read(&self, path: &OpsPath) -> std::io::Result<OpsRead>;

  /// 获取当前文件系统的标识（用于日志或调试）
  fn name(&self) -> &str;
}
