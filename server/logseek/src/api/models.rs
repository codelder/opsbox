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

/// S3 兼容对象存储设置请求/响应（支持 AWS S3、MinIO、阿里云 OSS 等）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct S3SettingsPayload {
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

impl From<S3SettingsPayload> for settings::S3Settings {
  fn from(value: S3SettingsPayload) -> Self {
    Self {
      endpoint: value.endpoint,
      bucket: value.bucket,
      access_key: value.access_key,
      secret_key: value.secret_key,
    }
  }
}

impl From<settings::S3Settings> for S3SettingsPayload {
  fn from(value: settings::S3Settings) -> Self {
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

/// S3 Profile 负载（用于 POST 请求）
///
/// 每个 Profile 包含完整的 S3 访问配置：Endpoint + Bucket + Credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3ProfilePayload {
  pub profile_name: String,
  pub endpoint: String,
  pub bucket: String,
  pub access_key: String,
  pub secret_key: String,
}

impl From<S3ProfilePayload> for settings::S3Profile {
  fn from(value: S3ProfilePayload) -> Self {
    Self {
      profile_name: value.profile_name,
      endpoint: value.endpoint,
      bucket: value.bucket,
      access_key: value.access_key,
      secret_key: value.secret_key,
    }
  }
}

impl From<settings::S3Profile> for S3ProfilePayload {
  fn from(value: settings::S3Profile) -> Self {
    Self {
      profile_name: value.profile_name,
      endpoint: value.endpoint,
      bucket: value.bucket,
      access_key: value.access_key,
      secret_key: value.secret_key,
    }
  }
}

/// S3 Profile 列表响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3ProfileListResponse {
  pub profiles: Vec<S3ProfilePayload>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_search_body_deserialization() {
    let json = r#"{"q":"error","context":5}"#;
    let body: SearchBody = serde_json::from_str(json).unwrap();
    assert_eq!(body.q, "error");
    assert_eq!(body.context, Some(5));
  }

  #[test]
  fn test_search_body_optional_context() {
    let json = r#"{"q":"warn"}"#;
    let body: SearchBody = serde_json::from_str(json).unwrap();
    assert_eq!(body.q, "warn");
    assert_eq!(body.context, None);
  }

  #[test]
  fn test_nl2q_out_serialization() {
    let out = NL2QOut {
      q: "error OR warning".to_string(),
    };
    let json = serde_json::to_string(&out).unwrap();
    assert!(json.contains("error OR warning"));
  }

  #[test]
  fn test_s3_settings_payload_serialization() {
    let payload = S3SettingsPayload {
      endpoint: "localhost:9000".to_string(),
      bucket: "logs".to_string(),
      access_key: "minioadmin".to_string(),
      secret_key: "minioadmin".to_string(),
      configured: true,
      connection_error: None,
    };

    let json = serde_json::to_string(&payload).unwrap();
    let deserialized: S3SettingsPayload = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.endpoint, "localhost:9000");
    assert_eq!(deserialized.bucket, "logs");
    assert!(deserialized.configured);
  }

  #[test]
  fn test_s3_settings_payload_deserialization_with_defaults() {
    let json = r#"{}"#;
    let payload: S3SettingsPayload = serde_json::from_str(json).unwrap();

    assert_eq!(payload.endpoint, "");
    assert_eq!(payload.bucket, "");
    assert!(!payload.configured);
    assert_eq!(payload.connection_error, None);
  }

  #[test]
  fn test_s3_settings_payload_with_connection_error() {
    let json = r#"{"endpoint":"localhost:9000","connection_error":"Connection timeout"}"#;
    let payload: S3SettingsPayload = serde_json::from_str(json).unwrap();

    assert_eq!(payload.endpoint, "localhost:9000");
    assert_eq!(payload.connection_error, Some("Connection timeout".to_string()));
  }

  #[test]
  fn test_s3_settings_conversion_to_domain() {
    let payload = S3SettingsPayload {
      endpoint: "localhost:9000".to_string(),
      bucket: "logs".to_string(),
      access_key: "admin".to_string(),
      secret_key: "password".to_string(),
      configured: true,
      connection_error: None,
    };

    let settings: settings::S3Settings = payload.into();

    assert_eq!(settings.endpoint, "localhost:9000");
    assert_eq!(settings.bucket, "logs");
    assert_eq!(settings.access_key, "admin");
    assert_eq!(settings.secret_key, "password");
  }

  #[test]
  fn test_s3_settings_conversion_from_domain() {
    let settings = settings::S3Settings {
      endpoint: "localhost:9000".to_string(),
      bucket: "logs".to_string(),
      access_key: "admin".to_string(),
      secret_key: "password".to_string(),
    };

    let payload: S3SettingsPayload = settings.into();

    assert_eq!(payload.endpoint, "localhost:9000");
    assert_eq!(payload.bucket, "logs");
    assert!(!payload.configured); // 默认值
    assert_eq!(payload.connection_error, None); // 默认值
  }

  #[test]
  fn test_view_params_deserialization() {
    let json = r#"{"sid":"test-session","file":"test.log","start":10,"end":20}"#;
    let params: ViewParams = serde_json::from_str(json).unwrap();

    assert_eq!(params.sid, "test-session");
    assert_eq!(params.file, "test.log");
    assert_eq!(params.start, Some(10));
    assert_eq!(params.end, Some(20));
  }

  #[test]
  fn test_view_params_optional_fields() {
    let json = r#"{"sid":"test-session","file":"test.log"}"#;
    let params: ViewParams = serde_json::from_str(json).unwrap();

    assert_eq!(params.sid, "test-session");
    assert_eq!(params.file, "test.log");
    assert_eq!(params.start, None);
    assert_eq!(params.end, None);
  }

  #[test]
  fn test_s3_settings_default() {
    let payload = S3SettingsPayload::default();

    assert_eq!(payload.endpoint, "");
    assert_eq!(payload.bucket, "");
    assert_eq!(payload.access_key, "");
    assert_eq!(payload.secret_key, "");
    assert!(!payload.configured);
    assert_eq!(payload.connection_error, None);
  }

  #[test]
  fn test_app_error_display() {
    use crate::query;
    let error = AppError::QueryParse(query::ParseError::UnexpectedToken { span: (0, 1) });

    let error_string = error.to_string();
    assert!(error_string.contains("查询语法错误"));
  }

  #[test]
  fn test_app_error_to_problem() {
    use crate::query;
    let error = AppError::QueryParse(query::ParseError::InvalidRegex {
      message: "invalid syntax".to_string(),
      span: (0, 5),
    });

    let problem: Problem = error.into();
    assert_eq!(problem.status_code, StatusCode::BAD_REQUEST);
  }
}
