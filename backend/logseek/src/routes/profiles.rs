//! S3 Profile 管理路由
//!
//! 处理 /profiles 端点，管理多个 S3 配置

use crate::api::LogSeekApiError;
use crate::api::models::{S3ProfileListResponse, S3ProfilePayload};
use crate::repository::s3;
use axum::{
  extract::{Json, Path, State},
  http::StatusCode,
};
use opsbox_core::SqlitePool;

/// 列出所有 S3 Profiles
pub async fn list_profiles(State(pool): State<SqlitePool>) -> Result<Json<S3ProfileListResponse>, LogSeekApiError> {
  let profiles = s3::list_s3_profiles(&pool).await?;

  let payload_list: Vec<S3ProfilePayload> = profiles.into_iter().map(Into::into).collect();

  Ok(Json(S3ProfileListResponse { profiles: payload_list }))
}

/// 保存或更新 S3 Profile
pub async fn save_profile(
  State(pool): State<SqlitePool>,
  Json(payload): Json<S3ProfilePayload>,
) -> Result<StatusCode, LogSeekApiError> {
  // 将前端负载转换为内部 Profile 结构
  let profile: s3::S3Profile = payload.into();

  // 注意：从 Profile 中移除 bucket 后，无法在保存时验证连接，
  // 因为没有目标 bucket。连接验证现在将在使用具体存储桶的场景中进行。
  // s3::verify_s3_settings(&verify_target).await?;

  // 验证通过后再保存/更新 Profile
  s3::save_s3_profile(&pool, &profile).await?;

  Ok(StatusCode::NO_CONTENT)
}

/// 删除 S3 Profile
pub async fn delete_profile(
  State(pool): State<SqlitePool>,
  Path(name): Path<String>,
) -> Result<StatusCode, LogSeekApiError> {
  s3::delete_s3_profile(&pool, &name).await?;
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
  async fn test_profile_routes() {
    let pool = setup_test_db().await;

    // 1. Save a profile
    let payload = S3ProfilePayload {
      profile_name: "test-profile".to_string(),
      endpoint: "http://minio:9000".to_string(),
      access_key: "ak".to_string(),
      secret_key: "sk".to_string(),
    };
    save_profile(State(pool.clone()), Json(payload.clone())).await.unwrap();

    // 2. List profiles
    let resp = list_profiles(State(pool.clone())).await.unwrap();
    assert_eq!(resp.profiles.len(), 1);
    assert_eq!(resp.profiles[0].profile_name, "test-profile");

    // 3. Delete profile
    delete_profile(State(pool.clone()), Path("test-profile".to_string()))
      .await
      .unwrap();
    let resp = list_profiles(State(pool.clone())).await.unwrap();
    assert_eq!(resp.profiles.len(), 0);
  }
}
