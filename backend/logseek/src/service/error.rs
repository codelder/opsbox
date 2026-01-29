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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_service_error_config_error() {
    let err = ServiceError::ConfigError("配置文件缺失".to_string());
    assert_eq!(err.to_string(), "配置错误: 配置文件缺失");
  }

  #[test]
  fn test_service_error_search_failed() {
    let err = ServiceError::SearchFailed {
      path: "/test/path".to_string(),
      error: "连接超时".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("/test/path"));
    assert!(msg.contains("连接超时"));
  }

  #[test]
  fn test_service_error_processing_error() {
    let err = ServiceError::ProcessingError("数据解析失败".to_string());
    assert_eq!(err.to_string(), "数据处理错误: 数据解析失败");
  }

  #[test]
  fn test_service_error_io_error() {
    let err = ServiceError::IoError {
      path: "/test/file.log".to_string(),
      error: "文件不存在".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("/test/file.log"));
    assert!(msg.contains("文件不存在"));
  }

  #[test]
  fn test_service_error_channel_closed() {
    let err = ServiceError::ChannelClosed;
    assert_eq!(err.to_string(), "Channel 已关闭: 接收端已断开连接");
  }

  #[test]
  fn test_service_error_from_io_error() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "文件未找到");
    let service_err: ServiceError = io_err.into();

    match service_err {
      ServiceError::ProcessingError(msg) => {
        assert!(msg.contains("IO 错误"));
        assert!(msg.contains("文件未找到"));
      }
      _ => panic!("期望 ProcessingError"),
    }
  }

  #[test]
  fn test_service_error_from_repository_error() {
    let repo_err = crate::repository::RepositoryError::NotFound("资源不存在".to_string());
    let service_err: ServiceError = repo_err.into();

    match service_err {
      ServiceError::Repository(_) => {
        // 成功转换
      }
      _ => panic!("期望 Repository 错误"),
    }
  }
}
