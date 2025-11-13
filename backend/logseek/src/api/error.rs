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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    /// 辅助函数：从 Response 中提取 JSON body
    async fn extract_json_body(response: Response) -> serde_json::Value {
        let (_parts, body) = response.into_parts();
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn test_service_error_config_error_conversion() {
        let service_err = crate::service::ServiceError::ConfigError("配置文件缺失".to_string());
        let api_err: LogSeekApiError = service_err.into();
        let response = api_err.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let json = extract_json_body(response).await;
        assert_eq!(json["title"], "配置错误");
        assert_eq!(json["status"], 500);
        assert!(json["detail"].as_str().unwrap().contains("配置文件缺失"));
    }

    #[tokio::test]
    async fn test_service_error_processing_error_conversion() {
        let service_err = crate::service::ServiceError::ProcessingError("数据解析失败".to_string());
        let api_err: LogSeekApiError = service_err.into();
        let response = api_err.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let json = extract_json_body(response).await;
        assert_eq!(json["title"], "数据处理失败");
        assert_eq!(json["status"], 500);
        assert!(json["detail"].as_str().unwrap().contains("数据解析失败"));
    }

    #[tokio::test]
    async fn test_service_error_search_failed_conversion() {
        let service_err = crate::service::ServiceError::SearchFailed {
            path: "/test/path".to_string(),
            error: "连接超时".to_string(),
        };
        let api_err: LogSeekApiError = service_err.into();
        let response = api_err.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let json = extract_json_body(response).await;
        assert_eq!(json["title"], "搜索失败");
        assert_eq!(json["status"], 500);
        let detail = json["detail"].as_str().unwrap();
        assert!(detail.contains("/test/path"));
        assert!(detail.contains("连接超时"));
    }

    #[tokio::test]
    async fn test_service_error_io_error_conversion() {
        let service_err = crate::service::ServiceError::IoError {
            path: "/test/file.log".to_string(),
            error: "文件不存在".to_string(),
        };
        let api_err: LogSeekApiError = service_err.into();
        let response = api_err.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let json = extract_json_body(response).await;
        assert_eq!(json["title"], "IO 操作失败");
        assert_eq!(json["status"], 500);
    }

    #[tokio::test]
    async fn test_service_error_channel_closed_conversion() {
        let service_err = crate::service::ServiceError::ChannelClosed;
        let api_err: LogSeekApiError = service_err.into();
        let response = api_err.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let json = extract_json_body(response).await;
        assert_eq!(json["title"], "通信中断");
        assert_eq!(json["status"], 500);
        assert_eq!(json["detail"], "数据通道已关闭");
    }

    #[tokio::test]
    async fn test_repository_error_not_found_conversion() {
        let repo_err = crate::repository::RepositoryError::NotFound("资源ID: 123".to_string());
        let api_err: LogSeekApiError = repo_err.into();
        let response = api_err.into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let json = extract_json_body(response).await;
        assert_eq!(json["title"], "资源不存在");
        assert_eq!(json["status"], 404);
        assert!(json["detail"].as_str().unwrap().contains("资源ID: 123"));
    }

    #[tokio::test]
    async fn test_repository_error_storage_error_conversion() {
        let repo_err = crate::repository::RepositoryError::StorageError("S3 连接失败".to_string());
        let api_err: LogSeekApiError = repo_err.into();
        let response = api_err.into_response();

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

        let json = extract_json_body(response).await;
        assert_eq!(json["title"], "存储服务错误");
        assert_eq!(json["status"], 502);
        assert!(json["detail"].as_str().unwrap().contains("S3 连接失败"));
    }

    #[tokio::test]
    async fn test_repository_error_database_conversion() {
        let repo_err = crate::repository::RepositoryError::Database("查询失败".to_string());
        let api_err: LogSeekApiError = repo_err.into();
        let response = api_err.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let json = extract_json_body(response).await;
        assert_eq!(json["title"], "数据库错误");
        assert_eq!(json["status"], 500);
    }

    #[tokio::test]
    async fn test_query_parse_error_conversion() {
        let parse_err = crate::query::ParseError::InvalidRegex {
            message: "无效的正则表达式".to_string(),
            span: (0, 10),
        };
        let api_err: LogSeekApiError = parse_err.into();
        let response = api_err.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let json = extract_json_body(response).await;
        assert_eq!(json["title"], "查询语法错误");
        assert_eq!(json["status"], 400);
    }

    #[tokio::test]
    async fn test_domain_error_conversion() {
        let domain_err = crate::domain::FileUrlError::InvalidFormat("无效的 URL 格式".to_string());
        let api_err: LogSeekApiError = domain_err.into();
        let response = api_err.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let json = extract_json_body(response).await;
        assert_eq!(json["title"], "业务验证失败");
        assert_eq!(json["status"], 400);
    }

    #[tokio::test]
    async fn test_response_content_type_header() {
        let service_err = crate::service::ServiceError::ConfigError("测试错误".to_string());
        let api_err: LogSeekApiError = service_err.into();
        let response = api_err.into_response();

        let content_type = response.headers().get("content-type").unwrap();
        assert_eq!(
            content_type.to_str().unwrap(),
            "application/problem+json; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn test_error_context_preserved() {
        // 测试错误上下文信息是否完整保留
        let service_err = crate::service::ServiceError::ProcessingError(
            "处理文件 /path/to/file.log 时发生错误: 编码不支持".to_string(),
        );
        let api_err: LogSeekApiError = service_err.into();
        let response = api_err.into_response();

        let json = extract_json_body(response).await;
        let detail = json["detail"].as_str().unwrap();
        assert!(detail.contains("/path/to/file.log"));
        assert!(detail.contains("编码不支持"));
    }

    #[tokio::test]
    async fn test_nested_repository_error_in_service_error() {
        // 测试嵌套的 Repository 错误（通过 ServiceError::Repository）
        let repo_err = crate::repository::RepositoryError::NotFound("嵌套资源".to_string());
        let service_err = crate::service::ServiceError::Repository(repo_err);
        let api_err: LogSeekApiError = service_err.into();
        let response = api_err.into_response();

        // 应该正确处理嵌套错误，返回 404
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let json = extract_json_body(response).await;
        assert_eq!(json["title"], "资源不存在");
        assert_eq!(json["status"], 404);
    }

    #[tokio::test]
    async fn test_problem_details_format() {
        // 验证 Problem Details RFC 7807 格式
        let service_err = crate::service::ServiceError::ConfigError("测试".to_string());
        let api_err: LogSeekApiError = service_err.into();
        let response = api_err.into_response();

        let json = extract_json_body(response).await;

        // 验证必需字段
        assert!(json.get("type").is_some());
        assert!(json.get("title").is_some());
        assert!(json.get("status").is_some());
        assert!(json.get("detail").is_some());

        // 验证 type 字段
        assert_eq!(json["type"], "about:blank");
    }
}
