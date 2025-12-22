use crate::SqlitePool;
use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// S3 兼容对象存储配置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct S3Settings {
  pub endpoint: String,
  pub access_key: String,
  pub secret_key: String,
}

/// S3 配置 Profile
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct S3Profile {
  pub profile_name: String,
  pub endpoint: String,
  pub access_key: String,
  pub secret_key: String,
}

/// 加载指定 profile 的 S3 配置
pub async fn load_s3_profile(pool: &SqlitePool, profile_name: &str) -> Result<Option<S3Profile>> {
  let profile_name = if profile_name.is_empty() {
    "default"
  } else {
    profile_name
  };
  debug!("加载 S3 Profile: {}", profile_name);

  let row = sqlx::query_as::<_, (String, String, String, String)>(
    "SELECT profile_name, endpoint, access_key, secret_key FROM s3_profiles WHERE profile_name = ?",
  )
  .bind(profile_name)
  .fetch_optional(pool)
  .await
  .map_err(AppError::Database)?;

  Ok(row.map(|(profile_name, endpoint, access_key, secret_key)| S3Profile {
    profile_name,
    endpoint,
    access_key,
    secret_key,
  }))
}

/// 列出所有 S3 Profile
pub async fn list_s3_profiles(pool: &SqlitePool) -> Result<Vec<S3Profile>> {
  let rows = sqlx::query_as::<_, (String, String, String, String)>(
    "SELECT profile_name, endpoint, access_key, secret_key FROM s3_profiles ORDER BY profile_name",
  )
  .fetch_all(pool)
  .await
  .map_err(AppError::Database)?;

  Ok(
    rows
      .into_iter()
      .map(|(profile_name, endpoint, access_key, secret_key)| S3Profile {
        profile_name,
        endpoint,
        access_key,
        secret_key,
      })
      .collect(),
  )
}
