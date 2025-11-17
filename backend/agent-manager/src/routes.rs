//! Agent Manager 路由

use crate::manager::AgentManager;
use crate::models::{AgentInfo, AgentListResponse, AgentRegisterRequest, AgentTag, HeartbeatResponse};
use axum::extract::connect_info::ConnectInfo;
use axum::{
  Json, Router,
  extract::{Path, Query, State},
  http::{HeaderMap, StatusCode},
  routing::{delete, get, post},
};
use serde::Deserialize;
use std::net::SocketAddr;
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

/// 日志配置响应
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LogConfigResponse {
  /// 日志级别
  pub level: String,
  /// 日志保留数量（天）
  pub retention_count: usize,
  /// 日志目录
  pub log_dir: String,
}

/// 更新日志级别请求
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct UpdateLogLevelRequest {
  /// 日志级别: "error" | "warn" | "info" | "debug" | "trace"
  pub level: String,
}

/// 更新保留数量请求
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct UpdateRetentionRequest {
  /// 保留数量（天）
  pub retention_count: usize,
}

/// 通用成功响应
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SuccessResponse {
  pub message: String,
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
    .route("/{agent_id}/log/config", get(proxy_agent_log_config))
    .route("/{agent_id}/log/level", axum::routing::put(proxy_agent_log_level))
    .route(
      "/{agent_id}/log/retention",
      axum::routing::put(proxy_agent_log_retention),
    )
    .with_state(manager)
}

/// 注册 Agent
async fn register_agent(
  State(manager): State<Arc<AgentManager>>,
  ConnectInfo(peer): ConnectInfo<SocketAddr>,
  headers: HeaderMap,
  Json(req): Json<AgentRegisterRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
  tracing::info!("收到 Agent 注册请求: id={}, name={}", req.info.id, req.info.name);

  // 先完成 Agent 基础信息注册
  manager
    .register_agent(req.info.clone())
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

  // 从请求中提取客户端 IP（优先 X-Forwarded-For，其次 ConnectInfo）
  let xfwd_ip = headers
    .get("x-forwarded-for")
    .and_then(|v| v.to_str().ok())
    .and_then(|s| s.split(',').next().map(|x| x.trim().to_string()));
  let client_ip = xfwd_ip.unwrap_or_else(|| peer.ip().to_string());

  // 组合监听端口（若未上报则使用 Agent 默认端口 4001）
  let port = req.listen_port.unwrap_or(4001);

  tracing::info!("推断 Agent 访问端点: host={}, port={}", client_ip, port);

  // 以标签的形式持久化（保留现有用户自定义标签）：host 与 listen_port
  let host_tag = AgentTag::new("host".to_string(), client_ip);
  let port_tag = AgentTag::new("listen_port".to_string(), port.to_string());

  // 使用 add 接口避免覆盖已有标签集合
  if let Err(e) = manager.add_agent_tag(&req.info.id, host_tag).await {
    tracing::warn!("保存 host 标签失败: {}", e);
  }
  if let Err(e) = manager.add_agent_tag(&req.info.id, port_tag).await {
    tracing::warn!("保存 listen_port 标签失败: {}", e);
  }

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
      tracing::error!("设置标签失败: {}", e);
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
      tracing::error!("添加标签失败: {}", e);
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
      tracing::error!("移除标签失败: {}", e);
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
      tracing::error!("清空标签失败: {}", e);
      Err(StatusCode::NOT_FOUND)
    }
  }
}

/// 代理获取 Agent 日志配置
async fn proxy_agent_log_config(
  State(manager): State<Arc<AgentManager>>,
  Path(agent_id): Path<String>,
) -> Result<Json<LogConfigResponse>, (StatusCode, String)> {
  // 1. 获取 Agent 信息（包含 host 和 listen_port 标签）
  let agent = manager
    .get_agent(&agent_id)
    .await
    .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Agent {} 不存在", agent_id)))?;

  // 2. 从标签中提取 host 和 port
  let host = agent
    .tags
    .iter()
    .find(|t| t.key == "host")
    .map(|t| t.value.clone())
    .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, "Agent 缺少 host 标签".to_string()))?;

  let port = agent
    .tags
    .iter()
    .find(|t| t.key == "listen_port")
    .and_then(|t| t.value.parse::<u16>().ok())
    .unwrap_or(4001);

  // 3. 构造 Agent API URL
  let url = format!("http://{}:{}/api/v1/log/config", host, port);

  tracing::debug!("代理请求 Agent 日志配置: agent_id={}, url={}", agent_id, url);

  // 4. 转发请求
  let client = reqwest::Client::new();
  let response = client
    .get(&url)
    .timeout(std::time::Duration::from_secs(10))
    .send()
    .await
    .map_err(|e| {
      tracing::error!("无法连接到 Agent {}: {}", agent_id, e);
      (StatusCode::BAD_GATEWAY, format!("无法连接到 Agent: {}", e))
    })?;

  if !response.status().is_success() {
    let status = response.status();
    tracing::error!("Agent {} 返回错误状态: {}", agent_id, status);
    return Err((StatusCode::BAD_GATEWAY, format!("Agent 返回错误: {}", status)));
  }

  let config = response.json::<LogConfigResponse>().await.map_err(|e| {
    tracing::error!("解析 Agent {} 响应失败: {}", agent_id, e);
    (StatusCode::INTERNAL_SERVER_ERROR, format!("解析响应失败: {}", e))
  })?;

  tracing::info!("成功获取 Agent {} 日志配置", agent_id);
  Ok(Json(config))
}

