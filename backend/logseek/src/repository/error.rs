use thiserror::Error;

/// Repository 层错误
///
/// 负责数据访问层的错误统一，包括数据库、存储、缓存等操作
#[derive(Debug, Clone, Error)]
pub enum RepositoryError {
  #[error("查询失败: {0}")]
  QueryFailed(String),

  #[error("对象存储错误: {0}")]
  StorageError(String),

  #[error("资源不存在: {0}")]
  NotFound(String),

  #[error("缓存操作失败: {0}")]
  CacheFailed(String),

  #[error("数据库错误: {0}")]
  Database(String),
}

/// Repository 层 Result 类型别名
pub type Result<T> = std::result::Result<T, RepositoryError>;

// 自动从 sqlx::Error 转换为 RepositoryError
impl From<sqlx::Error> for RepositoryError {
  fn from(err: sqlx::Error) -> Self {
    Self::Database(format!("数据库错误: {}", err))
  }
}
