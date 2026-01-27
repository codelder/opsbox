//! 自然语言转查询路由
//!
//! 处理 /nl2q 端点，将自然语言转换为查询字符串

use crate::api::LogSeekApiError;
use crate::api::models::NL2QOut;
use crate::service::ServiceError;
use axum::extract::{Json, State};
use opsbox_core::SqlitePool;

// NL → Q 端点，实现将自然语言转换为查询字符串
pub async fn nl2q(
  State(pool): State<SqlitePool>,
  Json(body): Json<crate::service::nl2q::NLBody>,
) -> Result<Json<NL2QOut>, LogSeekApiError> {
  tracing::info!("NL2Q API请求: {}", body.nl);

  let start = std::time::Instant::now();
  let q = crate::service::nl2q::call_llm(&pool, &body.nl).await.map_err(|e| {
    tracing::error!("NL2Q API失败: {}", e);
    LogSeekApiError::Service(ServiceError::ProcessingError(format!("LLM 调用失败: {}", e)))
  })?;

  tracing::info!("NL2Q API成功: {} -> '{}', 耗时: {:?}", body.nl, q, start.elapsed());
  Ok(Json(NL2QOut { q }))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_service_error_to_api_error_conversion() {
    // 验证 ServiceError 能正确转换为 LogSeekApiError
    let service_err = ServiceError::ProcessingError("LLM 调用失败: test".to_string());
    let api_err = LogSeekApiError::Service(service_err);

    // 验证错误类型正确
    match api_err {
      LogSeekApiError::Service(ServiceError::ProcessingError(msg)) => {
        assert!(msg.contains("LLM"));
      }
      _ => panic!("Expected ProcessingError"),
    }
  }
}
