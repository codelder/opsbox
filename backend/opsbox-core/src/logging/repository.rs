//! 日志配置 Repository
//!
//! 提供日志配置的数据库持久化操作

use crate::logging::{LogError, LogLevel};
use sqlx::SqlitePool;
use std::path::PathBuf;

/// 日志配置数据模型
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LogConfigModel {
  pub id: i64,
  pub component: String,
  pub level: String,
  pub retention_count: i64,
  pub updated_at: i64,
}

/// 日志配置响应
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogConfigResponse {
  /// 日志级别
  pub level: String,
  /// 日志保留数量（天）
  pub retention_count: usize,
  /// 日志目录
  pub log_dir: String,
}

/// 日志配置仓库
#[derive(Clone)]
pub struct LogConfigRepository {
  pool: SqlitePool,
}

impl LogConfigRepository {
  /// 创建新的日志配置仓库
  pub fn new(pool: SqlitePool) -> Self {
    Self { pool }
  }

  /// 获取日志配置
  pub async fn get(&self, component: &str) -> Result<LogConfigModel, LogError> {
    // 确保配置存在
    self.ensure_config_exists(component).await?;

    let config = sqlx::query_as::<_, LogConfigModel>(
      "SELECT id, component, level, retention_count, updated_at FROM log_config WHERE component = ?",
    )
    .bind(component)
    .fetch_one(&self.pool)
    .await
    .map_err(|e| LogError::InvalidConfig(format!("查询日志配置失败: {}", e)))?;

    Ok(config)
  }

  /// 更新日志级别
  pub async fn update_level(&self, component: &str, level: LogLevel) -> Result<(), LogError> {
    // 确保配置存在
    self.ensure_config_exists(component).await?;

    let level_str = level.to_string();
    let now = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap()
      .as_secs() as i64;

    sqlx::query("UPDATE log_config SET level = ?, updated_at = ? WHERE component = ?")
      .bind(&level_str)
      .bind(now)
      .bind(component)
      .execute(&self.pool)
      .await
      .map_err(|e| LogError::InvalidConfig(format!("更新日志级别失败: {}", e)))?;

    tracing::info!("已更新 {} 的日志级别为 {}", component, level_str);
    Ok(())
  }

  /// 更新日志保留数量
  pub async fn update_retention(&self, component: &str, count: usize) -> Result<(), LogError> {
    // 确保配置存在
    self.ensure_config_exists(component).await?;

    let now = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap()
      .as_secs() as i64;

    sqlx::query("UPDATE log_config SET retention_count = ?, updated_at = ? WHERE component = ?")
      .bind(count as i64)
      .bind(now)
      .bind(component)
      .execute(&self.pool)
      .await
      .map_err(|e| LogError::InvalidConfig(format!("更新日志保留数量失败: {}", e)))?;

    tracing::info!("已更新 {} 的日志保留数量为 {} 天", component, count);
    Ok(())
  }

  /// 确保配置存在，如果不存在则创建默认配置
  async fn ensure_config_exists(&self, component: &str) -> Result<(), LogError> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM log_config WHERE component = ?")
      .bind(component)
      .fetch_one(&self.pool)
      .await
      .map_err(|e| LogError::InvalidConfig(format!("检查配置是否存在失败: {}", e)))?;

    if exists == 0 {
      let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

      sqlx::query("INSERT INTO log_config (component, level, retention_count, updated_at) VALUES (?, 'info', 7, ?)")
        .bind(component)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| LogError::InvalidConfig(format!("创建默认日志配置失败: {}", e)))?;

