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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repository_error_display() {
        let err = RepositoryError::QueryFailed("SQL syntax error".to_string());
        assert_eq!(err.to_string(), "查询失败: SQL syntax error");

        let err = RepositoryError::StorageError("S3 connection failed".to_string());
        assert_eq!(err.to_string(), "对象存储错误: S3 connection failed");

        let err = RepositoryError::NotFound("User ID 123".to_string());
        assert_eq!(err.to_string(), "资源不存在: User ID 123");

        let err = RepositoryError::CacheFailed("Redis timeout".to_string());
        assert_eq!(err.to_string(), "缓存操作失败: Redis timeout");

        let err = RepositoryError::Database("Connection pool exhausted".to_string());
        assert_eq!(err.to_string(), "数据库错误: Connection pool exhausted");
    }

    #[test]
    fn test_repository_error_clone() {
        let err = RepositoryError::NotFound("test".to_string());
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
    }

    #[test]
    fn test_result_type_alias() {
        let ok_result: Result<i32> = Ok(42);
        assert!(ok_result.is_ok());
        assert_eq!(ok_result.unwrap(), 42);

        let err_result: Result<i32> = Err(RepositoryError::NotFound("test".to_string()));
        assert!(err_result.is_err());
    }
}
