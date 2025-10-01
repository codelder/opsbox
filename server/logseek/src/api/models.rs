// API 层数据模型
use crate::repository::settings;
use crate::service::search::SearchError;
use crate::utils::storage::StorageError;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use problemdetails::Problem;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// API 层错误类型
#[derive(Debug, Error)]
pub enum AppError {
  #[error("存储错误")]
  StorageError(StorageError),
  #[error("检索错误")]
  SearchError(SearchError),
  #[error(transparent)]
  BadJson(#[from] JsonRejection),
  #[error("查询语法错误")]
  QueryParse(#[from] crate::query::ParseError),
  #[error("设置存储错误")]
  Settings(#[from] opsbox_core::AppError),
}

impl From<AppError> for Problem {
  fn from(error: AppError) -> Self {
    match error {
      AppError::StorageError(e) => problemdetails::new(StatusCode::INTERNAL_SERVER_ERROR)
        .with_title("存储错误")
        .with_detail(e.to_string()),
      AppError::SearchError(e) => problemdetails::new(StatusCode::INTERNAL_SERVER_ERROR)
        .with_title("检索错误")
        .with_detail(e.to_string()),
      AppError::BadJson(e) => problemdetails::new(StatusCode::BAD_REQUEST)
        .with_title("JSON请求错误")
        .with_detail(e.to_string()),
      AppError::QueryParse(e) => problemdetails::new(StatusCode::BAD_REQUEST)
        .with_title("查询语法错误")
        .with_detail(e.to_string()),
      AppError::Settings(e) => {
        let status = match e {
          opsbox_core::AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
          opsbox_core::AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
          opsbox_core::AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
          opsbox_core::AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
          opsbox_core::AppError::NotFound(_) => StatusCode::NOT_FOUND,
          opsbox_core::AppError::ExternalService(_) => StatusCode::BAD_GATEWAY,
        };
        problemdetails::new(status)
          .with_title(e.to_string())
          .with_detail(e.to_string())
      }
    }
  }
}

/// 搜索请求体
#[derive(Debug, Clone, Deserialize)]
pub struct SearchBody {
  pub q: String,
  pub context: Option<usize>,
}

/// NL2Q 响应
#[derive(Debug, Clone, Serialize)]
pub struct NL2QOut {
  pub q: String,
}

/// MinIO 设置请求/响应
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MinioSettingsPayload {
  #[serde(default)]
  pub endpoint: String,
  #[serde(default)]
  pub bucket: String,
  #[serde(default)]
  pub access_key: String,
  #[serde(default)]
  pub secret_key: String,
  #[serde(default)]
  pub configured: bool,
  #[serde(default)]
  pub connection_error: Option<String>,
}

impl From<MinioSettingsPayload> for settings::MinioSettings {
  fn from(value: MinioSettingsPayload) -> Self {
    Self {
      endpoint: value.endpoint,
      bucket: value.bucket,
      access_key: value.access_key,
      secret_key: value.secret_key,
    }
  }
}

impl From<settings::MinioSettings> for MinioSettingsPayload {
  fn from(value: settings::MinioSettings) -> Self {
    Self {
      endpoint: value.endpoint,
      bucket: value.bucket,
      access_key: value.access_key,
      secret_key: value.secret_key,
      configured: false,
      connection_error: None,
    }
  }
}

/// 查看缓存参数
#[derive(Debug, Clone, Deserialize)]
pub struct ViewParams {
  pub sid: String,
  pub file: String,
  pub start: Option<usize>,
  pub end: Option<usize>,
}
