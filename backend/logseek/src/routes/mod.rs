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
pub mod search;
pub mod settings;
pub mod view;

// 重新导出常用函数
pub use helpers::{cpu_max_concurrency, s3_max_concurrency, stream_channel_capacity};

/// 创建 LogSeek 主路由
pub fn router(db_pool: SqlitePool) -> Router {
  Router::new()
    // 搜索路由（多存储源并行搜索）
    .route("/search.ndjson", axum::routing::post(search::stream_search))
    .route("/view.cache.json", axum::routing::get(view::view_cache_json))
    .route(
      "/settings/s3",
      axum::routing::get(settings::get_s3_settings).post(settings::save_s3_settings),
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
    // Planner 脚本管理
    .route(
      "/settings/planners",
      axum::routing::get(planners::list_scripts).post(planners::save_script),
    )
    .route(
      "/settings/planners/{app}",
      axum::routing::get(planners::get_script).delete(planners::delete_script),
    )
    .route("/settings/planners/test", axum::routing::post(planners::test_script))
    .route(
      "/settings/planners/readme",
      axum::routing::get(planners::get_readme_html),
    )
    // 自然语言 → 查询字符串
    .route("/nl2q", axum::routing::post(nl2q::nl2q))
    .with_state(db_pool)
}
