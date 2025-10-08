// ============================================================================
// Agent 客户端 - 远程搜索服务
// ============================================================================
//
// 此模块只负责调用 Agent 的搜索服务，Agent 的注册和管理由 agent-manager 模块负责

use super::{SearchOptions, SearchResultStream, SearchService, ServiceCapabilities, StorageError};
use async_trait::async_trait;
use futures::StreamExt;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ✅ 复用 agent-manager 的数据模型
pub use agent_manager::models::{AgentInfo, AgentStatus};

/// Agent 搜索请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSearchRequest {
  pub task_id: String,
  pub query: String,
  pub context_lines: usize,
  pub path_filter: Option<String>,
  pub scope: super::SearchScope,
}

/// Agent 消息格式（NDJSON 流式传输）
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AgentMessage {
  /// 搜索结果
  Result(crate::service::search::SearchResult),

  /// 进度更新
  Progress(super::SearchProgress),

  /// 错误
  Error(String),

  /// 完成
  Complete,
}

/// Agent 客户端
///
/// 与单个 Agent 通信，实现 SearchService trait
pub struct AgentClient {
  /// Agent ID
  pub agent_id: String,

  /// Agent 端点 (e.g., "http://192.168.1.10:8090")
  endpoint: String,

  /// HTTP 客户端
  client: reqwest::Client,

  /// 请求超时
  timeout: Duration,
}

impl AgentClient {
  /// 创建新的 Agent 客户端
  ///
  /// # 参数
  ///
  /// * `agent_id` - Agent 的唯一标识符
  /// * `endpoint` - Agent 的 HTTP 端点
  pub fn new(agent_id: String, endpoint: String) -> Self {
    Self {
      agent_id,
      endpoint,
      client: reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .unwrap(),
      timeout: Duration::from_secs(60),
    }
  }

  /// 检查 Agent 健康状态
  pub async fn health_check(&self) -> bool {
    let url = format!("{}/health", self.endpoint);

    match tokio::time::timeout(Duration::from_secs(5), self.client.get(&url).send()).await {
      Ok(Ok(response)) => response.status().is_success(),
      Ok(Err(e)) => {
        warn!("Agent {} 健康检查失败: {}", self.agent_id, e);
        false
      }
      Err(_) => {
        warn!("Agent {} 健康检查超时", self.agent_id);
        false
      }
    }
  }

  /// 获取 Agent 信息
  pub async fn get_info(&self) -> Result<AgentInfo, StorageError> {
    let url = format!("{}/api/v1/info", self.endpoint);

    let response = self
      .client
      .get(&url)
      .timeout(Duration::from_secs(10))
      .send()
      .await
      .map_err(|e| StorageError::ConnectionError(format!("获取 Agent 信息失败: {}", e)))?;

    if !response.status().is_success() {
      return Err(StorageError::Other(format!("Agent 返回错误: {}", response.status())));
    }

    response
      .json()
      .await
      .map_err(|e| StorageError::Other(format!("解析 Agent 信息失败: {}", e)))
  }
}

#[async_trait]
impl SearchService for AgentClient {
  fn service_type(&self) -> &'static str {
    "AgentClient"
  }

  async fn search(
    &self,
    query: &str,
    context_lines: usize,
    options: SearchOptions,
  ) -> Result<SearchResultStream, StorageError> {
    info!(
      "向 Agent {} 发送搜索请求: query={}, context_lines={}",
      self.agent_id, query, context_lines
    );

    // 1. 构造搜索请求
    let request = AgentSearchRequest {
      task_id: uuid::Uuid::new_v4().to_string(),
      query: query.to_string(),
      context_lines,
      path_filter: options.path_filter,
      scope: options.scope,
    };

    debug!("Agent 搜索请求: {:?}", request);

    // 2. 发送 POST 请求到 Agent
    let url = format!("{}/api/v1/search", self.endpoint);
    let response = self
      .client
      .post(&url)
      .json(&request)
      .timeout(options.timeout_secs.map(Duration::from_secs).unwrap_or(self.timeout))
      .send()
      .await
      .map_err(|e| {
        error!("Agent {} 连接失败: {}", self.agent_id, e);
        StorageError::ConnectionError(format!("Agent 连接失败: {}", e))
      })?;

    if !response.status().is_success() {
      let status = response.status();
      let error_text = response.text().await.unwrap_or_default();
      return Err(StorageError::Other(format!(
        "Agent 返回错误: {} - {}",
        status, error_text
      )));
    }

    // 3. 流式接收结果（NDJSON 格式）
    debug!("开始接收 Agent 搜索结果流");

    let stream = response.bytes_stream();
    let agent_id = self.agent_id.clone();
    let mut result_count = 0;

    let result_stream = Box::pin(
      stream
        .filter_map(move |chunk_result| {
          let agent_id = agent_id.clone();
          async move {
            match chunk_result {
              Ok(chunk) => {
                let text = String::from_utf8_lossy(&chunk);

                // 处理每一行 JSON
                let results: Vec<Result<_, StorageError>> = text
                  .lines()
                  .filter(|line| !line.trim().is_empty())
                  .filter_map(|line| match serde_json::from_str::<AgentMessage>(line) {
                    Ok(AgentMessage::Result(result)) => Some(Ok(result)),
                    Ok(AgentMessage::Progress(progress)) => {
                      debug!(
                        "Agent {} 进度: {}/{} 文件",
                        agent_id, progress.processed_files, progress.matched_files
                      );
                      None
                    }
                    Ok(AgentMessage::Error(err)) => Some(Err(StorageError::Other(format!("Agent 错误: {}", err)))),
                    Ok(AgentMessage::Complete) => {
                      debug!("Agent {} 搜索完成", agent_id);
                      None
                    }
                    Err(e) => {
                      warn!("解析 Agent 消息失败: {} (line: {})", e, line);
                      None
                    }
                  })
                  .collect();

                if results.is_empty() {
                  None
                } else {
                  Some(futures::stream::iter(results))
                }
              }
              Err(e) => {
                error!("Agent {} 流读取错误: {}", agent_id, e);
                Some(futures::stream::iter(vec![Err(StorageError::Io(
                  std::io::Error::other(e.to_string()),
                ))]))
              }
            }
          }
        })
        .flatten()
        .inspect(move |result| {
          if result.is_ok() {
            result_count += 1;
            if result_count % 10 == 0 {
              debug!("Agent 已返回 {} 个结果", result_count);
            }
          }
        }),
    );

    Ok(Box::new(result_stream))
  }

  fn capabilities(&self) -> ServiceCapabilities {
    ServiceCapabilities {
      supports_progress: true,
      supports_cancellation: true,
      supports_streaming: true,
      max_concurrent_searches: 10,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_agent_client_creation() {
    let client = AgentClient::new("test-agent".to_string(), "http://localhost:8090".to_string());

    assert_eq!(client.agent_id, "test-agent");
    assert_eq!(client.endpoint, "http://localhost:8090");
  }
}
