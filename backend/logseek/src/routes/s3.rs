//! S3 设置路由
//!
//! 处理 /settings/s3 端点，管理 S3 存储配置

use crate::api::LogSeekApiError;
use crate::api::models::S3SettingsPayload;
use crate::repository::s3;
use axum::{
  extract::{Json, Query, State},
  http::StatusCode,
};
use opsbox_core::SqlitePool;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct S3Query {
  /// 是否在返回设置时验证连接（默认 false，不进行外部连接）
  pub verify: Option<bool>,
}

/// 获取 S3 设置
pub async fn get_s3_settings(
  State(pool): State<SqlitePool>,
  Query(q): Query<S3Query>,
) -> Result<Json<S3SettingsPayload>, LogSeekApiError> {
  let settings_opt = s3::load_s3_settings(&pool).await?;

  // 基础负载：仅反映“是否已配置”而非“是否连通”
  let mut payload = settings_opt.clone().map_or_else(S3SettingsPayload::default, Into::into);
  payload.configured = settings_opt.is_some();

  // 可选：按需验证连接（仅在显式请求 verify=true 时）
  if let (true, Some(_settings_value)) = (q.verify.unwrap_or(false), settings_opt.as_ref()) {
    // 由于 Profile 不再包含 bucket，如果需要验证连接，目前只能在有具体业务请求时进行。
    // 这里暂时跳过或者如果以后有默认测试桶再加回来。
    // payload.connection_error = Some("无法在没有存储桶的情况下验证连接".to_string());
  }

  Ok(Json(payload))
}

pub async fn save_s3_settings(
  State(pool): State<SqlitePool>,
  Json(payload): Json<S3SettingsPayload>,
) -> Result<StatusCode, LogSeekApiError> {
  let settings: s3::S3Settings = payload.into();
  s3::save_s3_settings(&pool, &settings).await?;
  Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::s3::init_schema;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        init_schema(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn test_s3_settings_routes() {
        let pool = setup_test_db().await;

        // 1. Initial get - should be default/configured=false
        let resp = get_s3_settings(State(pool.clone()), Query(S3Query { verify: None })).await.unwrap();
        assert!(!resp.configured);

        // 2. Save settings
        let payload = S3SettingsPayload {
            endpoint: "http://minio:9000".to_string(),
            access_key: "ak".to_string(),
            secret_key: "sk".to_string(),
            configured: true,
            connection_error: None,
        };
        save_s3_settings(State(pool.clone()), Json(payload.clone())).await.unwrap();

        // 3. Get again - should be configured=true
        let resp = get_s3_settings(State(pool.clone()), Query(S3Query { verify: None })).await.unwrap();
        assert!(resp.configured);
        assert_eq!(resp.endpoint, "http://minio:9000");
    }
}
