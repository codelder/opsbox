//! Agent Manager 路由

use crate::manager::AgentManager;
use crate::models::{AgentInfo, AgentListResponse, AgentRegisterRequest, HeartbeatResponse};
use axum::{
  extract::{Path, State},
  http::StatusCode,
  routing::{get, post},
  Json, Router,
};
use std::sync::Arc;

/// 创建 Agent 管理路由
pub fn create_routes(manager: Arc<AgentManager>) -> Router {
  Router::new()
    .route("/register", post(register_agent))
    .route("/", get(list_agents))
    .route("/{agent_id}", get(get_agent).delete(unregister_agent))
    .route("/{agent_id}/heartbeat", post(heartbeat))
    .with_state(manager)
}

/// 注册 Agent
async fn register_agent(
  State(manager): State<Arc<AgentManager>>,
  Json(req): Json<AgentRegisterRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
  log::info!("收到 Agent 注册请求: id={}, name={}", req.id, req.name);

  manager
    .register_agent(req)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

  Ok(StatusCode::CREATED)
}

/// Agent 心跳
async fn heartbeat(
  State(manager): State<Arc<AgentManager>>,
  Path(agent_id): Path<String>,
) -> Result<Json<HeartbeatResponse>, (StatusCode, String)> {
  manager
    .heartbeat(&agent_id)
    .await
    .map_err(|e| (StatusCode::NOT_FOUND, e))?;

  Ok(Json(HeartbeatResponse {
    success: true,
    message: "心跳已更新".to_string(),
  }))
}

/// 获取 Agent 信息
async fn get_agent(
  State(manager): State<Arc<AgentManager>>,
  Path(agent_id): Path<String>,
) -> Result<Json<AgentInfo>, (StatusCode, String)> {
  let agent = manager
    .get_agent(&agent_id)
    .await
    .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Agent {} 不存在", agent_id)))?;

  Ok(Json(agent))
}

/// 列出所有 Agent
async fn list_agents(State(manager): State<Arc<AgentManager>>) -> Json<AgentListResponse> {
  let agents = manager.list_agents().await;
  let total = agents.len();

  Json(AgentListResponse { agents, total })
}

/// 注销 Agent
async fn unregister_agent(
  State(manager): State<Arc<AgentManager>>,
  Path(agent_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
  manager
    .unregister_agent(&agent_id)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

  Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::http::Request;
  use tower::ServiceExt;

  #[tokio::test]
  async fn test_register_agent_route() {
    let manager = Arc::new(AgentManager::new());
    let app = create_routes(manager);

    let agent_info = serde_json::json!({
        "id": "test-agent",
        "name": "Test Agent",
        "version": "1.0.0",
        "hostname": "localhost",
        "tags": ["test"],
        "search_roots": ["/var/log"],
        "last_heartbeat": 0,
        "status": {"type": "Online"}
    });

    let response = app
      .oneshot(
        Request::builder()
          .method("POST")
          .uri("/register")
          .header("content-type", "application/json")
          .body(serde_json::to_string(&agent_info).unwrap())
          .unwrap(),
      )
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
  }
}
