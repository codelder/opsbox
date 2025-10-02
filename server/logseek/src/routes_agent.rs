// ============================================================================
// Agent 管理路由
// ============================================================================

use crate::storage::agent::{AgentInfo, AgentManager};
use axum::{
  Json, Router,
  extract::{Path, State},
  http::StatusCode,
  routing::{get, post},
};
use log::{error, info};
use std::sync::Arc;

/// Agent 管理状态
#[derive(Clone)]
pub struct AgentState {
  pub manager: Arc<AgentManager>,
}

impl AgentState {
  pub fn new() -> Self {
    Self {
      manager: Arc::new(AgentManager::new()),
    }
  }
}

impl Default for AgentState {
  fn default() -> Self {
    Self::new()
  }
}

/// 创建 Agent 管理路由
pub fn agent_routes() -> Router<Arc<AgentState>> {
  Router::new()
    .route("/agents/register", post(register_agent))
    .route("/agents/{agent_id}/heartbeat", post(agent_heartbeat))
    .route("/agents/{agent_id}", get(get_agent_info))
    .route("/agents", get(list_agents))
}

/// 注册 Agent
async fn register_agent(
  State(state): State<Arc<AgentState>>,
  Json(info): Json<AgentInfo>,
) -> Result<StatusCode, (StatusCode, String)> {
  info!(
    "Agent 注册请求: id={}, name={}, hostname={}",
    info.id, info.name, info.hostname
  );

  state.manager.register_agent(info).await.map_err(|e| {
    error!("Agent 注册失败: {}", e);
    (StatusCode::INTERNAL_SERVER_ERROR, format!("注册失败: {}", e))
  })?;

  Ok(StatusCode::CREATED)
}

/// Agent 心跳
async fn agent_heartbeat(State(_state): State<Arc<AgentState>>, Path(agent_id): Path<String>) -> StatusCode {
  log::debug!("收到 Agent {} 的心跳", agent_id);
  // TODO: 更新 Agent 最后心跳时间
  StatusCode::OK
}

/// 获取 Agent 信息
async fn get_agent_info(
  State(state): State<Arc<AgentState>>,
  Path(agent_id): Path<String>,
) -> Result<Json<AgentInfo>, (StatusCode, String)> {
  let agent = state
    .manager
    .get_agent(&agent_id)
    .await
    .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Agent {} 不存在", agent_id)))?;

  agent
    .get_info()
    .await
    .map(Json)
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// 列举所有 Agent
async fn list_agents(State(state): State<Arc<AgentState>>) -> Json<Vec<String>> {
  let ids = state.manager.list_agent_ids().await;
  Json(ids)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_agent_state_creation() {
    let state = AgentState::new();
    assert!(Arc::strong_count(&state.manager) >= 1);
  }
}
