use super::RepositoryError;
use super::error::Result;
use crate::utils::storage::{self, S3Error};
use opsbox_core::{SqlitePool, run_migration};
use tracing::{debug, error, info};
pub use opsbox_core::repository::s3::{S3Settings, S3Profile};
use opsbox_core::repository::s3::{
    load_s3_profile as core_load_s3_profile,
    list_s3_profiles as core_list_s3_profiles
};

// Structs moved to opsbox_core::repository::s3

/// 初始化 LogSeek 模块的数据库表
pub async fn init_schema(db_pool: &SqlitePool) -> Result<()> {
  // 完全采用 profiles 存储（默认 profile_name='default'）
  let schema_sql = r#"
    CREATE TABLE IF NOT EXISTS s3_profiles (
        profile_name TEXT PRIMARY KEY,
        endpoint TEXT NOT NULL,
        access_key TEXT NOT NULL,
        secret_key TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
  "#;

  run_migration(db_pool, schema_sql, "logseek")
    .await
    .map_err(|e| RepositoryError::Database(e.to_string()))?;

  Ok(())
}

pub async fn load_s3_settings(pool: &SqlitePool) -> Result<Option<S3Settings>> {
  debug!("加载 S3 配置（default profile）");
  let profile = core_load_s3_profile(pool, "default")
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("查询 S3 配置失败: {}", e)))?;

  match &profile {
    Some(_) => info!("S3 配置加载成功 (profile=default)"),
    None => info!("S3 配置不存在 (profile=default)"),
  }

  Ok(profile.map(|p| S3Settings {
    endpoint: p.endpoint,
    access_key: p.access_key,
    secret_key: p.secret_key,
  }))
}

pub async fn load_required_s3_settings(pool: &SqlitePool) -> Result<S3Settings> {
  load_s3_settings(pool)
    .await?
    .ok_or_else(|| RepositoryError::NotFound("未配置 S3 对象存储连接".to_string()))
}

pub async fn save_s3_settings(pool: &SqlitePool, settings: &S3Settings) -> Result<()> {
  info!("保存 S3 配置(default): endpoint={}", settings.endpoint);

  // 注意：移除 bucket 后，save_s3_settings 不再自动验证连接，
  // 因为没有目标 bucket 无法调用 ListObjects 等接口
  // verify_s3_settings(settings).await?;

  debug!("将 S3 配置写入 s3_profiles(default)");
  let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;

  // 如果存在则更新，否则插入
  let existing: Option<(String,)> =
    sqlx::query_as("SELECT profile_name FROM s3_profiles WHERE profile_name = 'default'")
      .fetch_optional(pool)
      .await
      .map_err(|e| RepositoryError::QueryFailed(format!("查询 S3 Profile 失败: {}", e)))?;

  if existing.is_some() {
    sqlx::query(
      "UPDATE s3_profiles SET endpoint = ?, access_key = ?, secret_key = ?, updated_at = ? WHERE profile_name = 'default'",
    )
    .bind(&settings.endpoint)
    .bind(&settings.access_key)
    .bind(&settings.secret_key)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("更新 S3 配置失败: {}", e)))?;
  } else {
    sqlx::query(
      "INSERT INTO s3_profiles (profile_name, endpoint, access_key, secret_key, created_at, updated_at) VALUES ('default', ?, ?, ?, ?, ?)",
    )
    .bind(&settings.endpoint)
    .bind(&settings.access_key)
    .bind(&settings.secret_key)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("保存 S3 配置失败: {}", e)))?;
  }

  info!("S3 配置保存成功 (profile=default)");
  Ok(())
}

