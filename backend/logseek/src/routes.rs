//! 路由模块
//!
//! 组织和注册所有 HTTP 路由

use axum::Router;
use opsbox_core::SqlitePool;

// 子模块
pub mod helpers;
pub mod llm;
pub mod nl2q;
pub mod planners;
pub mod profiles;
pub mod s3;
pub mod search;
pub mod view;

// 重新导出常用函数
pub use helpers::{cpu_max_concurrency, s3_max_concurrency, stream_channel_capacity};

/// 创建 LogSeek 主路由
pub fn router(db_pool: SqlitePool) -> Router {
  Router::new()
    // 搜索路由（多存储源并行搜索）
    .route("/search.ndjson", axum::routing::post(search::stream_search))
    .route(
      "/search/session/{sid}",
      axum::routing::delete(search::delete_search_session),
    )
    .route("/view.cache.json", axum::routing::get(view::view_cache_json))
    .route("/view/download", axum::routing::get(view::download_file))
    .route("/view/raw", axum::routing::get(view::view_raw_file))
    .route("/view.files.json", axum::routing::get(view::get_file_list_json))
    .route(
      "/settings/s3",
      axum::routing::get(s3::get_s3_settings).post(s3::save_s3_settings),
    )
    // LLM 设置管理
    .route(
      "/settings/llm/backends",
      axum::routing::get(llm::list_backends).post(llm::upsert_backend),
    )
    .route(
      "/settings/llm/backends/{name}",
      axum::routing::delete(llm::delete_backend),
    )
    .route(
      "/settings/llm/backends/{name}/models",
      axum::routing::get(llm::list_models_by_backend),
    )
    .route("/settings/llm/models", axum::routing::post(llm::list_models_by_params))
    .route(
      "/settings/llm/default",
      axum::routing::get(llm::get_default).post(llm::set_default),
    )
    // S3 Profile 管理
    .route(
      "/profiles",
      axum::routing::get(profiles::list_profiles).post(profiles::save_profile),
    )
    .route("/profiles/{name}", axum::routing::delete(profiles::delete_profile))
    // Planner 脚本管理（将 CRUD 端点放到 scripts 命名空间，避免与 test/readme 冲突）
    .route(
      "/settings/planners/scripts",
      axum::routing::get(planners::list_scripts).post(planners::save_script),
    )
    .route(
      "/settings/planners/scripts/{app}",
      axum::routing::get(planners::get_script).delete(planners::delete_script),
    )
    // 其他动作/文档端点保持不变
    .route("/settings/planners/test", axum::routing::post(planners::test_script))
    .route("/settings/planners/readme", axum::routing::get(planners::get_readme_md))
    .route(
      "/settings/planners/default",
      axum::routing::get(planners::get_default).post(planners::set_default),
    )
    // 自然语言 → 查询字符串
    .route("/nl2q", axum::routing::post(nl2q::nl2q))
    .with_state(db_pool)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_router_creation() {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    let router = router(pool);

    // Router should be created successfully
    // We can't easily test routes without making actual HTTP requests,
    // but we can verify the router is constructed
    assert!(std::mem::size_of_val(&router) > 0);
  }

  #[test]
  fn test_module_exports() {
    // Test that re-exported functions are accessible
    let capacity = stream_channel_capacity();
    assert_eq!(capacity, 256);

    let s3_concurrency = s3_max_concurrency();
    assert!((1..=128).contains(&s3_concurrency));

    let cpu_concurrency = cpu_max_concurrency();
    assert!((1..=16).contains(&cpu_concurrency));
  }
}
