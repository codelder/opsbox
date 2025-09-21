pub mod routes;
pub use routes::router;

pub mod query;
pub mod renderer;
mod search;
pub mod storage;

// BBIP 文件路径生成与查询字符串处理服务
pub mod bbip_service;

pub mod settings;
pub mod simple_cache;

// 中文注释：自然语言转查询串服务（调用本地 Ollama）
pub mod nl2q;

/// Ensure all persistent stores required by the service are ready.
pub async fn ensure_initialized() -> Result<(), settings::SettingsError> {
  settings::ensure_store().await
}
