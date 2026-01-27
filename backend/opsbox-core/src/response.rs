use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::{Serialize, Deserialize, de::DeserializeOwned};

fn default_success() -> bool {
  true
}

/// 标准成功响应
#[derive(Debug, Serialize, Deserialize)]
#[serde(bound = "T: DeserializeOwned")]
pub struct SuccessResponse<T: Serialize + DeserializeOwned> {
  #[serde(default = "default_success")]
  pub success: bool,
  #[serde(skip_serializing_if = "Option::is_none", default)]
  pub data: Option<T>,
  #[serde(skip_serializing_if = "Option::is_none", default)]
  pub message: Option<String>,
}

impl<T: Serialize + DeserializeOwned> SuccessResponse<T> {
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

impl<T: Serialize + DeserializeOwned> IntoResponse for SuccessResponse<T> {
  fn into_response(self) -> axum::response::Response {
    (StatusCode::OK, Json(self)).into_response()
  }
}

/// 创建简单的 JSON 成功响应
pub fn ok<T: Serialize + DeserializeOwned>(data: T) -> impl IntoResponse {
  SuccessResponse::with_data(data)
}

/// 创建带消息的成功响应
pub fn ok_with_message<T: Serialize + DeserializeOwned>(data: T, message: impl Into<String>) -> impl IntoResponse {
  SuccessResponse::with_data_and_message(data, message)
}

/// 创建纯消息成功响应
pub fn ok_message(message: impl Into<String>) -> impl IntoResponse {
  SuccessResponse::<()>::with_message(message)
}

/// 创建创建资源成功响应 (201 Created)
pub fn created<T: Serialize + DeserializeOwned>(data: T) -> impl IntoResponse {
  (StatusCode::CREATED, Json(SuccessResponse::with_data(data)))
}

/// 创建无内容成功响应 (204 No Content)
pub fn no_content() -> impl IntoResponse {
  StatusCode::NO_CONTENT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_response_structure() {
        let resp = SuccessResponse::<String>::with_data("hello".to_string());
        assert!(resp.success);
        assert_eq!(resp.data, Some("hello".to_string()));
        assert_eq!(resp.message, None);

        let resp = SuccessResponse::<()>::with_message("msg");
        assert!(resp.success);
        assert!(resp.data.is_none());
        assert_eq!(resp.message, Some("msg".to_string()));

        let resp = SuccessResponse::<i32>::with_data_and_message(123, "ok");
        assert!(resp.success);
        assert_eq!(resp.data, Some(123));
        assert_eq!(resp.message, Some("ok".to_string()));
    }

    #[test]
    fn test_serialization() {
        let resp = SuccessResponse::with_data(1);
        let val = serde_json::to_value(&resp).unwrap();
        assert_eq!(val["success"], true);
        assert_eq!(val["data"], 1);
        assert!(val.get("message").is_none());
    }

    #[tokio::test]
    async fn test_helpers() {
        // Test helper functions just strictly by return types or simple check
        let _ = ok(1);
        let _ = ok_message("foo");
        let _ = created(1);
        let _ = no_content();
    }
}
