//! Agent Manager 模块
//!
//! 提供 Agent 注册、管理和标签功能

pub mod manager;
pub mod models;
pub mod repository;
pub mod routes;

use axum::Router;
pub use manager::AgentManager;
use once_cell::sync::OnceCell;
use opsbox_core::SqlitePool;
use repository::AgentRepository;
use std::sync::Arc;

/// 全局 Agent Manager 实例
static GLOBAL_AGENT_MANAGER: OnceCell<Arc<AgentManager>> = OnceCell::new();

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
    tracing::info!("Agent Manager 模块配置完成");
  }

  async fn init_schema(&self, pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
    // 初始化 Agent Manager 的数据库表结构
    let repository = AgentRepository::new(pool.clone());
    repository.init_schema().await?;

    // 额外：初始化全局 Agent Manager（避免在运行时内 block_on）
    if let Err(e) = init_global_agent_manager(pool.clone()).await {
      // 如果已经初始化则忽略，仅记录告警
      tracing::warn!("全局 Agent Manager 初始化跳过: {}", e);
    }

    tracing::info!("Agent Manager: 数据库表结构初始化完成");
    Ok(())
  }

  fn router(&self, _pool: SqlitePool) -> Router {
    // 使用在 init_schema 中初始化的全局实例，避免在运行时中进行阻塞
    let manager = get_global_agent_manager().unwrap_or_else(|| {
      panic!("全局 Agent Manager 未初始化，请确保在启动流程中调用了 init_schema");
    });

    // 创建路由
    routes::create_routes(manager)
  }

  fn cleanup(&self) {
    tracing::info!("Agent Manager 模块清理完成");
  }
}

/// 初始化全局 Agent Manager 实例
pub async fn init_global_agent_manager(pool: SqlitePool) -> Result<(), String> {
  let manager = AgentManager::new(pool).await?;
  GLOBAL_AGENT_MANAGER
    .set(Arc::new(manager))
    .map_err(|_| "全局 Agent Manager 已初始化".to_string())?;
  tracing::info!("全局 Agent Manager 初始化完成");
  Ok(())
}

/// 获取全局 Agent Manager 实例
pub fn get_global_agent_manager() -> Option<Arc<AgentManager>> {
  GLOBAL_AGENT_MANAGER.get().cloned()
}

// 使用 inventory 自动注册模块
opsbox_core::register_module!(AgentManagerModule);

#[cfg(test)]
mod tests {
    use super::*;
    use opsbox_core::Module;

    #[test]
    fn test_agent_manager_module_name() {
        let module = AgentManagerModule::default();
        assert_eq!(module.name(), "AgentManager");
        assert_eq!(module.api_prefix(), "/api/v1/agents");
    }

    #[tokio::test]
    async fn test_agent_manager_module_lifecycle() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let module = AgentManagerModule::default();

        // Test name and prefix
        assert_eq!(module.name(), "AgentManager");
        assert_eq!(module.api_prefix(), "/api/v1/agents");

        // Test init_schema (will also init global manager)
        let result = module.init_schema(&pool).await;
        assert!(result.is_ok());

        // Test configure and cleanup
        module.configure();
        module.cleanup();

        // Test get_global_agent_manager after init
        let manager = get_global_agent_manager();
        assert!(manager.is_some());

        // Test duplicate init
        let result = init_global_agent_manager(pool.clone()).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "全局 Agent Manager 已初始化");
    }

    #[tokio::test]
    async fn test_agent_manager_direct_usage() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        // 确保数据库中有表
        AgentRepository::new(pool.clone()).init_schema().await.unwrap();

        // 初始化全局管理器
        let _ = init_global_agent_manager(pool).await;

        // 直接使用 AgentManager 的方法而不是便利函数
        if let Some(manager) = get_global_agent_manager() {
            let online_agents = manager.list_online_agents().await;
            assert_eq!(online_agents.len(), 0);

            let all_tags = manager.get_all_tags().await;
            assert_eq!(all_tags.len(), 0);
        }
    }
}
