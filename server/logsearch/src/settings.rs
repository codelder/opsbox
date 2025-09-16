use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use thiserror::Error;
use tokio::sync::OnceCell;

#[derive(Debug, Error)]
pub enum SettingsError {
  #[error("数据库错误: {0}")]
  Database(#[from] sqlx::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MinioSettings {
  pub endpoint: String,
  pub bucket: String,
  pub access_key: String,
  pub secret_key: String,
}

impl Default for MinioSettings {
  fn default() -> Self {
    Self {
      endpoint: std::env::var("LOGSEARCH_MINIO_ENDPOINT").unwrap_or_else(|_| "http://192.168.50.61:9002".to_string()),
      bucket: std::env::var("LOGSEARCH_MINIO_BUCKET").unwrap_or_else(|_| "backupdr".to_string()),
      access_key: std::env::var("LOGSEARCH_MINIO_ACCESS_KEY").unwrap_or_else(|_| "admin".to_string()),
      secret_key: std::env::var("LOGSEARCH_MINIO_SECRET_KEY").unwrap_or_else(|_| "G5t3o6f2".to_string()),
    }
  }
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

pub async fn load_or_default_minio_settings() -> Result<MinioSettings, SettingsError> {
  Ok(load_minio_settings().await?.unwrap_or_default())
}

pub async fn save_minio_settings(settings: &MinioSettings) -> Result<(), SettingsError> {
  let pool = pool().await?;
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
