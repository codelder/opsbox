// ============================================================================
// SearchCoordinator 集成示例
// ============================================================================
// 
// 本文件展示如何将 SearchCoordinator 集成到现有的 routes.rs 中
//

use axum::{
    extract::State,
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::post,
    Json, Router,
};
use futures::stream::Stream;
use logseek::{
    query::Query,
    service::coordinator::SearchCoordinator,
    storage::{
        agent::{AgentClient, AgentManager},
        local::LocalFileSystem,
    },
};
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::RwLock;

// ============================================================================
// 应用状态
// ============================================================================

struct AppState {
    /// 搜索协调器
    coordinator: Arc<RwLock<SearchCoordinator>>,
    
    /// Agent 管理器
    agent_manager: Arc<AgentManager>,
}

impl AppState {
    fn new() -> Self {
        let mut coordinator = SearchCoordinator::new();
        
        // 添加本地文件系统数据源
        coordinator.add_data_source(Arc::new(
            LocalFileSystem::new(PathBuf::from("/var/log"))
                .with_recursive(true)
        ));
        
        Self {
            coordinator: Arc::new(RwLock::new(coordinator)),
            agent_manager: Arc::new(AgentManager::new()),
        }
    }
}

// ============================================================================
// API 路由
// ============================================================================

fn router() -> Router {
    let state = Arc::new(AppState::new());
    
    Router::new()
        // 分布式搜索端点
        .route("/api/v1/search/distributed", post(distributed_search))
        
        // Agent 管理端点
        .route("/api/v1/agents/register", post(register_agent))
        .route("/api/v1/agents/:id/heartbeat", post(agent_heartbeat))
        .route("/api/v1/agents", get(list_agents))
        
        .with_state(state)
}

// ============================================================================
// 搜索处理器
// ============================================================================

#[derive(Deserialize)]
struct DistributedSearchRequest {
    query: String,
    #[serde(default = "default_context")]
    context_lines: usize,
}

fn default_context() -> usize {
    3
}

/// 分布式搜索端点
async fn distributed_search(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DistributedSearchRequest>,
) -> Result<
    Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>,
    (StatusCode, String),
> {
    // 1. 获取协调器
    let coordinator = state.coordinator.read().await;
    
    // 2. 执行搜索
    let mut results = coordinator
        .search(&req.query, req.context_lines)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    // 3. 转换为 SSE 流
    let stream = async_stream::stream! {
        while let Some(result) = results.recv().await {
            let json = serde_json::to_string(&result).unwrap_or_default();
            yield Ok(Event::default().data(json));
        }
        
        yield Ok(Event::default().event("done").data(""));
    };
    
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ============================================================================
// Agent 管理
// ============================================================================

/// 注册 Agent
async fn register_agent(
    State(state): State<Arc<AppState>>,
    Json(info): Json<logseek::storage::agent::AgentInfo>,
) -> StatusCode {
    match state.agent_manager.register_agent(info.clone()).await {
        Ok(_) => {
            // 将 Agent 添加到协调器
            if let Some(agent) = state.agent_manager.get_agent(&info.id).await {
                state.coordinator.write().await.add_search_service(agent);
            }
            
            StatusCode::CREATED
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Agent 心跳
async fn agent_heartbeat(
    State(_state): State<Arc<AppState>>,
    axum::extract::Path(agent_id): axum::extract::Path<String>,
) -> StatusCode {
    // TODO: 更新 Agent 最后心跳时间
    log::debug!("收到 Agent {} 的心跳", agent_id);
    StatusCode::OK
}

/// 列举所有 Agent
async fn list_agents(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<String>> {
    let ids = state.agent_manager.list_agent_ids().await;
    Json(ids)
}

// ============================================================================
// 使用示例
// ============================================================================

#[tokio::main]
async fn main() {
    // 创建应用状态
    let state = Arc::new(AppState::new());
    
    // 示例 1: 手动添加 Agent
    {
        let agent = Arc::new(AgentClient::new(
            "agent-1".to_string(),
            "http://192.168.1.10:8090".to_string(),
        ));
        
        state.coordinator.write().await.add_search_service(agent);
    }
    
    // 示例 2: 执行分布式搜索
    {
        let coordinator = state.coordinator.read().await;
        let mut results = coordinator.search("error path:*.log", 3).await.unwrap();
        
        while let Some(result) = results.recv().await {
            println!("找到: {} ({} 行)", result.path, result.lines.len());
        }
    }
}

