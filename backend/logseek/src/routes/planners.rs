use axum::{
  Json,
  body::Body,
  extract::{Path, State},
  http::{StatusCode, header},
  response::Response,
};
use opsbox_core::SqlitePool;
use serde::{Deserialize, Serialize};

use crate::api::LogSeekApiError;
use crate::repository::{RepositoryError, planners};
use crate::service::ServiceError;

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
  pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultPlannerPayload {
  pub app: String,
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
  /// 可选的脚本内容（用于测试未保存的脚本）
  #[serde(default)]
  pub script: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerTestResponse {
  /// 清理后的查询（移除了 app:/dt:/fdt:/tdt: 等）
  pub cleaned_query: String,
  /// 规划出的来源列表（ORL 字符串）
  pub sources: Vec<String>,
  /// 调试日志（print 函数的输出）
  pub debug_logs: Vec<String>,
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
  let default = planners::get_default(&pool).await?;
  Ok(Json(PlannerListResponse { items: list, default }))
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
    None => Err(LogSeekApiError::Repository(RepositoryError::NotFound(format!(
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
    return Err(LogSeekApiError::Service(ServiceError::ConfigError(
      "app 不能为空".to_string(),
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

/// 获取 README 原始 Markdown 文本
pub async fn get_readme_md() -> Result<Response<Body>, LogSeekApiError> {
  // 编译期内嵌 README 内容
  let md = include_str!("../planners/README.md");
  Response::builder()
    .status(StatusCode::OK)
    .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
    .body(Body::from(md))
    .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("构建响应失败: {}", e))))
}

/// 测试脚本：输入完整 q，返回清理后的查询与来源列表
/// 如果提供了 script 参数，使用该脚本内容进行测试（用于测试未保存的脚本）
pub async fn test_script(
  State(pool): State<SqlitePool>,
  Json(body): Json<PlannerTestPayload>,
) -> Result<Json<PlannerTestResponse>, LogSeekApiError> {
  if body.app.trim().is_empty() {
    return Err(LogSeekApiError::Service(ServiceError::ConfigError(
      "app 不能为空".to_string(),
    )));
  }
  // 使用内部实现，支持传入脚本内容
  let plan = if let Some(script_content) = &body.script {
    // 使用传入的脚本内容进行测试
    crate::domain::source_planner::plan_with_starlark_with_script(&pool, Some(&body.app), &body.q, Some(script_content))
      .await?
  } else {
    // 使用已保存的脚本
    crate::domain::source_planner::plan_with_starlark(&pool, Some(&body.app), &body.q).await?
  };

  // 将 Resource 转换为 ORL 字符串
  let sources: Vec<String> = plan
    .sources
    .iter()
    .map(opsbox_core::dfs::build_orl_from_resource)
    .collect();

  Ok(Json(PlannerTestResponse {
    cleaned_query: plan.cleaned_query,
    sources,
    debug_logs: plan.debug_logs,
  }))
}

/// 获取默认规划脚本
pub async fn get_default(State(pool): State<SqlitePool>) -> Result<Json<Option<String>>, LogSeekApiError> {
  let app = planners::get_default(&pool).await?;
  Ok(Json(app))
}

/// 设置默认规划脚本
pub async fn set_default(
  State(pool): State<SqlitePool>,
  Json(body): Json<DefaultPlannerPayload>,
) -> Result<StatusCode, LogSeekApiError> {
  planners::set_default(&pool, Some(&body.app)).await?;
  Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::repository::planners::init_schema;

  async fn setup_test_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    init_schema(&pool).await.unwrap();
    pool
  }

  #[tokio::test]
  async fn test_planner_crud_routes() {
    let pool = setup_test_db().await;

    // 1. Save a script
    let payload = PlannerUpsertPayload {
      app: "test-app".to_string(),
      script: "def plan(q): return []".to_string(),
    };
    save_script(State(pool.clone()), Json(payload)).await.unwrap();

    // 2. Get the script
    let resp = get_script(State(pool.clone()), Path("test-app".to_string()))
      .await
      .unwrap();
    assert_eq!(resp.app, "test-app");
    assert_eq!(resp.script, "def plan(q): return []");

    // 3. List scripts
    let list = list_scripts(State(pool.clone())).await.unwrap();
    assert_eq!(list.items.len(), 1);
    assert_eq!(list.items[0].app, "test-app");

    // 4. Set/Get default
    set_default(
      State(pool.clone()),
      Json(DefaultPlannerPayload { app: "test-app".into() }),
    )
    .await
    .unwrap();
    let default = get_default(State(pool.clone())).await.unwrap();
    assert_eq!(default.0, Some("test-app".to_string()));

    // 5. Delete
    delete_script(State(pool.clone()), Path("test-app".to_string()))
      .await
      .unwrap();
    let list = list_scripts(State(pool.clone())).await.unwrap();
    assert_eq!(list.items.len(), 0);
  }

  #[tokio::test]
  async fn test_get_readme_md() {
    let resp = get_readme_md().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    // Verify it contains some known text from planners/README.md
    // include_str! should work in tests
  }
}
