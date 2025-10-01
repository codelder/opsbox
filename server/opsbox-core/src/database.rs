use crate::error::{AppError, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
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

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "./opsbox.db".to_string(),
            max_connections: 10,
            connect_timeout: 30,
        }
    }
}

impl DatabaseConfig {
    /// 从环境变量创建配置
    pub fn from_env() -> Self {
        let url = std::env::var("OPSBOX_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .unwrap_or_else(|_| "./opsbox.db".to_string());

        let max_connections = std::env::var("OPSBOX_DATABASE_MAX_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        let connect_timeout = std::env::var("OPSBOX_DATABASE_CONNECT_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        Self {
            url,
            max_connections,
            connect_timeout,
        }
    }

    /// 使用指定路径创建配置
    pub fn with_path(path: impl AsRef<Path>) -> Self {
        Self {
            url: path.as_ref().to_string_lossy().to_string(),
            ..Default::default()
        }
    }
}

/// 初始化数据库连接池
pub async fn init_pool(config: &DatabaseConfig) -> Result<SqlitePool> {
    log::info!("初始化数据库连接池: {}", config.url);

    // 解析连接选项
    let connect_options = if config.url.starts_with("sqlite://") {
        SqliteConnectOptions::from_str(&config.url)
            .map_err(|e| AppError::config(format!("无效的数据库 URL: {}", e)))?
    } else {
        SqliteConnectOptions::new()
            .filename(&config.url)
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

    log::info!(
        "数据库连接池初始化成功，最大连接数: {}",
        config.max_connections
    );

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
    log::info!("执行 {} 模块的数据库迁移", module);
    
    sqlx::query(sql)
        .execute(pool)
        .await
        .map_err(|e| {
            log::error!("{} 模块数据库迁移失败: {}", module, e);
            AppError::Database(e)
        })?;

    log::info!("{} 模块数据库迁移完成", module);
    Ok(())
}

