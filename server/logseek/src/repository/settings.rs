use crate::utils::storage::{self, StorageError};
use log::{debug, error, info};
use opsbox_core::{AppError, Result, SqlitePool, run_migration};
use serde::{Deserialize, Serialize};

/// S3 兼容对象存储配置
///
/// 支持所有 S3 兼容的对象存储服务：
/// - AWS S3
/// - S3
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

/// 初始化 LogSeek 模块的数据库表
pub async fn init_schema(db_pool: &SqlitePool) -> Result<()> {
  let schema_sql = r#"
    -- LogSeek S3 兼容对象存储配置表
    CREATE TABLE IF NOT EXISTS logseek_s3_config (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        endpoint TEXT NOT NULL,
        bucket TEXT NOT NULL,
        access_key TEXT NOT NULL,
        secret_key TEXT NOT NULL,
        updated_at INTEGER NOT NULL
    );

    -- LogSeek 通用设置表
    CREATE TABLE IF NOT EXISTS logseek_settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        updated_at INTEGER NOT NULL
    );
  "#;

  run_migration(db_pool, schema_sql, "logseek").await
}

pub async fn load_s3_settings(pool: &SqlitePool) -> Result<Option<S3Settings>> {
  debug!("加载 S3 配置");
  let row = sqlx::query_as::<_, (String, String, String, String)>(
    "SELECT endpoint, bucket, access_key, secret_key FROM logseek_s3_config WHERE id = 1",
  )
  .fetch_optional(pool)
  .await
  .map_err(|e| AppError::internal(format!("查询 S3 配置失败: {}", e)))?;

  match &row {
    Some(_) => info!("S3 配置加载成功"),
    None => info!("S3 配置不存在"),
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
    "保存 S3 配置: endpoint={}, bucket={}",
    settings.endpoint, settings.bucket
  );

  debug!("验证 S3 配置有效性");
  verify_s3_settings(settings).await?;

  debug!("将 S3 配置写入数据库");
  let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;

  sqlx::query(
    "INSERT INTO logseek_s3_config (id, endpoint, bucket, access_key, secret_key, updated_at)
     VALUES (1, ?, ?, ?, ?, ?)
     ON CONFLICT(id) DO UPDATE SET 
       endpoint = excluded.endpoint, 
       bucket = excluded.bucket,
       access_key = excluded.access_key, 
       secret_key = excluded.secret_key,
       updated_at = excluded.updated_at",
  )
  .bind(&settings.endpoint)
  .bind(&settings.bucket)
  .bind(&settings.access_key)
  .bind(&settings.secret_key)
  .bind(now)
  .execute(pool)
  .await
  .map_err(|e| AppError::internal(format!("保存 S3 配置失败: {}", e)))?;

  info!("S3 配置保存成功");
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
    Err(StorageError::InvalidBaseUrl(_)) => {
      error!("S3 Endpoint地址无效: {}", settings.endpoint);
      Err(AppError::external_service(
        "S3连接失败: Endpoint 地址无效，请确认格式例如 http://host:9000",
      ))
    }
    Err(StorageError::S3Build) => {
      error!("S3客户端构建失败: endpoint={}", settings.endpoint);
      Err(AppError::external_service(
        "S3连接失败: 无法建立 S3 连接，请检查 Endpoint、Access Key 和 Secret Key",
      ))
    }
    Err(StorageError::S3ListObjects(msg)) => {
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
    Err(StorageError::S3GetObject(msg)) => {
      Err(AppError::external_service(format!("S3连接失败: 无法读取对象：{}", msg)))
    }
    Err(StorageError::S3ToStream(msg)) => Err(AppError::external_service(format!(
      "S3连接失败: 读取对象流失败：{}",
      msg
    ))),
    Err(StorageError::Regex(msg)) => Err(AppError::bad_request(format!("无效的对象筛选正则：{}", msg))),
    Err(StorageError::Io(err)) => Err(AppError::external_service(format!("S3连接失败: 网络通信错误：{}", err))),
    Err(StorageError::ConnectionTimeout) => Err(AppError::external_service(
      "S3连接失败: 连接超时，请检查网络或 S3 服务状态",
    )),
  }
}
