//! S3 设置路由
//!
//! 处理 /settings/s3 端点，管理 S3 存储配置

use crate::api::models::{AppError, S3SettingsPayload};
use crate::repository::settings;
use axum::{
  extract::{Json, Query, State},
  http::StatusCode,
};
use opsbox_core::SqlitePool;
use problemdetails::Problem;
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
) -> Result<Json<S3SettingsPayload>, Problem> {
  let settings_opt = settings::load_s3_settings(&pool).await.map_err(AppError::Settings)?;

  // 基础负载：仅反映“是否已配置”而非“是否连通”
  let mut payload = settings_opt.clone().map_or_else(S3SettingsPayload::default, Into::into);
  payload.configured = settings_opt.is_some();

  // 可选：按需验证连接（仅在显式请求 verify=true 时）
  if let (true, Some(settings_value)) = (q.verify.unwrap_or(false), settings_opt.as_ref())
    && let Err(e) = settings::verify_s3_settings(settings_value).await
  {
    payload.connection_error = Some(format!("无法连接对象存储：{}", e));
  }

  Ok(Json(payload))
}

pub async fn save_s3_settings(
  State(pool): State<SqlitePool>,
  Json(payload): Json<S3SettingsPayload>,
) -> Result<StatusCode, Problem> {
  let settings: settings::S3Settings = payload.into();
  settings::save_s3_settings(&pool, &settings)
    .await
    .map_err(AppError::Settings)?;
  Ok(StatusCode::NO_CONTENT)
}
