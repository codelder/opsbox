//! Agent Manager 模块
//!
//! 提供 Agent 注册、管理和标签功能

pub mod manager;
pub mod models;
pub mod repository;
pub mod routes;

use axum::Router;
use manager::AgentManager;
use models::AgentTag;
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
    log::info!("Agent Manager 模块配置完成");
  }

  async fn init_schema(&self, pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
    // 初始化 Agent Manager 的数据库表结构
    let repository = AgentRepository::new(pool.clone());
    repository.init_schema().await?;
    log::info!("Agent Manager: 数据库表结构初始化完成");
    Ok(())
  }

  fn router(&self, pool: SqlitePool) -> Router {
    // 创建新的 Agent Manager 实例
    let manager = tokio::runtime::Handle::current()
      .block_on(async { AgentManager::new(pool).await.expect("Failed to create AgentManager") });

    // 创建路由
    routes::create_routes(Arc::new(manager))
  }

  fn cleanup(&self) {
    log::info!("Agent Manager 模块清理完成");
  }
}

/// 初始化全局 Agent Manager 实例
pub async fn init_global_agent_manager(pool: SqlitePool) -> Result<(), String> {
  let manager = AgentManager::new(pool).await?;
  GLOBAL_AGENT_MANAGER
    .set(Arc::new(manager))
    .map_err(|_| "全局 Agent Manager 已初始化".to_string())?;
  log::info!("全局 Agent Manager 初始化完成");
  Ok(())
}

/// 获取全局 Agent Manager 实例
pub fn get_global_agent_manager() -> Option<Arc<AgentManager>> {
  GLOBAL_AGENT_MANAGER.get().cloned()
}

/// 获取在线 Agent 端点列表
pub async fn get_online_agent_endpoints() -> Vec<String> {
  if let Some(manager) = get_global_agent_manager() {
    manager
      .list_online_agents()
      .await
      .into_iter()
      .map(|agent| {
        // 构造 Agent 端点 URL
        let endpoint_template =
          std::env::var("AGENT_ENDPOINT_TEMPLATE").unwrap_or_else(|_| "http://localhost:8090".to_string());

        if endpoint_template.contains("{id}") {
          endpoint_template.replace("{id}", &agent.id)
        } else {
          endpoint_template
        }
      })
      .collect()
  } else {
    log::warn!("全局 Agent Manager 未初始化");
    vec![]
  }
}

/// 按标签获取在线 Agent 端点列表
pub async fn get_online_agent_endpoints_by_tags(tags: &[(String, String)]) -> Vec<String> {
  if let Some(manager) = get_global_agent_manager() {
    // 转换标签格式
    let agent_tags: Vec<AgentTag> = tags
      .iter()
      .map(|(key, value)| AgentTag::new(key.clone(), value.clone()))
      .collect();

    manager
      .list_online_agents_by_tags(&agent_tags)
      .await
      .into_iter()
      .map(|agent| {
        // 构造 Agent 端点 URL
        let endpoint_template =
          std::env::var("AGENT_ENDPOINT_TEMPLATE").unwrap_or_else(|_| "http://localhost:8090".to_string());

        if endpoint_template.contains("{id}") {
          endpoint_template.replace("{id}", &agent.id)
        } else {
          endpoint_template
        }
      })
      .collect()
  } else {
    log::warn!("全局 Agent Manager 未初始化");
    vec![]
  }
}

/// 获取所有标签
pub async fn get_all_tags() -> Vec<(String, String)> {
  if let Some(manager) = get_global_agent_manager() {
    manager
      .get_all_tags()
      .await
      .into_iter()
      .map(|tag| (tag.key, tag.value))
      .collect()
  } else {
    log::warn!("全局 Agent Manager 未初始化");
    vec![]
  }
}

// 使用 inventory 自动注册模块
opsbox_core::register_module!(AgentManagerModule);
