use crate::utils::storage::{self, S3Error};
use log::{debug, error, info};
use opsbox_core::{AppError, Result, SqlitePool, run_migration};
use serde::{Deserialize, Serialize};

/// S3 兼容对象存储配置
///
/// 支持所有 S3 兼容的对象存储服务：
/// - AWS S3
/// - MinIO
/// - 阿里云 OSS
/// - 腾讯云 COS
/// - Cloudflare R2
/// - 其他 S3 兼容服务
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct S3Settings {
  pub endpoint: String,
  pub bucket: String,
  pub access_key: String,
  pub secret_key: String,
}

/// S3 配置 Profile
///
/// 每个 Profile 包含完整的 S3 访问配置：Endpoint + Bucket + Credentials
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct S3Profile {
  pub profile_name: String,
  pub endpoint: String,
  pub bucket: String,
  pub access_key: String,
  pub secret_key: String,
}

/// 初始化 LogSeek 模块的数据库表
pub async fn init_schema(db_pool: &SqlitePool) -> Result<()> {
  // 完全采用 profiles 存储（默认 profile_name='default'）
  let schema_sql = r#"
    CREATE TABLE IF NOT EXISTS s3_profiles (
        profile_name TEXT PRIMARY KEY,
        endpoint TEXT NOT NULL,
        bucket TEXT NOT NULL,
        access_key TEXT NOT NULL,
        secret_key TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
  "#;

  run_migration(db_pool, schema_sql, "logseek").await?;

  Ok(())
}

pub async fn load_s3_settings(pool: &SqlitePool) -> Result<Option<S3Settings>> {
  debug!("加载 S3 配置（default profile）");
  let row = sqlx::query_as::<_, (String, String, String, String)>(
    "SELECT endpoint, bucket, access_key, secret_key FROM s3_profiles WHERE profile_name = 'default'",
  )
  .fetch_optional(pool)
  .await
  .map_err(|e| AppError::internal(format!("查询 S3 配置失败: {}", e)))?;

  match &row {
    Some(_) => info!("S3 配置加载成功 (profile=default)"),
    None => info!("S3 配置不存在 (profile=default)"),
  }

  Ok(row.map(|(endpoint, bucket, access_key, secret_key)| S3Settings {
    endpoint,
    bucket,
    access_key,
    secret_key,
  }))
}

pub async fn load_required_s3_settings(pool: &SqlitePool) -> Result<S3Settings> {
  load_s3_settings(pool)
    .await?
    .ok_or_else(|| AppError::not_found("未配置 S3 对象存储连接"))
}

pub async fn save_s3_settings(pool: &SqlitePool, settings: &S3Settings) -> Result<()> {
  info!(
    "保存 S3 配置(default): endpoint={}, bucket={}",
    settings.endpoint, settings.bucket
  );

  debug!("验证 S3 配置有效性");
  verify_s3_settings(settings).await?;

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
      .map_err(|e| AppError::internal(format!("查询 S3 Profile 失败: {}", e)))?;

  if existing.is_some() {
    sqlx::query(
      "UPDATE s3_profiles SET endpoint = ?, bucket = ?, access_key = ?, secret_key = ?, updated_at = ? WHERE profile_name = 'default'",
    )
    .bind(&settings.endpoint)
    .bind(&settings.bucket)
    .bind(&settings.access_key)
    .bind(&settings.secret_key)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| AppError::internal(format!("更新 S3 配置失败: {}", e)))?;
  } else {
    sqlx::query(
      "INSERT INTO s3_profiles (profile_name, endpoint, bucket, access_key, secret_key, created_at, updated_at) VALUES ('default', ?, ?, ?, ?, ?, ?)",
    )
    .bind(&settings.endpoint)
    .bind(&settings.bucket)
    .bind(&settings.access_key)
    .bind(&settings.secret_key)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| AppError::internal(format!("保存 S3 配置失败: {}", e)))?;
  }

  info!("S3 配置保存成功 (profile=default)");
  Ok(())
}

