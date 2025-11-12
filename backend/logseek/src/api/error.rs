use axum::extract::rejection::JsonRejection;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use thiserror::Error;

/// API 层错误类型（LogSeek 模块专用）
///
/// 聚合来自各层的错误，并转换为标准的 HTTP 响应
#[derive(Debug, Error)]
pub enum LogSeekApiError {
  /// Service 层错误
  #[error(transparent)]
  Service(#[from] crate::service::ServiceError),

  /// Repository 层错误
  #[error(transparent)]
  Repository(#[from] crate::repository::RepositoryError),

  /// Domain 层错误
  #[error(transparent)]
  Domain(#[from] crate::domain::FileUrlError),

  /// JSON 解析失败
  #[error("JSON 解析失败: {0}")]
  BadJson(#[from] JsonRejection),

  /// 查询语法错误
  #[error("查询语法错误: {0}")]
  QueryParse(#[from] crate::query::ParseError),

  /// 存储层错误
  #[error("存储错误: {0}")]
  StorageError(#[from] crate::utils::storage::S3Error),

  /// 核心服务错误
  #[error(transparent)]
  Internal(#[from] opsbox_core::AppError),
}

impl IntoResponse for LogSeekApiError {
  fn into_response(self) -> Response {
    let (status, title, detail) = match &self {
      // Service 层错误映射
      LogSeekApiError::Service(e) => match e {
        crate::service::ServiceError::SearchFailed { .. } => {
          (StatusCode::INTERNAL_SERVER_ERROR, "搜索失败", e.to_string())
        }
        crate::service::ServiceError::ConfigError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "配置错误", e.to_string()),
        crate::service::ServiceError::ProcessingError(_) => {
          (StatusCode::INTERNAL_SERVER_ERROR, "数据处理失败", e.to_string())
        }
        crate::service::ServiceError::IoError { .. } => {
          (StatusCode::INTERNAL_SERVER_ERROR, "IO 操作失败", e.to_string())
        }
        crate::service::ServiceError::ChannelClosed => (
          StatusCode::INTERNAL_SERVER_ERROR,
          "通信中断",
          "数据通道已关闭".to_string(),
        ),
        crate::service::ServiceError::Repository(repo_err) => {
          // 递归处理 Repository 错误
          let repo_api_err: LogSeekApiError = repo_err.clone().into();
          return repo_api_err.into_response();
        }
      },

      // Repository 层错误映射
      LogSeekApiError::Repository(e) => match e {
        crate::repository::RepositoryError::NotFound(_) => (StatusCode::NOT_FOUND, "资源不存在", e.to_string()),
        crate::repository::RepositoryError::StorageError(_) => (StatusCode::BAD_GATEWAY, "存储服务错误", e.to_string()),
        crate::repository::RepositoryError::Database(_) => {
          (StatusCode::INTERNAL_SERVER_ERROR, "数据库错误", e.to_string())
        }
        crate::repository::RepositoryError::QueryFailed(_) => {
          (StatusCode::INTERNAL_SERVER_ERROR, "查询失败", e.to_string())
        }
        crate::repository::RepositoryError::CacheFailed(_) => {
          (StatusCode::INTERNAL_SERVER_ERROR, "缓存操作失败", e.to_string())
        }
      },

      // Domain 层错误映射
      LogSeekApiError::Domain(_) => (StatusCode::BAD_REQUEST, "业务验证失败", self.to_string()),

      // 协议级错误
      LogSeekApiError::BadJson(_) => (StatusCode::BAD_REQUEST, "JSON 请求格式错误", self.to_string()),
      LogSeekApiError::QueryParse(_) => (StatusCode::BAD_REQUEST, "查询语法错误", self.to_string()),

      // 存储错误
      LogSeekApiError::StorageError(_) => (StatusCode::BAD_GATEWAY, "存储服务错误", self.to_string()),

      // 核心服务错误
      LogSeekApiError::Internal(core_err) => {
        let status = match core_err {
          opsbox_core::AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
          opsbox_core::AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
          opsbox_core::AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
          opsbox_core::AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
          opsbox_core::AppError::NotFound(_) => StatusCode::NOT_FOUND,
          opsbox_core::AppError::ExternalService(_) => StatusCode::BAD_GATEWAY,
        };
        (status, "内部错误", core_err.to_string())
      }
    };

    // 记录错误日志
    log::error!("[LogSeek API] [{}] {}", title, detail);

    // 构建简单 JSON 响应
    let json_body = serde_json::json!({
      "type": "about:blank",
      "title": title,
      "detail": detail,
      "status": status.as_u16(),
    });
    let json_str = serde_json::to_string(&json_body)
      .unwrap_or_else(|_| r#"{"type":"about:blank","title":"Internal Server Error","status":500}"#.to_string());

    let mut response = Response::new(axum::body::Body::from(json_str));
    *response.status_mut() = status;
    response.headers_mut().insert(
      header::CONTENT_TYPE,
      header::HeaderValue::from_static("application/problem+json; charset=utf-8"),
    );
    response
  }
}

/// API 层 Result 类型别名
pub type Result<T> = std::result::Result<T, LogSeekApiError>;