      tracing::info!("已为 {} 创建默认日志配置", component);
    }

    Ok(())
  }

  /// 获取日志配置响应（包含日志目录）
  pub async fn get_response(&self, component: &str, log_dir: PathBuf) -> Result<LogConfigResponse, LogError> {
    let config = self.get(component).await?;
    Ok(LogConfigResponse {
      level: config.level,
      retention_count: config.retention_count as usize,
      log_dir: log_dir.to_string_lossy().to_string(),
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::logging::schema::LOG_CONFIG_SCHEMA;

  async fn setup_test_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:")
      .await
      .expect("Failed to create test database");

    sqlx::query(LOG_CONFIG_SCHEMA)
      .execute(&pool)
      .await
      .expect("Failed to run migration");

    pool
  }

  #[tokio::test]
  async fn test_get_default_config() {
    let pool = setup_test_db().await;
    let repo = LogConfigRepository::new(pool);

    let config = repo.get("server").await.unwrap();
    assert_eq!(config.component, "server");
    assert_eq!(config.level, "info");
    assert_eq!(config.retention_count, 7);
  }

  #[tokio::test]
  async fn test_update_level() {
    let pool = setup_test_db().await;
    let repo = LogConfigRepository::new(pool);

    // 首先获取配置以确保它存在
    let _ = repo.get("server").await.unwrap();

    // 更新日志级别
    repo.update_level("server", LogLevel::Debug).await.unwrap();

    // 验证更新
    let config = repo.get("server").await.unwrap();
    assert_eq!(config.level, "debug");
  }

  #[tokio::test]
  async fn test_update_level_all_levels() {
    let pool = setup_test_db().await;
    let repo = LogConfigRepository::new(pool);

    // 测试所有日志级别
    for level in [
      LogLevel::Error,
      LogLevel::Warn,
      LogLevel::Info,
      LogLevel::Debug,
      LogLevel::Trace,
    ] {
      repo.update_level("server", level).await.unwrap();
      let config = repo.get("server").await.unwrap();
      assert_eq!(config.level, level.to_string());
    }
  }

  #[tokio::test]
  async fn test_update_retention() {
    let pool = setup_test_db().await;
    let repo = LogConfigRepository::new(pool);

    // 首先获取配置以确保它存在
    let _ = repo.get("server").await.unwrap();

    // 更新保留数量
    repo.update_retention("server", 14).await.unwrap();

    // 验证更新
    let config = repo.get("server").await.unwrap();
    assert_eq!(config.retention_count, 14);
  }

  #[tokio::test]
  async fn test_update_retention_various_values() {
    let pool = setup_test_db().await;
    let repo = LogConfigRepository::new(pool);

    // 测试不同的保留数量
    for count in [1, 7, 14, 30, 90, 365] {
      repo.update_retention("server", count).await.unwrap();
      let config = repo.get("server").await.unwrap();
      assert_eq!(config.retention_count, count as i64);
    }
  }

  #[tokio::test]
  async fn test_get_response() {
    let pool = setup_test_db().await;
    let repo = LogConfigRepository::new(pool);

    let log_dir = PathBuf::from("/var/log/opsbox");
    let response = repo.get_response("server", log_dir).await.unwrap();

    assert_eq!(response.level, "info");
    assert_eq!(response.retention_count, 7);
    assert_eq!(response.log_dir, "/var/log/opsbox");
  }

  #[tokio::test]
  async fn test_create_default_for_agent() {
    let pool = setup_test_db().await;
    let repo = LogConfigRepository::new(pool);

    // 获取 agent 配置（应该自动创建）
    let config = repo.get("agent").await.unwrap();
    assert_eq!(config.component, "agent");
    assert_eq!(config.level, "info");
    assert_eq!(config.retention_count, 7);
  }

  #[tokio::test]
  async fn test_multiple_components() {
    let pool = setup_test_db().await;
    let repo = LogConfigRepository::new(pool);

    // 创建多个组件的配置
    let server_config = repo.get("server").await.unwrap();
    let agent_config = repo.get("agent").await.unwrap();
    let custom_config = repo.get("custom").await.unwrap();

    assert_eq!(server_config.component, "server");
    assert_eq!(agent_config.component, "agent");
    assert_eq!(custom_config.component, "custom");

    // 更新一个组件不应影响其他组件
    repo.update_level("server", LogLevel::Debug).await.unwrap();
    repo.update_retention("agent", 30).await.unwrap();

    let server_config = repo.get("server").await.unwrap();
    let agent_config = repo.get("agent").await.unwrap();

    assert_eq!(server_config.level, "debug");
    assert_eq!(server_config.retention_count, 7);
    assert_eq!(agent_config.level, "info");
    assert_eq!(agent_config.retention_count, 30);
  }

  #[tokio::test]
  async fn test_updated_at_timestamp() {
    let pool = setup_test_db().await;
    let repo = LogConfigRepository::new(pool);

    let config1 = repo.get("server").await.unwrap();
    let timestamp1 = config1.updated_at;

    // 等待一小段时间确保时间戳不同
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    repo.update_level("server", LogLevel::Debug).await.unwrap();
    let config2 = repo.get("server").await.unwrap();
    let timestamp2 = config2.updated_at;

    assert!(timestamp2 > timestamp1, "Updated timestamp should be greater");
  }

  #[tokio::test]
  async fn test_repository_clone() {
    let pool = setup_test_db().await;
    let repo1 = LogConfigRepository::new(pool);
    let repo2 = repo1.clone();

    // 通过 repo1 更新
    repo1.update_level("server", LogLevel::Debug).await.unwrap();

    // 通过 repo2 读取
    let config = repo2.get("server").await.unwrap();
    assert_eq!(config.level, "debug");
  }
}