pub async fn verify_s3_settings(settings: &S3Settings) -> Result<()> {
  debug!(
    "开始验证S3连接: endpoint={}, bucket={}",
    settings.endpoint, settings.bucket
  );

  match storage::test_s3_connection(
    &settings.endpoint,
    &settings.access_key,
    &settings.secret_key,
    &settings.bucket,
  )
  .await
  {
    Ok(_) => {
      info!("S3配置验证成功");
      Ok(())
    }
    Err(S3Error::InvalidBaseUrl(_)) => {
      error!("S3 Endpoint地址无效: {}", settings.endpoint);
      Err(AppError::external_service(
        "S3连接失败: Endpoint 地址无效，请确认格式例如 http://host:9000",
      ))
    }
    Err(S3Error::S3Build) => {
      error!("S3客户端构建失败: endpoint={}", settings.endpoint);
      Err(AppError::external_service(
        "S3连接失败: 无法建立 S3 连接，请检查 Endpoint、Access Key 和 Secret Key",
      ))
    }
    Err(S3Error::S3ListObjects(msg)) => {
      let lower = msg.to_ascii_lowercase();
      if lower.contains("nosuchbucket") {
        Err(AppError::external_service(format!(
          "S3连接失败: 桶 {} 不存在或无权限访问，请确认 Bucket 名称",
          settings.bucket
        )))
      } else if lower.contains("accessdenied") || lower.contains("signature") {
        Err(AppError::external_service(
          "S3连接失败: 访问被拒绝，请确认 Access Key 与 Secret Key 是否正确",
        ))
      } else {
        Err(AppError::external_service(format!(
          "S3连接失败: 无法列举桶 {}：{}",
          settings.bucket, msg
        )))
      }
    }
    Err(S3Error::S3GetObject(msg)) => Err(AppError::external_service(format!("S3连接失败: 无法读取对象：{}", msg))),
    Err(S3Error::S3ToStream(msg)) => Err(AppError::external_service(format!(
      "S3连接失败: 读取对象流失败：{}",
      msg
    ))),
    Err(S3Error::Regex(msg)) => Err(AppError::bad_request(format!("无效的对象筛选正则：{}", msg))),
    Err(S3Error::Io(err)) => Err(AppError::external_service(format!("S3连接失败: 网络通信错误：{}", err))),
    Err(S3Error::ConnectionTimeout) => Err(AppError::external_service(
      "S3连接失败: 连接超时，请检查网络或 S3 服务状态",
    )),
  }
}

// ============================================================================
// S3 Profiles 管理（支持多个 S3 配置）
// ============================================================================

/// 加载指定 profile 的 S3 配置
pub async fn load_s3_profile(pool: &SqlitePool, profile_name: &str) -> Result<Option<S3Profile>> {
  debug!("加载 S3 Profile: {}", profile_name);

  let row = sqlx::query_as::<_, (String, String, String, String, String)>(
    "SELECT profile_name, endpoint, bucket, access_key, secret_key FROM s3_profiles WHERE profile_name = ?",
  )
  .bind(profile_name)
  .fetch_optional(pool)
  .await
  .map_err(|e| AppError::internal(format!("查询 S3 Profile 失败: {}", e)))?;

  Ok(
    row.map(|(profile_name, endpoint, bucket, access_key, secret_key)| S3Profile {
      profile_name,
      endpoint,
      bucket,
      access_key,
      secret_key,
    }),
  )
}

/// 加载所有 S3 Profiles
pub async fn list_s3_profiles(pool: &SqlitePool) -> Result<Vec<S3Profile>> {
  debug!("加载所有 S3 Profiles");

  let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
    "SELECT profile_name, endpoint, bucket, access_key, secret_key FROM s3_profiles ORDER BY profile_name",
  )
  .fetch_all(pool)
  .await
  .map_err(|e| AppError::internal(format!("查询 S3 Profiles 失败: {}", e)))?;

  Ok(
    rows
      .into_iter()
      .map(|(profile_name, endpoint, bucket, access_key, secret_key)| S3Profile {
        profile_name,
        endpoint,
        bucket,
        access_key,
        secret_key,
      })
      .collect(),
  )
}

/// 保存或更新 S3 Profile
pub async fn save_s3_profile(pool: &SqlitePool, profile: &S3Profile) -> Result<()> {
  info!(
    "保存 S3 Profile: profile={}, endpoint={}, bucket={}",
    profile.profile_name, profile.endpoint, profile.bucket
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
      "UPDATE s3_profiles SET endpoint = ?, bucket = ?, access_key = ?, secret_key = ?, updated_at = ? WHERE profile_name = ?",
    )
    .bind(&profile.endpoint)
    .bind(&profile.bucket)
    .bind(&profile.access_key)
    .bind(&profile.secret_key)
    .bind(now)
    .bind(&profile.profile_name)
    .execute(pool)
    .await
    .map_err(|e| AppError::internal(format!("更新 S3 Profile 失败: {}", e)))?;

    info!("S3 Profile 更新成功: {}", profile.profile_name);
  } else {
    // 插入新配置
    sqlx::query(
      "INSERT INTO s3_profiles (profile_name, endpoint, bucket, access_key, secret_key, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&profile.profile_name)
    .bind(&profile.endpoint)
    .bind(&profile.bucket)
    .bind(&profile.access_key)
    .bind(&profile.secret_key)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| AppError::internal(format!("保存 S3 Profile 失败: {}", e)))?;

    info!("S3 Profile 创建成功: {}", profile.profile_name);
  }

  Ok(())
}

/// 删除 S3 Profile
pub async fn delete_s3_profile(pool: &SqlitePool, profile_name: &str) -> Result<()> {
  info!("删除 S3 Profile: {}", profile_name);

  // 不允许删除 default profile
  if profile_name == "default" {
    return Err(AppError::bad_request("不能删除 default profile"));
  }

  sqlx::query("DELETE FROM s3_profiles WHERE profile_name = ?")
    .bind(profile_name)
    .execute(pool)
    .await
    .map_err(|e| AppError::internal(format!("删除 S3 Profile 失败: {}", e)))?;

  info!("S3 Profile 删除成功: {}", profile_name);
  Ok(())
}
