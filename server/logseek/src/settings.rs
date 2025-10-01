use crate::storage::{self, StorageError};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use thiserror::Error;
use tokio::sync::OnceCell;
use log::{debug, info, error};

#[derive(Debug, Error)]
pub enum SettingsError {
  #[error("数据库错误: {0}")]
  Database(#[from] sqlx::Error),
  #[error("未配置 MinIO 连接")]
  NotConfigured,
  #[error("MinIO 连接失败: {0}")]
  Connection(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MinioSettings {
  pub endpoint: String,
  pub bucket: String,
  pub access_key: String,
  pub secret_key: String,
}

static DB_POOL: OnceCell<SqlitePool> = OnceCell::const_new();

async fn pool() -> Result<&'static SqlitePool, SettingsError> {
  let pool = DB_POOL
    .get_or_try_init(|| async {
      let db_path = std::env::var("LOGSEARCH_SETTINGS_DB").unwrap_or_else(|_| "logseek_settings.db".to_string());
      info!("初始化设置数据库: {}", db_path);
      
      if let Some(parent) = Path::new(&db_path).parent() {
        if !parent.as_os_str().is_empty() {
          debug!("创建数据库目录: {:?}", parent);
          tokio::fs::create_dir_all(parent).await.ok();
        }
      }
      // Ensure the database file exists and is writable before connecting.
      if !db_path.starts_with("sqlite://") {
        debug!("确保数据库文件存在");
        let _ = tokio::fs::OpenOptions::new()
          .create(true)
          .write(true)
          .open(&db_path)
          .await;
      }
      let db_url = if db_path.starts_with("sqlite://") {
        db_path
      } else {
        format!("sqlite://{}", db_path)
      };
      debug!("连接SQLite数据库: {}", db_url);
      let pool = SqlitePoolOptions::new().max_connections(5).connect(&db_url).await?;
      info!("数据库连接池创建成功，最大连接数: 5");

      debug!("创建minio_settings表");
      sqlx::query(
        "CREATE TABLE IF NOT EXISTS minio_settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            endpoint TEXT NOT NULL,
            bucket TEXT NOT NULL,
            access_key TEXT NOT NULL,
            secret_key TEXT NOT NULL
        )",
      )
      .execute(&pool)
      .await?;
      debug!("数据库表创建成功");

      Ok::<SqlitePool, sqlx::Error>(pool)
    })
    .await
    .map_err(SettingsError::Database)?;
  Ok(pool)
}

pub async fn load_minio_settings() -> Result<Option<MinioSettings>, SettingsError> {
  debug!("加载MinIO配置");
  let pool = pool().await?;
  let row = sqlx::query_as::<_, (String, String, String, String)>(
    "SELECT endpoint, bucket, access_key, secret_key FROM minio_settings WHERE id = 1",
  )
  .fetch_optional(pool)
  .await?;
  
  match &row {
    Some(_) => info!("MinIO配置加载成功"),
    None => info!("MinIO配置不存在")
  }
  
  Ok(row.map(|(endpoint, bucket, access_key, secret_key)| MinioSettings {
    endpoint,
    bucket,
    access_key,
    secret_key,
  }))
}

pub async fn load_required_minio_settings() -> Result<MinioSettings, SettingsError> {
  load_minio_settings().await?.ok_or(SettingsError::NotConfigured)
}

/// Ensure the settings database (and table) exist.
pub async fn ensure_store() -> Result<(), SettingsError> {
  let _ = pool().await?;
  Ok(())
}

pub async fn save_minio_settings(settings: &MinioSettings) -> Result<(), SettingsError> {
  info!("保存MinIO配置: endpoint={}, bucket={}", settings.endpoint, settings.bucket);
  
  // Ensure the database and table exist even if validation fails
  let pool = pool().await?;
  
  debug!("验证MinIO配置有效性");
  verify_minio_settings(settings).await?;
  
  debug!("将MinIO配置写入数据库");
  sqlx::query(
    "INSERT INTO minio_settings (id, endpoint, bucket, access_key, secret_key)
     VALUES (1, ?, ?, ?, ?)
     ON CONFLICT(id) DO UPDATE SET endpoint = excluded.endpoint, bucket = excluded.bucket,
       access_key = excluded.access_key, secret_key = excluded.secret_key",
  )
  .bind(&settings.endpoint)
  .bind(&settings.bucket)
  .bind(&settings.access_key)
  .bind(&settings.secret_key)
  .execute(pool)
  .await?;
  
  info!("MinIO配置保存成功");
  Ok(())
}

pub async fn verify_minio_settings(settings: &MinioSettings) -> Result<(), SettingsError> {
  debug!("开始验证MinIO连接: endpoint={}, bucket={}", settings.endpoint, settings.bucket);
  
  match storage::test_minio_connection(
    &settings.endpoint,
    &settings.access_key,
    &settings.secret_key,
    &settings.bucket,
  )
  .await
  {
    Ok(_) => {
      info!("MinIO配置验证成功");
      Ok(())
    },
    Err(StorageError::InvalidBaseUrl(_)) => {
      error!("MinIO Endpoint地址无效: {}", settings.endpoint);
      Err(SettingsError::Connection(
        "Endpoint 地址无效，请确认格式例如 http://host:9000".to_string(),
      ))
    },
    Err(StorageError::MinioBuild) => {
      error!("MinIO客户端构建失败: endpoint={}", settings.endpoint);
      Err(SettingsError::Connection(
        "无法建立 MinIO 连接，请检查 Endpoint、Access Key 和 Secret Key".to_string(),
      ))
    },
    Err(StorageError::MinioListObjects(msg)) => {
      let lower = msg.to_ascii_lowercase();
      if lower.contains("nosuchbucket") {
        Err(SettingsError::Connection(format!(
          "桶 {} 不存在或无权限访问，请确认 Bucket 名称",
          settings.bucket
        )))
      } else if lower.contains("accessdenied") || lower.contains("signature") {
        Err(SettingsError::Connection(
          "访问被拒绝，请确认 Access Key 与 Secret Key 是否正确".to_string(),
        ))
      } else {
        Err(SettingsError::Connection(format!(
          "无法列举桶 {}：{}",
          settings.bucket, msg
        )))
      }
    }
    Err(StorageError::MinioGetObject(msg)) => Err(SettingsError::Connection(format!(
      "无法读取对象：{}",
      msg
    ))),
    Err(StorageError::MinioToStream(msg)) => Err(SettingsError::Connection(format!(
      "读取对象流失败：{}",
      msg
    ))),
    Err(StorageError::Regex(msg)) => Err(SettingsError::Connection(format!(
      "无效的对象筛选正则：{}",
      msg
    ))),
    Err(StorageError::Io(err)) => Err(SettingsError::Connection(format!(
      "网络通信错误：{}",
      err
    ))),
    Err(StorageError::ConnectionTimeout) => Err(SettingsError::Connection(
      "连接超时，请检查网络或 MinIO 服务状态".to_string(),
    )),
  }
}