/// 代理更新 Agent 日志级别
async fn proxy_agent_log_level(
  State(manager): State<Arc<AgentManager>>,
  Path(agent_id): Path<String>,
  Json(req): Json<UpdateLogLevelRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, String)> {
  // 1. 获取 Agent 信息（包含 host 和 listen_port 标签）
  let agent = manager
    .get_agent(&agent_id)
    .await
    .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Agent {} 不存在", agent_id)))?;

  // 2. 从标签中提取 host 和 port
  let host = agent
    .tags
    .iter()
    .find(|t| t.key == "host")
    .map(|t| t.value.clone())
    .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, "Agent 缺少 host 标签".to_string()))?;

  let port = agent
    .tags
    .iter()
    .find(|t| t.key == "listen_port")
    .and_then(|t| t.value.parse::<u16>().ok())
    .unwrap_or(4001);

  // 3. 构造 Agent API URL
  let url = format!("http://{}:{}/api/v1/log/level", host, port);

  tracing::debug!(
    "代理更新 Agent 日志级别: agent_id={}, level={}, url={}",
    agent_id,
    req.level,
    url
  );

  // 4. 转发请求
  let client = reqwest::Client::new();
  let response = client
    .put(&url)
    .json(&req)
    .timeout(std::time::Duration::from_secs(10))
    .send()
    .await
    .map_err(|e| {
      tracing::error!("无法连接到 Agent {}: {}", agent_id, e);
      (StatusCode::BAD_GATEWAY, format!("无法连接到 Agent: {}", e))
    })?;

  if !response.status().is_success() {
    let status = response.status();
    let error_text = response.text().await.unwrap_or_default();
    tracing::error!("Agent {} 返回错误状态: {}, 错误信息: {}", agent_id, status, error_text);
    return Err((StatusCode::BAD_GATEWAY, format!("Agent 返回错误: {}", status)));
  }

  let result = response.json::<SuccessResponse>().await.map_err(|e| {
    tracing::error!("解析 Agent {} 响应失败: {}", agent_id, e);
    (StatusCode::INTERNAL_SERVER_ERROR, format!("解析响应失败: {}", e))
  })?;

  tracing::info!("成功更新 Agent {} 日志级别为: {}", agent_id, req.level);
  Ok(Json(result))
}

/// 代理更新 Agent 日志保留数量
async fn proxy_agent_log_retention(
  State(manager): State<Arc<AgentManager>>,
  Path(agent_id): Path<String>,
  Json(req): Json<UpdateRetentionRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, String)> {
  // 1. 获取 Agent 信息（包含 host 和 listen_port 标签）
  let agent = manager
    .get_agent(&agent_id)
    .await
    .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Agent {} 不存在", agent_id)))?;

  // 2. 从标签中提取 host 和 port
  let host = agent
    .tags
    .iter()
    .find(|t| t.key == "host")
    .map(|t| t.value.clone())
    .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, "Agent 缺少 host 标签".to_string()))?;

  let port = agent
    .tags
    .iter()
    .find(|t| t.key == "listen_port")
    .and_then(|t| t.value.parse::<u16>().ok())
    .unwrap_or(4001);

  // 3. 构造 Agent API URL
  let url = format!("http://{}:{}/api/v1/log/retention", host, port);

  tracing::debug!(
    "代理更新 Agent 日志保留数量: agent_id={}, retention_count={}, url={}",
    agent_id,
    req.retention_count,
    url
  );

  // 4. 转发请求
  let client = reqwest::Client::new();
  let response = client
    .put(&url)
    .json(&req)
    .timeout(std::time::Duration::from_secs(10))
    .send()
    .await
    .map_err(|e| {
      tracing::error!("无法连接到 Agent {}: {}", agent_id, e);
      (StatusCode::BAD_GATEWAY, format!("无法连接到 Agent: {}", e))
    })?;

  if !response.status().is_success() {
    let status = response.status();
    let error_text = response.text().await.unwrap_or_default();
    tracing::error!("Agent {} 返回错误状态: {}, 错误信息: {}", agent_id, status, error_text);
    return Err((StatusCode::BAD_GATEWAY, format!("Agent 返回错误: {}", status)));
  }

  let result = response.json::<SuccessResponse>().await.map_err(|e| {
    tracing::error!("解析 Agent {} 响应失败: {}", agent_id, e);
    (StatusCode::INTERNAL_SERVER_ERROR, format!("解析响应失败: {}", e))
  })?;

  tracing::info!("成功更新 Agent {} 日志保留数量为: {} 天", agent_id, req.retention_count);
  Ok(Json(result))
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::http::StatusCode;
  use tower::ServiceExt;

  #[tokio::test]
  async fn test_register_agent_route() {
    use axum::extract::connect_info::ConnectInfo;
    use axum::http::Request;
    use std::net::SocketAddr;

    let pool = sqlx::sqlite::SqlitePool::connect("sqlite::memory:").await.unwrap();

    let manager = Arc::new(AgentManager::new(pool).await.unwrap());
    let app = create_routes(manager);

    // 注册时允许携带 listen_port（可选）
    let agent_info = serde_json::json!({
        "id": "test-agent",
        "name": "Test Agent",
        "version": "1.0.0",
        "hostname": "localhost",
        "tags": [],
        "search_roots": ["/var/log"],
        "last_heartbeat": 0,
        "status": {"type": "Online"},
        "listen_port": 4001
    });

    let mut req = Request::builder()
      .method("POST")
      .uri("/register")
      .header("content-type", "application/json")
      .body(serde_json::to_string(&agent_info).unwrap())
      .unwrap();

    // 注入连接信息，模拟客户端远端地址
    req
      .extensions_mut()
      .insert(ConnectInfo::<SocketAddr>("127.0.0.1:55555".parse().unwrap()));

    let response = app.oneshot(req).await.unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
  }
}
