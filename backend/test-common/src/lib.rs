//! 共享测试工具库
//!
//! 为OpsBox测试提供共享的工具函数、mock实现和测试夹具

pub mod agent_mock;
pub mod archive_utils;
pub mod database;
pub mod file_utils;
pub mod llm_mock;
pub mod orl_utils;
pub mod performance;
pub mod s3_mock;
pub mod search_utils;
pub mod security;
pub mod test_monitoring;

/// 测试配置常量
pub mod constants {
  /// 测试数据库连接字符串
  pub const TEST_DB_CONNECTION: &str = "sqlite::memory:";
  /// 测试数据库连接池大小
  pub const TEST_DB_POOL_SIZE: u32 = 5;
  /// 测试数据库连接超时（秒）
  pub const TEST_DB_CONNECT_TIMEOUT: u64 = 30;

  /// 测试文件目录前缀
  pub const TEST_FILE_DIR_PREFIX: &str = "opsbox_test_";

  /// 测试Agent端口范围
  pub const AGENT_PORT_START: u16 = 15000;
  pub const AGENT_PORT_END: u16 = 16000;

  /// 测试S3端口范围
  pub const S3_PORT_START: u16 = 17000;
  pub const S3_PORT_END: u16 = 18000;
}

/// 通用测试错误类型
#[derive(Debug)]
pub enum TestError {
  Io(std::io::Error),
  Database(String),
  Network(String),
  Timeout(String),
  Archive(String),
  Other(String),
}

impl From<std::io::Error> for TestError {
  fn from(err: std::io::Error) -> Self {
    TestError::Io(err)
  }
}

impl From<async_zip::error::ZipError> for TestError {
  fn from(err: async_zip::error::ZipError) -> Self {
    TestError::Archive(err.to_string())
  }
}

// Note: We handle other archive-related errors explicitly using map_err in archive_utils.rs
// to avoid conflicting implementations and missing error types

impl std::fmt::Display for TestError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      TestError::Io(e) => write!(f, "IO error: {}", e),
      TestError::Database(e) => write!(f, "Database error: {}", e),
      TestError::Network(e) => write!(f, "Network error: {}", e),
      TestError::Timeout(e) => write!(f, "Timeout error: {}", e),
      TestError::Archive(e) => write!(f, "Archive error: {}", e),
      TestError::Other(e) => write!(f, "Other error: {}", e),
    }
  }
}

impl std::error::Error for TestError {}
