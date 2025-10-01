use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

/// 标准成功响应
#[derive(Debug, Serialize)]
pub struct SuccessResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl<T: Serialize> SuccessResponse<T> {
    /// 创建带数据的成功响应
    pub fn with_data(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
        }
    }

    /// 创建带消息的成功响应
    pub fn with_message(message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: None,
            message: Some(message.into()),
        }
    }

    /// 创建带数据和消息的成功响应
    pub fn with_data_and_message(data: T, message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: Some(message.into()),
        }
    }
}

impl<T: Serialize> IntoResponse for SuccessResponse<T> {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::OK, Json(self)).into_response()
    }
}

/// 创建简单的 JSON 成功响应
pub fn ok<T: Serialize>(data: T) -> impl IntoResponse {
    SuccessResponse::with_data(data)
}

/// 创建带消息的成功响应
pub fn ok_with_message<T: Serialize>(data: T, message: impl Into<String>) -> impl IntoResponse {
    SuccessResponse::with_data_and_message(data, message)
}

/// 创建纯消息成功响应
pub fn ok_message(message: impl Into<String>) -> impl IntoResponse {
    SuccessResponse::<()>::with_message(message)
}

/// 创建创建资源成功响应 (201 Created)
pub fn created<T: Serialize>(data: T) -> impl IntoResponse {
    (StatusCode::CREATED, Json(SuccessResponse::with_data(data)))
}

/// 创建无内容成功响应 (204 No Content)
pub fn no_content() -> impl IntoResponse {
    StatusCode::NO_CONTENT
}