pub async fn verify_s3_settings_with_bucket(settings: &S3Settings, bucket: &str) -> Result<()> {
  debug!("开始验证S3连接: endpoint={}, bucket={}", settings.endpoint, bucket);

  match storage::test_s3_connection(&settings.endpoint, &settings.access_key, &settings.secret_key, bucket).await {
    Ok(_) => {
      info!("S3配置验证成功");
      Ok(())
    }
    Err(S3Error::InvalidBaseUrl { url: _ }) => {
      error!("S3 Endpoint地址无效: {}", settings.endpoint);
      Err(RepositoryError::StorageError(
        "S3连接失败: Endpoint 地址无效，请确认格式例如 http://host:9000".to_string(),
      ))
    }
    Err(S3Error::S3Build { reason }) => {
      error!("S3客户端构建失败: endpoint={}, reason={}", settings.endpoint, reason);
      Err(RepositoryError::StorageError(format!(
        "S3连接失败: 无法建立 S3 连接，请检查 Endpoint、Access Key 和 Secret Key。原因: {}",
        reason
      )))
    }
    Err(S3Error::S3ListObjects { bucket, prefix, error }) => {
      let lower = error.to_ascii_lowercase();
      if lower.contains("nosuchbucket") {
        Err(RepositoryError::StorageError(format!(
          "S3连接失败: 桶 {} 不存在或无权限访问，请确认 Bucket 名称",
          bucket
        )))
      } else if lower.contains("accessdenied") || lower.contains("signature") {
        Err(RepositoryError::StorageError(
          "S3连接失败: 访问被拒绝，请确认 Access Key 与 Secret Key 是否正确".to_string(),
        ))
      } else {
        Err(RepositoryError::StorageError(format!(
          "S3连接失败: 无法列举桶 bucket={}, prefix={}, error={}",
          bucket, prefix, error
        )))
      }
    }
    Err(S3Error::S3GetObject { bucket, key, error }) => Err(RepositoryError::StorageError(format!(
      "S3连接失败: 无法读取对象 bucket={}, key={}, error={}",
      bucket, key, error
    ))),
    Err(S3Error::S3ToStream { bucket, key, error }) => Err(RepositoryError::StorageError(format!(
      "S3连接失败: 读取对象流失败 bucket={}, key={}, error={}",
      bucket, key, error
    ))),
    Err(S3Error::Regex { pattern, error }) => Err(RepositoryError::StorageError(format!(
      "无效的对象筛选正则 pattern={}, error={}",
      pattern, error
    ))),
    Err(S3Error::Io { path, error }) => Err(RepositoryError::StorageError(format!(
      "S3连接失败: 网络通信错误 path={}, error={}",
      path, error
    ))),
    Err(S3Error::ConnectionTimeout { bucket, operation }) => Err(RepositoryError::StorageError(format!(
      "S3连接失败: 连接超时 bucket={}, operation={}，请检查网络或 S3 服务状态",
      bucket, operation
    ))),
  }
}

// ============================================================================
// S3 Profiles 管理（支持多个 S3 配置）
// ============================================================================

/// 加载指定 profile 的 S3 配置
pub async fn load_s3_profile(pool: &SqlitePool, profile_name: &str) -> Result<Option<S3Profile>> {
  core_load_s3_profile(pool, profile_name)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("查询 S3 Profile 失败: {}", e)))
}

/// 加载所有 S3 Profiles
pub async fn list_s3_profiles(pool: &SqlitePool) -> Result<Vec<S3Profile>> {
  debug!("加载所有 S3 Profiles");
  core_list_s3_profiles(pool)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("查询 S3 Profiles 失败: {}", e)))
}

/// 保存或更新 S3 Profile
pub async fn save_s3_profile(pool: &SqlitePool, profile: &S3Profile) -> Result<()> {
  info!(
    "保存 S3 Profile: profile={}, endpoint={}",
    profile.profile_name, profile.endpoint
  );

  let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;

  // 检查是否已存在
  let existing = load_s3_profile(pool, &profile.profile_name).await?;

  if existing.is_some() {
    // 更新现有配置
    sqlx::query(
      "UPDATE s3_profiles SET endpoint = ?, access_key = ?, secret_key = ?, updated_at = ? WHERE profile_name = ?",
    )
    .bind(&profile.endpoint)
    .bind(&profile.access_key)
    .bind(&profile.secret_key)
    .bind(now)
    .bind(&profile.profile_name)
    .execute(pool)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("更新 S3 Profile 失败: {}", e)))?;

    info!("S3 Profile 更新成功: {}", profile.profile_name);
  } else {
    // 插入新配置
    sqlx::query(
      "INSERT INTO s3_profiles (profile_name, endpoint, access_key, secret_key, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&profile.profile_name)
    .bind(&profile.endpoint)
    .bind(&profile.access_key)
    .bind(&profile.secret_key)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("保存 S3 Profile 失败: {}", e)))?;

    info!("S3 Profile 创建成功: {}", profile.profile_name);
  }

  Ok(())
}

/// 删除 S3 Profile
pub async fn delete_s3_profile(pool: &SqlitePool, profile_name: &str) -> Result<()> {
  info!("删除 S3 Profile: {}", profile_name);

  // 不允许删除 default profile
  if profile_name == "default" {
    return Err(RepositoryError::StorageError("不能删除 default profile".to_string()));
  }

  sqlx::query("DELETE FROM s3_profiles WHERE profile_name = ?")
    .bind(profile_name)
    .execute(pool)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("删除 S3 Profile 失败: {}", e)))?;

  info!("S3 Profile 删除成功: {}", profile_name);
  Ok(())
}
