use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

/// 统一错误类型
#[derive(Error, Debug)]
pub enum AppError {
    /// 数据库错误
    #[error("数据库错误: {0}")]
    Database(#[from] sqlx::Error),

    /// 配置错误
    #[error("配置错误: {0}")]
    Config(String),

    /// 内部服务器错误
    #[error("内部错误: {0}")]
    Internal(String),

    /// 请求参数错误
    #[error("请求参数错误: {0}")]
    BadRequest(String),

    /// 资源未找到
    #[error("资源未找到: {0}")]
    NotFound(String),

    /// 外部服务错误
    #[error("外部服务错误: {0}")]
    ExternalService(String),
}

impl AppError {
    /// 创建配置错误
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// 创建内部错误
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    /// 创建参数错误
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    /// 创建未找到错误
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// 创建外部服务错误
    pub fn external_service(msg: impl Into<String>) -> Self {
        Self::ExternalService(msg.into())
    }

    /// 获取 HTTP 状态码
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::ExternalService(_) => StatusCode::BAD_GATEWAY,
        }
    }

    /// 获取错误类型标识
    fn error_type(&self) -> &'static str {
        match self {
            Self::Database(_) => "database_error",
            Self::Config(_) => "configuration_error",
            Self::Internal(_) => "internal_error",
            Self::BadRequest(_) => "bad_request",
            Self::NotFound(_) => "not_found",
            Self::ExternalService(_) => "external_service_error",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let error_msg = self.to_string();
        let error_type = self.error_type();

        // 记录错误日志
        match status {
            StatusCode::INTERNAL_SERVER_ERROR | StatusCode::BAD_GATEWAY => {
                log::error!("[{}] {}", error_type, error_msg);
            }
            StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND => {
                log::warn!("[{}] {}", error_type, error_msg);
            }
            _ => {
                log::info!("[{}] {}", error_type, error_msg);
            }
        }

        // 使用 RFC 7807 Problem Details 格式响应
        #[derive(Serialize)]
        struct ProblemDetail {
            r#type: String,
            title: String,
            status: u16,
            detail: String,
        }

        let problem = ProblemDetail {
            r#type: format!("https://opsbox.dev/errors/{}", error_type),
            title: error_msg.clone(),
            status: status.as_u16(),
            detail: error_msg,
        };

        (status, Json(problem)).into_response()
    }
}

/// Result 类型别名
pub type Result<T> = std::result::Result<T, AppError>;
