//! S3 Profile 管理路由
//!
//! 处理 /profiles 端点，管理多个 S3 配置

use crate::api::models::{AppError, S3ProfileListResponse, S3ProfilePayload};
use crate::repository::settings;
use axum::{
  extract::{Json, Path, State},
  http::StatusCode,
};
use opsbox_core::SqlitePool;
use problemdetails::Problem;

/// 列出所有 S3 Profiles
pub async fn list_profiles(State(pool): State<SqlitePool>) -> Result<Json<S3ProfileListResponse>, Problem> {
  let profiles = settings::list_s3_profiles(&pool).await.map_err(AppError::Settings)?;

  let payload_list: Vec<S3ProfilePayload> = profiles.into_iter().map(Into::into).collect();

  Ok(Json(S3ProfileListResponse { profiles: payload_list }))
}

/// 保存或更新 S3 Profile
pub async fn save_profile(
  State(pool): State<SqlitePool>,
  Json(payload): Json<S3ProfilePayload>,
) -> Result<StatusCode, Problem> {
  // 将前端负载转换为内部 Profile 结构
  let profile: settings::S3Profile = payload.into();

  // 在持久化前先验证 S3 连接可用性（防止保存不可用配置）
  // 校验内容：Endpoint、Bucket、Access Key、Secret Key
  let verify_target = settings::S3Settings {
    endpoint: profile.endpoint.clone(),
    bucket: profile.bucket.clone(),
    access_key: profile.access_key.clone(),
    secret_key: profile.secret_key.clone(),
  };
  settings::verify_s3_settings(&verify_target)
    .await
    .map_err(AppError::Settings)?;

  // 验证通过后再保存/更新 Profile
  settings::save_s3_profile(&pool, &profile)
    .await
    .map_err(AppError::Settings)?;

  Ok(StatusCode::NO_CONTENT)
}

/// 删除 S3 Profile
pub async fn delete_profile(State(pool): State<SqlitePool>, Path(name): Path<String>) -> Result<StatusCode, Problem> {
  settings::delete_s3_profile(&pool, &name)
    .await
    .map_err(AppError::Settings)?;
  Ok(StatusCode::NO_CONTENT)
}
