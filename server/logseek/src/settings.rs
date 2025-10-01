use crate::storage::{self, StorageError};
use opsbox_core::{Result, AppError, SqlitePool, run_migration};
use serde::{Deserialize, Serialize};
use log::{debug, info, error};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MinioSettings {
  pub endpoint: String,
  pub bucket: String,
  pub access_key: String,
  pub secret_key: String,
}

/// 初始化 LogSeek 模块的数据库表
pub async fn init_schema(db_pool: &SqlitePool) -> Result<()> {
  let schema_sql = r#"
    -- LogSeek MinIO 配置表
    CREATE TABLE IF NOT EXISTS logseek_minio_config (
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

pub async fn load_minio_settings(pool: &SqlitePool) -> Result<Option<MinioSettings>> {
  debug!("加载MinIO配置");
  let row = sqlx::query_as::<_, (String, String, String, String)>(
    "SELECT endpoint, bucket, access_key, secret_key FROM logseek_minio_config WHERE id = 1",
  )
  .fetch_optional(pool)
  .await
  .map_err(|e| AppError::internal(format!("查询MinIO配置失败: {}", e)))?;
  
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

pub async fn load_required_minio_settings(pool: &SqlitePool) -> Result<MinioSettings> {
  load_minio_settings(pool)
    .await?
    .ok_or_else(|| AppError::not_found("未配置 MinIO 连接"))
}


pub async fn save_minio_settings(pool: &SqlitePool, settings: &MinioSettings) -> Result<()> {
  info!("保存MinIO配置: endpoint={}, bucket={}", settings.endpoint, settings.bucket);
  
  debug!("验证MinIO配置有效性");
  verify_minio_settings(settings).await?;
  
  debug!("将MinIO配置写入数据库");
  let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;
  
  sqlx::query(
    "INSERT INTO logseek_minio_config (id, endpoint, bucket, access_key, secret_key, updated_at)
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
  .map_err(|e| AppError::internal(format!("保存MinIO配置失败: {}", e)))?;
  
  info!("MinIO配置保存成功");
  Ok(())
}

pub async fn verify_minio_settings(settings: &MinioSettings) -> Result<()> {
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
      Err(AppError::external_service(
        "MinIO连接失败: Endpoint 地址无效，请确认格式例如 http://host:9000",
      ))
    },
    Err(StorageError::MinioBuild) => {
      error!("MinIO客户端构建失败: endpoint={}", settings.endpoint);
      Err(AppError::external_service(
        "MinIO连接失败: 无法建立 MinIO 连接，请检查 Endpoint、Access Key 和 Secret Key",
      ))
    },
    Err(StorageError::MinioListObjects(msg)) => {
      let lower = msg.to_ascii_lowercase();
      if lower.contains("nosuchbucket") {
        Err(AppError::external_service(
          format!("MinIO连接失败: 桶 {} 不存在或无权限访问，请确认 Bucket 名称", settings.bucket),
        ))
      } else if lower.contains("accessdenied") || lower.contains("signature") {
        Err(AppError::external_service(
          "MinIO连接失败: 访问被拒绝，请确认 Access Key 与 Secret Key 是否正确",
        ))
      } else {
        Err(AppError::external_service(
          format!("MinIO连接失败: 无法列举桶 {}：{}", settings.bucket, msg),
        ))
      }
    }
    Err(StorageError::MinioGetObject(msg)) => Err(AppError::external_service(
      format!("MinIO连接失败: 无法读取对象：{}", msg),
    )),
    Err(StorageError::MinioToStream(msg)) => Err(AppError::external_service(
      format!("MinIO连接失败: 读取对象流失败：{}", msg),
    )),
    Err(StorageError::Regex(msg)) => Err(AppError::bad_request(
      format!("无效的对象筛选正则：{}", msg),
    )),
    Err(StorageError::Io(err)) => Err(AppError::external_service(
      format!("MinIO连接失败: 网络通信错误：{}", err),
    )),
    Err(StorageError::ConnectionTimeout) => Err(AppError::external_service(
      "MinIO连接失败: 连接超时，请检查网络或 MinIO 服务状态",
    )),
  }
}
