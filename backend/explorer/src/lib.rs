//! Explorer 模块
//!
//! 提供分布式资源浏览功能，支持本地文件系统、Agent 远程文件和 S3 存储。

pub mod api;
pub mod domain;
pub mod fs;
pub mod service;

// 重新导出供 Agent 使用的类型
pub use service::{ListerConfig, LocalEntry, ResourceLister};
pub use domain::{ResourceItem, ResourceType};

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

  #[cfg(feature = "agent-manager")]
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

  #[cfg(not(feature = "agent-manager"))]
  fn router(&self, pool: SqlitePool) -> Router {
    let service = service::ExplorerService::new(pool);
    api::router(Arc::new(service))
  }
}

register_module!(ExplorerModule);
