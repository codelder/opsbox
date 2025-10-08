//! S3 Profile 管理路由
//! 
//! 处理 /profiles 端点，管理多个 S3 配置

use axum::{
  extract::{Json, Path, State},
  http::StatusCode,
};
use crate::api::models::{AppError, S3ProfileListResponse, S3ProfilePayload};
use crate::repository::settings;
use opsbox_core::SqlitePool;
use problemdetails::Problem;

/// 列出所有 S3 Profiles
pub async fn list_profiles(State(pool): State<SqlitePool>) -> Result<Json<S3ProfileListResponse>, Problem> {
  let profiles = settings::list_s3_profiles(&pool)
    .await
    .map_err(AppError::Settings)?;
  
  let payload_list: Vec<S3ProfilePayload> = profiles.into_iter().map(Into::into).collect();
  
  Ok(Json(S3ProfileListResponse {
    profiles: payload_list,
  }))
}

/// 保存或更新 S3 Profile
pub async fn save_profile(
  State(pool): State<SqlitePool>,
  Json(payload): Json<S3ProfilePayload>,
) -> Result<StatusCode, Problem> {
  let profile: settings::S3Profile = payload.into();
  settings::save_s3_profile(&pool, &profile)
    .await
    .map_err(AppError::Settings)?;
  Ok(StatusCode::NO_CONTENT)
}

/// 删除 S3 Profile
pub async fn delete_profile(
  State(pool): State<SqlitePool>,
  Path(name): Path<String>,
) -> Result<StatusCode, Problem> {
  settings::delete_s3_profile(&pool, &name)
    .await
    .map_err(AppError::Settings)?;
  Ok(StatusCode::NO_CONTENT)
}
