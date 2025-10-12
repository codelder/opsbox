//! Agent Manager 路由

use crate::manager::AgentManager;
use crate::models::{AgentInfo, AgentListResponse, AgentRegisterRequest, AgentTag, HeartbeatResponse};
use axum::{
  extract::{Path, Query, State},
  http::StatusCode,
  routing::{delete, get, post},
  Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

/// 查询参数
#[derive(Debug, Deserialize)]
pub struct AgentQuery {
  /// 标签筛选（多个标签用逗号分隔）
  pub tags: Option<String>,
  /// 是否只返回在线 Agent
  pub online_only: Option<bool>,
}

/// 标签列表响应
#[derive(Debug, serde::Serialize)]
pub struct TagListResponse {
  pub tags: Vec<String>,
  pub total: usize,
}

/// 设置标签请求
#[derive(Debug, serde::Deserialize)]
pub struct SetTagsRequest {
  pub tags: Vec<AgentTag>,
}

/// 添加标签请求
#[derive(Debug, serde::Deserialize)]
pub struct AddTagRequest {
  pub key: String,
  pub value: String,
}

/// 移除标签请求
#[derive(Debug, serde::Deserialize)]
pub struct RemoveTagRequest {
  pub key: String,
  pub value: String,
}

/// 创建 Agent 管理路由
pub fn create_routes(manager: Arc<AgentManager>) -> Router {
  Router::new()
    .route("/register", post(register_agent))
    .route("/", get(list_agents))
    .route("/tags", get(list_tags))
    .route("/{agent_id}", get(get_agent).delete(unregister_agent))
    .route("/{agent_id}/heartbeat", post(heartbeat))
    .route("/{agent_id}/tags", post(set_agent_tags).get(get_agent_tags))
    .route("/{agent_id}/tags/add", post(add_agent_tag))
    .route("/{agent_id}/tags/remove", delete(remove_agent_tag))
    .route("/{agent_id}/tags/clear", delete(clear_agent_tags))
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

/// 列出所有 Agent（支持标签筛选）
async fn list_agents(
  State(manager): State<Arc<AgentManager>>,
  Query(query): Query<AgentQuery>,
) -> Json<AgentListResponse> {
  let agents = if let Some(tags_str) = &query.tags {
    // 解析标签字符串（逗号分隔的 key=value 格式）
    let tag_filters: Vec<AgentTag> = tags_str
      .split(',')
      .filter_map(|s| {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
          AgentTag::from_string(trimmed)
        } else {
          None
        }
      })
      .collect();

    if query.online_only.unwrap_or(false) {
      manager.list_online_agents_by_tags(&tag_filters).await
    } else {
      manager.list_agents_by_tags(&tag_filters).await
    }
  } else if query.online_only.unwrap_or(false) {
    manager.list_online_agents().await
  } else {
    manager.list_agents().await
  };

  let total = agents.len();
  Json(AgentListResponse { agents, total })
}

/// 列出所有可用的标签
async fn list_tags(State(manager): State<Arc<AgentManager>>) -> Json<TagListResponse> {
  let tags = manager.get_all_tags().await;
  let tag_strings: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
  let total = tag_strings.len();
  Json(TagListResponse {
    tags: tag_strings,
    total,
  })
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

/// 设置 Agent 标签
async fn set_agent_tags(
  State(manager): State<Arc<AgentManager>>,
  Path(agent_id): Path<String>,
  Json(req): Json<SetTagsRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
  match manager.set_agent_tags(&agent_id, req.tags).await {
    Ok(_) => Ok(Json(serde_json::json!({"message": "标签设置成功"}))),
    Err(e) => {
      log::error!("设置标签失败: {}", e);
      Err(StatusCode::NOT_FOUND)
    }
  }
}

/// 获取 Agent 标签
async fn get_agent_tags(
  State(manager): State<Arc<AgentManager>>,
  Path(agent_id): Path<String>,
) -> Result<Json<Vec<AgentTag>>, StatusCode> {
  match manager.get_agent(&agent_id).await {
    Some(agent) => Ok(Json(agent.tags)),
    None => Err(StatusCode::NOT_FOUND),
  }
}

/// 添加 Agent 标签
async fn add_agent_tag(
  State(manager): State<Arc<AgentManager>>,
  Path(agent_id): Path<String>,
  Json(req): Json<AddTagRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
  let tag = AgentTag::new(req.key, req.value);
  match manager.add_agent_tag(&agent_id, tag).await {
    Ok(_) => Ok(Json(serde_json::json!({"message": "标签添加成功"}))),
    Err(e) => {
      log::error!("添加标签失败: {}", e);
      Err(StatusCode::NOT_FOUND)
    }
  }
}

/// 移除 Agent 标签
async fn remove_agent_tag(
  State(manager): State<Arc<AgentManager>>,
  Path(agent_id): Path<String>,
  Json(req): Json<RemoveTagRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
  match manager.remove_agent_tag(&agent_id, &req.key, &req.value).await {
    Ok(_) => Ok(Json(serde_json::json!({"message": "标签移除成功"}))),
    Err(e) => {
      log::error!("移除标签失败: {}", e);
      Err(StatusCode::NOT_FOUND)
    }
  }
}

/// 清空 Agent 标签
async fn clear_agent_tags(
  State(manager): State<Arc<AgentManager>>,
  Path(agent_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
  match manager.clear_agent_tags(&agent_id).await {
    Ok(_) => Ok(Json(serde_json::json!({"message": "标签清空成功"}))),
    Err(e) => {
      log::error!("清空标签失败: {}", e);
      Err(StatusCode::NOT_FOUND)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::http::{Request, StatusCode};
  use tower::ServiceExt;

  #[tokio::test]
  async fn test_register_agent_route() {
    let pool = sqlx::sqlite::SqlitePool::connect("sqlite::memory:").await.unwrap();

    let manager = Arc::new(AgentManager::new(pool).await.unwrap());
    let app = create_routes(manager);

    let agent_info = serde_json::json!({
        "id": "test-agent",
        "name": "Test Agent",
        "version": "1.0.0",
        "hostname": "localhost",
        "tags": [],
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
