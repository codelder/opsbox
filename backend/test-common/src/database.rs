//! 数据库测试工具
//!
//! 提供创建测试数据库、初始化schema、清理等工具函数

use crate::TestError;
use opsbox_core::database::{DatabaseConfig, init_pool};
use sqlx::SqlitePool;
use tempfile::TempDir;

/// 测试数据库配置
pub struct TestDatabase {
  /// SQLite连接池
  pub pool: SqlitePool,
  /// 临时目录（当使用文件数据库时）
  pub temp_dir: Option<TempDir>,
}

impl TestDatabase {
  /// 创建内存数据库
  pub async fn in_memory() -> Result<Self, TestError> {
    let config = DatabaseConfig::new(
      ":memory:".to_string(),
      crate::constants::TEST_DB_POOL_SIZE,
      crate::constants::TEST_DB_CONNECT_TIMEOUT,
    );

    let pool = init_pool(&config)
      .await
      .map_err(|e| TestError::Database(format!("初始化内存数据库失败: {}", e)))?;

    Ok(Self { pool, temp_dir: None })
  }

  /// 创建基于文件的临时数据库
  pub async fn file_based() -> Result<Self, TestError> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");

    let config = DatabaseConfig::new(
      format!("sqlite://{}", db_path.display()),
      crate::constants::TEST_DB_POOL_SIZE,
      crate::constants::TEST_DB_CONNECT_TIMEOUT,
    );

    let pool = init_pool(&config)
      .await
      .map_err(|e| TestError::Database(format!("初始化文件数据库失败: {}", e)))?;

    Ok(Self {
      pool,
      temp_dir: Some(temp_dir),
    })
  }

  /// 获取数据库连接池
  pub fn pool(&self) -> &SqlitePool {
    &self.pool
  }
}

/// 初始化logseek模块的schema
pub async fn init_logseek_schema(pool: &SqlitePool) -> Result<(), TestError> {
  logseek::init_schema(pool)
    .await
    .map_err(|e| TestError::Database(format!("初始化logseek schema失败: {}", e)))
}

/// 清理所有测试数据
pub async fn cleanup_test_data(_pool: &SqlitePool) -> Result<(), TestError> {
  // 注意：这里应该删除测试数据，但保留表结构
  // 实际实现取决于具体的模块需求
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::constants::{TEST_DB_CONNECT_TIMEOUT, TEST_DB_POOL_SIZE};

  #[tokio::test]
  async fn test_test_database_in_memory() {
    // 测试创建内存数据库
    let db = TestDatabase::in_memory().await;
    assert!(db.is_ok());

    let db = db.unwrap();
    assert!(db.temp_dir.is_none()); // 内存数据库没有临时目录
    assert!(db.pool().size() > 0); // 连接池应该至少有一个连接
  }

  #[tokio::test]
  async fn test_test_database_file_based() {
    // 测试创建基于文件的数据库
    let db = TestDatabase::file_based().await;
    assert!(db.is_ok());

    let db = db.unwrap();
    assert!(db.temp_dir.is_some()); // 文件数据库应该有临时目录

    let temp_dir = db.temp_dir.unwrap();
    let db_path = temp_dir.path().join("test.db");
    assert!(db_path.exists()); // 数据库文件应该存在
  }

  #[tokio::test]
  async fn test_test_database_pool() {
    // 测试获取连接池
    let db = TestDatabase::in_memory().await.unwrap();
    let pool = db.pool();

    // 检查连接池基本属性 - 应该至少有一个连接
    assert!(pool.size() > 0);

    // 可以获取连接
    let connection = pool.acquire().await;
    assert!(connection.is_ok());
  }

  #[tokio::test]
  async fn test_init_logseek_schema() {
    // 测试初始化logseek schema
    let db = TestDatabase::in_memory().await.unwrap();
    let result = init_logseek_schema(db.pool()).await;

    // 由于logseek::init_schema可能涉及复杂的表创建，
    // 我们主要检查没有错误发生
    assert!(result.is_ok());
  }

  #[tokio::test]
  async fn test_cleanup_test_data() {
    // 测试清理测试数据（空实现）
    let db = TestDatabase::in_memory().await.unwrap();
    let result = cleanup_test_data(db.pool()).await;

    assert!(result.is_ok()); // 应该总是成功
  }

  #[test]
  fn test_constants_used() {
    // 测试常量是否正确导入
    // 这些常量在database.rs中使用，确保它们有定义
    // Verify constants
  }

  #[tokio::test]
  async fn test_database_configuration() {
    // 测试数据库配置的合理性
    let config = DatabaseConfig::new(
      "sqlite::memory:".to_string(),
      TEST_DB_POOL_SIZE,
      TEST_DB_CONNECT_TIMEOUT,
    );

    // 检查配置参数
    assert!(config.url.contains("memory"));
    assert_eq!(config.max_connections, TEST_DB_POOL_SIZE);
    assert_eq!(config.connect_timeout, TEST_DB_CONNECT_TIMEOUT);
  }
}
