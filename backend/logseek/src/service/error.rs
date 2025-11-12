use thiserror::Error;

/// Service 层错误
///
/// 负责业务逻辑层的错误统一，包括搜索、处理等操作
#[derive(Debug, Clone, Error)]
pub enum ServiceError {
  #[error("配置错误: {0}")]
  ConfigError(String),

  #[error("搜索失败 - 路径: {path}, 错误: {error}")]
  SearchFailed { path: String, error: String },

  #[error("数据处理错误: {0}")]
  ProcessingError(String),

  #[error("IO 错误: path={path}, error={error}")]
  IoError { path: String, error: String },

  #[error("Channel 已关闭: 接收端已断开连接")]
  ChannelClosed,

  /// Repository 层错误（自动转换）
  #[error(transparent)]
  Repository(#[from] crate::repository::RepositoryError),
}

/// Service 层 Result 类型别名
pub type Result<T> = std::result::Result<T, ServiceError>;

// 从 std::io::Error 转换
impl From<std::io::Error> for ServiceError {
  fn from(err: std::io::Error) -> Self {
    Self::ProcessingError(format!("IO 错误: {}", err))
  }
}
