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
    use crate::service::nl2q::NLBody;

    #[tokio::test]
    async fn test_nl2q_route_error() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        // 不初始化 schema 且环境变量中通常没有配置 LLM，会导致 call_llm 失败

        let body = NLBody { nl: "find error".to_string() };
        let res = nl2q(State(pool), Json(body)).await;

        assert!(res.is_err());
    }
}
