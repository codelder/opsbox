//! Agent Manager 模块
//!
//! 管理所有注册到 OpsBox 的 Agent，提供：
//! - Agent 注册和注销
//! - Agent 健康检查和心跳
//! - Agent 信息查询
//! - Agent 状态管理

pub mod manager;
pub mod models;
pub mod routes;

use axum::Router;
use manager::AgentManager;
use opsbox_core::SqlitePool;
use std::sync::Arc;

/// Agent Manager 模块
#[derive(Default)]
pub struct AgentManagerModule;

#[async_trait::async_trait]
impl opsbox_core::Module for AgentManagerModule {
  fn name(&self) -> &'static str {
    "AgentManager"
  }

  fn api_prefix(&self) -> &'static str {
    "/api/v1/agents"
  }

  fn configure(&self) {
    log::info!("Agent Manager 模块配置完成");
  }

  async fn init_schema(&self, _pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
    // Agent 信息目前存储在内存中，未来可以持久化到数据库
    log::info!("Agent Manager: 暂不需要数据库表");
    Ok(())
  }

  fn router(&self, _pool: SqlitePool) -> Router {
    // 创建 Agent 管理器
    let manager = Arc::new(AgentManager::new());

    // 创建路由
    routes::create_routes(manager)
  }

  fn cleanup(&self) {
    log::info!("Agent Manager 模块清理完成");
  }
}

// 使用 inventory 自动注册模块
opsbox_core::register_module!(AgentManagerModule);
