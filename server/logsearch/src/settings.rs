use crate::storage::{self, StorageError};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use thiserror::Error;
use tokio::sync::OnceCell;

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
      let db_path = std::env::var("LOGSEARCH_SETTINGS_DB").unwrap_or_else(|_| "logsearch_settings.db".to_string());
      if let Some(parent) = Path::new(&db_path).parent() {
        if !parent.as_os_str().is_empty() {
          tokio::fs::create_dir_all(parent).await.ok();
        }
      }
      // Ensure the database file exists and is writable before connecting.
      if !db_path.starts_with("sqlite://") {
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
      let pool = SqlitePoolOptions::new().max_connections(5).connect(&db_url).await?;

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

      Ok::<SqlitePool, sqlx::Error>(pool)
    })
    .await
    .map_err(SettingsError::Database)?;
  Ok(pool)
}

pub async fn load_minio_settings() -> Result<Option<MinioSettings>, SettingsError> {
  let pool = pool().await?;
  let row = sqlx::query_as::<_, (String, String, String, String)>(
    "SELECT endpoint, bucket, access_key, secret_key FROM minio_settings WHERE id = 1",
  )
  .fetch_optional(pool)
  .await?;
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
  // Ensure the database and table exist even if validation fails
  let pool = pool().await?;
  verify_minio_settings(settings).await?;
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
  Ok(())
}

pub async fn verify_minio_settings(settings: &MinioSettings) -> Result<(), SettingsError> {
  match storage::test_minio_connection(
    &settings.endpoint,
    &settings.access_key,
    &settings.secret_key,
    &settings.bucket,
  )
  .await
  {
    Ok(_) => Ok(()),
    Err(StorageError::InvalidBaseUrl(_)) => Err(SettingsError::Connection(
      "Endpoint 地址无效，请确认格式例如 http://host:9000".to_string(),
    )),
    Err(StorageError::MinioBuild) => Err(SettingsError::Connection(
      "无法建立 MinIO 连接，请检查 Endpoint、Access Key 和 Secret Key".to_string(),
    )),
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
  }
}
