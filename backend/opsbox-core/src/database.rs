use crate::error::{AppError, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use std::time::Duration;

/// 数据库配置
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
  /// 数据库文件路径或连接 URL
  pub url: String,
  /// 最大连接数
  pub max_connections: u32,
  /// 连接超时时间（秒）
  pub connect_timeout: u64,
}

impl DatabaseConfig {
  /// 创建数据库配置
  pub fn new(url: String, max_connections: u32, connect_timeout: u64) -> Self {
    Self {
      url,
      max_connections,
      connect_timeout,
    }
  }
}

/// 初始化数据库连接池
pub async fn init_pool(config: &DatabaseConfig) -> Result<SqlitePool> {
  tracing::info!("初始化数据库连接池: {}", config.url);

  // 解析连接选项
  let connect_options = if config.url.starts_with("sqlite://") {
    SqliteConnectOptions::from_str(&config.url).map_err(|e| AppError::config(format!("无效的数据库 URL: {}", e)))?
  } else {
    SqliteConnectOptions::new().filename(&config.url)
  };

  // 配置连接选项
  let connect_options = connect_options
    .create_if_missing(true)
    .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
    .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
    .busy_timeout(Duration::from_secs(10));

  // 创建连接池
  let pool = SqlitePoolOptions::new()
    .max_connections(config.max_connections)
    .acquire_timeout(Duration::from_secs(config.connect_timeout))
    .connect_with(connect_options)
    .await
    .map_err(AppError::Database)?;

  tracing::info!("数据库连接池初始化成功，最大连接数: {}", config.max_connections);

  Ok(pool)
}

/// 数据库健康检查
pub async fn health_check(pool: &SqlitePool) -> Result<()> {
  sqlx::query("SELECT 1")
    .execute(pool)
    .await
    .map_err(AppError::Database)?;
  Ok(())
}

/// 辅助函数：执行数据库迁移（各模块调用）
pub async fn run_migration(pool: &SqlitePool, sql: &str, module: &str) -> Result<()> {
  tracing::info!("执行 {} 模块的数据库迁移", module);

  sqlx::query(sql).execute(pool).await.map_err(|e| {
    tracing::error!("{} 模块数据库迁移失败: {}", module, e);
    AppError::Database(e)
  })?;

  tracing::info!("{} 模块数据库迁移完成", module);
  Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_config_new() {
        let config = DatabaseConfig::new("sqlite::memory:".to_string(), 5, 10);
        assert_eq!(config.url, "sqlite::memory:");
        assert_eq!(config.max_connections, 5);
        assert_eq!(config.connect_timeout, 10);
    }

    #[tokio::test]
    async fn test_init_pool_memory() {
        // Use in-memory database for testing
        let config = DatabaseConfig::new("sqlite::memory:".to_string(), 1, 5);
        let pool = init_pool(&config).await.expect("Failed to init pool");

        health_check(&pool).await.expect("Health check failed");

        // Test migration helper
        run_migration(&pool, "CREATE TABLE IF NOT EXISTS test (id INTEGER PRIMARY KEY)", "test_module")
            .await
            .expect("Migration failed");

        let row: (i32,) = sqlx::query_as("SELECT COUNT(*) FROM test")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, 0);
    }
}
