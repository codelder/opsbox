//! 自然语言转查询路由
//!
//! 处理 /nl2q 端点，将自然语言转换为查询字符串

use crate::api::models::NL2QOut;
use axum::{
  extract::{Json, State},
  http::StatusCode,
};
use opsbox_core::SqlitePool;
use problemdetails::Problem;

// NL → Q 端点，实现将自然语言转换为查询字符串
pub async fn nl2q(
  State(pool): State<SqlitePool>,
  Json(body): Json<crate::service::nl2q::NLBody>,
) -> Result<Json<NL2QOut>, Problem> {
  log::info!("NL2Q API请求: {}", body.nl);

  let start = std::time::Instant::now();
  let q = crate::service::nl2q::call_llm(&pool, &body.nl).await.map_err(|e| {
    log::error!("NL2Q API失败: {}", e);
    problemdetails::new(StatusCode::BAD_GATEWAY)
      .with_title("AI 生成失败")
      .with_detail(e.to_string())
  })?;

  log::info!("NL2Q API成功: {} -> '{}', 耗时: {:?}", body.nl, q, start.elapsed());
  Ok(Json(NL2QOut { q }))
}
