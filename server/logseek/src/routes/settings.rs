//! S3 设置路由
//!
//! 处理 /settings/s3 端点，管理 S3 存储配置

use crate::api::models::{AppError, S3SettingsPayload};
use crate::repository::settings;
use axum::{
  extract::{Json, State},
  http::StatusCode,
};
use opsbox_core::SqlitePool;
use problemdetails::Problem;

/// 获取 S3 设置
pub async fn get_s3_settings(State(pool): State<SqlitePool>) -> Result<Json<S3SettingsPayload>, Problem> {
  let settings_opt = settings::load_s3_settings(&pool).await.map_err(AppError::Settings)?;
  let mut payload = settings_opt.clone().map_or_else(S3SettingsPayload::default, Into::into);

  if let Some(settings_value) = settings_opt {
    match settings::verify_s3_settings(&settings_value).await {
      Ok(_) => {
        payload.configured = true;
      }
      Err(e) => {
        payload.configured = false;
        payload.connection_error = Some(format!("无法连接 MinIO：{}", e));
      }
    }
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
