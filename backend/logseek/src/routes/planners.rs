use axum::response::Html;
use axum::{
  Json,
  extract::{Path, State},
};
use opsbox_core::SqlitePool;
use serde::{Deserialize, Serialize};

use crate::api::LogSeekApiError;
use crate::domain::config::Source;
use crate::repository::planners;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerUpsertPayload {
  pub app: String,
  pub script: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerItemMeta {
  pub app: String,
  pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerListResponse {
  pub items: Vec<PlannerItemMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerGetResponse {
  pub app: String,
  pub script: String,
  pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerTestPayload {
  /// 业务标识（app:xxx）
  pub app: String,
  /// 完整查询 q（包含可选 app:/dt:/fdt:/tdt: 等）
  pub q: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerTestResponse {
  /// 清理后的查询（移除了 app:/dt:/fdt:/tdt: 等）
  pub cleaned_query: String,
  /// 规划出的来源列表（与 Source 对齐）
  pub sources: Vec<Source>,
}

/// 列出所有脚本（仅元信息）
pub async fn list_scripts(State(pool): State<SqlitePool>) -> Result<Json<PlannerListResponse>, LogSeekApiError> {
  let list = planners::list_scripts(&pool)
    .await?
    .into_iter()
    .map(|m| PlannerItemMeta {
      app: m.app,
      updated_at: m.updated_at,
    })
    .collect();
  Ok(Json(PlannerListResponse { items: list }))
}

/// 获取单个脚本（含内容）
pub async fn get_script(
  State(pool): State<SqlitePool>,
  Path(app): Path<String>,
) -> Result<Json<PlannerGetResponse>, LogSeekApiError> {
  match planners::load_script(&pool, &app).await? {
    Some(s) => Ok(Json(PlannerGetResponse {
      app: s.app,
      script: s.script,
      updated_at: s.updated_at,
    })),
    None => Err(LogSeekApiError::Internal(opsbox_core::AppError::not_found(format!(
      "业务 {} 未配置脚本",
      app
    )))),
  }
}

/// 保存/更新脚本
pub async fn save_script(
  State(pool): State<SqlitePool>,
  Json(body): Json<PlannerUpsertPayload>,
) -> Result<(), LogSeekApiError> {
  if body.app.trim().is_empty() {
    return Err(LogSeekApiError::Internal(opsbox_core::AppError::bad_request(
      "app 不能为空",
    )));
  }
  planners::upsert_script(&pool, &body.app, &body.script).await?;
  Ok(())
}

/// 删除脚本
pub async fn delete_script(State(pool): State<SqlitePool>, Path(app): Path<String>) -> Result<(), LogSeekApiError> {
  planners::delete_script(&pool, &app).await?;
  Ok(())
}

/// 获取渲染后的 README（HTML）
pub async fn get_readme_html() -> Result<Html<String>, LogSeekApiError> {
  // 编译期内嵌 README 内容
  let md = include_str!("../planners/README.md");
  let html = comrak::markdown_to_html(md, &comrak::ComrakOptions::default());
  Ok(Html(html))
}

/// 测试脚本：输入完整 q，返回清理后的查询与来源列表
pub async fn test_script(
  State(pool): State<SqlitePool>,
  Json(body): Json<PlannerTestPayload>,
) -> Result<Json<PlannerTestResponse>, LogSeekApiError> {
  if body.app.trim().is_empty() {
    return Err(LogSeekApiError::Internal(opsbox_core::AppError::bad_request(
      "app 不能为空",
    )));
  }
  // 直接备用 Starlark 规划器（仅做规划，不执行搜索）
  let plan = crate::domain::source_planner::plan_with_starlark(&pool, Some(&body.app), &body.q).await?;
  Ok(Json(PlannerTestResponse {
    cleaned_query: plan.cleaned_query,
    sources: plan.sources,
  }))
}
