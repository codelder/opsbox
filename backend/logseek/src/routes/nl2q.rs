//! 自然语言转查询路由
//!
//! 处理 /nl2q 端点，将自然语言转换为查询字符串

use crate::api::LogSeekApiError;
use crate::api::models::NL2QOut;
use axum::extract::{Json, State};
use opsbox_core::SqlitePool;

// NL → Q 端点，实现将自然语言转换为查询字符串
pub async fn nl2q(
  State(pool): State<SqlitePool>,
  Json(body): Json<crate::service::nl2q::NLBody>,
) -> Result<Json<NL2QOut>, LogSeekApiError> {
  log::info!("NL2Q API请求: {}", body.nl);

  let start = std::time::Instant::now();
  let q = crate::service::nl2q::call_llm(&pool, &body.nl).await.map_err(|e| {
    log::error!("NL2Q API失败: {}", e);
    LogSeekApiError::Internal(opsbox_core::AppError::internal(e.to_string()))
  })?;

  log::info!("NL2Q API成功: {} -> '{}', 耗时: {:?}", body.nl, q, start.elapsed());
  Ok(Json(NL2QOut { q }))
}
