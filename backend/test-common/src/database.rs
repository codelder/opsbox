//! 数据库测试工具
//!
//! 提供创建测试数据库、初始化schema、清理等工具函数

use opsbox_core::database::{DatabaseConfig, init_pool};
use sqlx::SqlitePool;
use tempfile::TempDir;
use crate::TestError;

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
            "sqlite::memory:".to_string(),
            crate::constants::TEST_DB_POOL_SIZE,
            crate::constants::TEST_DB_CONNECT_TIMEOUT,
        );

        let pool = init_pool(&config).await
            .map_err(|e| TestError::Database(format!("初始化内存数据库失败: {}", e)))?;

        Ok(Self {
            pool,
            temp_dir: None,
        })
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

        let pool = init_pool(&config).await
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
    logseek::init_schema(pool).await
        .map_err(|e| TestError::Database(format!("初始化logseek schema失败: {}", e)))
}

/// 清理所有测试数据
pub async fn cleanup_test_data(_pool: &SqlitePool) -> Result<(), TestError> {
    // 注意：这里应该删除测试数据，但保留表结构
    // 实际实现取决于具体的模块需求
    Ok(())
}