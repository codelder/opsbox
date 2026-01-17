pub mod api;
pub mod domain;
pub mod fs;
pub mod service;

use async_trait::async_trait;
use axum::Router;
use opsbox_core::{Module, SqlitePool, register_module};
use std::sync::Arc;

#[derive(Default)]
pub struct ExplorerModule;

#[async_trait]
impl Module for ExplorerModule {
  fn name(&self) -> &'static str {
    "explorer"
  }

  fn api_prefix(&self) -> &'static str {
    "/api/v1/explorer"
  }

  async fn init_schema(&self, _pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
  }

  fn router(&self, pool: SqlitePool) -> Router {
    let mut service = service::ExplorerService::new(pool);

    // 使用全局 AgentManager（与 agent-manager 路由共享同一实例）
    if let Some(agent_manager) = agent_manager::get_global_agent_manager() {
      service = service.with_agent_manager(agent_manager);
    } else {
      tracing::warn!("Explorer: 全局 Agent Manager 未初始化，Agent 功能将不可用");
    }

    api::router(Arc::new(service))
  }
}

register_module!(ExplorerModule);
