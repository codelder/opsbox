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

#[cfg(test)]
mod tests {
  use super::*;

  async fn setup_db() -> SqlitePool {
    // Use a unique in-memory database for each test by using a unique name
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    // Create table manually for testing as schema location is unknown/distributed
    let sql = r#"
            CREATE TABLE IF NOT EXISTS s3_profiles (
                profile_name TEXT PRIMARY KEY,
                endpoint TEXT NOT NULL,
                access_key TEXT NOT NULL,
                secret_key TEXT NOT NULL
            );
        "#;
    sqlx::query(sql).execute(&pool).await.unwrap();
    pool
  }

  #[tokio::test]
  async fn test_load_s3_profile() {
    let pool = setup_db().await;

    // Clear any existing data
    sqlx::query("DELETE FROM s3_profiles").execute(&pool).await.unwrap();

    // Insert test data
    sqlx::query("INSERT INTO s3_profiles (profile_name, endpoint, access_key, secret_key) VALUES (?, ?, ?, ?)")
      .bind("default")
      .bind("http://minio:9000")
      .bind("ak")
      .bind("sk")
      .execute(&pool)
      .await
      .unwrap();

    let profile = load_s3_profile(&pool, "default").await.unwrap().unwrap();
    assert_eq!(profile.endpoint, "http://minio:9000");
    assert_eq!(profile.access_key, "ak");

    let not_found = load_s3_profile(&pool, "nonexistent").await.unwrap();
    assert!(not_found.is_none());

    // Empty name defaults to "default"
    let default_loader = load_s3_profile(&pool, "").await.unwrap().unwrap();
    assert_eq!(default_loader.profile_name, "default");
  }

  #[tokio::test]
  async fn test_list_s3_profiles() {
    let pool = setup_db().await;

    // Clear any existing data
    sqlx::query("DELETE FROM s3_profiles").execute(&pool).await.unwrap();

    sqlx::query("INSERT INTO s3_profiles (profile_name, endpoint, access_key, secret_key) VALUES (?, ?, ?, ?)")
      .bind("a")
      .bind("e1")
      .bind("k1")
      .bind("s1")
      .execute(&pool)
      .await
      .unwrap();

    sqlx::query("INSERT INTO s3_profiles (profile_name, endpoint, access_key, secret_key) VALUES (?, ?, ?, ?)")
      .bind("b")
      .bind("e2")
      .bind("k2")
      .bind("s2")
      .execute(&pool)
      .await
      .unwrap();

    let profiles = list_s3_profiles(&pool).await.unwrap();
    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0].profile_name, "a");
    assert_eq!(profiles[1].profile_name, "b");
  }
}
