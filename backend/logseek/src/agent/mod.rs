// Agent 别名模块：对外暴露与远程 Agent 搜索相关的类型
// 便于在仅保留 Agent 能力的场景下使用更贴切的命名空间

// Agent 模块：远程 Agent 搜索能力与统一搜索类型
// 将原 storage 模块中的 Agent 客户端与搜索类型迁移至此

use crate::utils::strings::truncate_utf8;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// 对外暴露的公共类型与 trait

/// 搜索范围
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchScope {
  /// 搜索指定目录
  Directory { path: Option<String>, recursive: bool },
  /// 搜索指定文件列表
  Files { paths: Vec<String> },
  /// 搜索 tar.gz 文件
  TarGz { path: String },
  /// 搜索所有（由服务自己决定）
  All,
}

/// 搜索选项
#[derive(Debug, Clone)]
pub struct SearchOptions {
  /// 路径过滤
  pub path_filter: Option<String>,
  /// 搜索范围
  pub scope: SearchScope,
  /// 超时时间（秒）
  pub timeout_secs: Option<u64>,
  /// 最大结果数
  pub max_results: Option<usize>,
}

impl Default for SearchOptions {
  fn default() -> Self {
    Self {
      path_filter: None,
      scope: SearchScope::All,
      timeout_secs: Some(300),
      max_results: None,
    }
  }
}

/// 搜索结果流
pub type SearchResultStream =
  Box<dyn Stream<Item = Result<crate::service::search::SearchResult, AgentClientError>> + Send + Unpin>;

/// Agent/搜索相关错误统一
#[derive(Debug, Error)]
pub enum AgentClientError {
  #[error("IO错误: {0}")]
  Io(#[from] std::io::Error),
  #[error("权限被拒绝: {0}")]
  PermissionDenied(String),
  #[error("文件不存在: {0}")]
  NotFound(String),
  #[error("连接错误: {0}")]
  ConnectionError(String),
  #[error("Agent 不可用: {0}")]
  AgentUnavailable(String),
  #[error("超时")]
  Timeout,
  #[error("任务被取消")]
  Cancelled,
  #[error("查询解析错误: {0}")]
  QueryParseError(String),
  #[error("其他错误: {0}")]
  Other(String),
}

/// 搜索服务 trait（远程执行搜索，直接返回结果）
#[async_trait]
pub trait SearchService: Send + Sync {
  /// 获取服务类型
  fn service_type(&self) -> &'static str;

  /// 执行搜索并返回结果流
  async fn search(
    &self,
    query: &str,
    context_lines: usize,
    options: SearchOptions,
  ) -> Result<SearchResultStream, AgentClientError>;

  /// 取消搜索（可选）
  async fn cancel(&self, task_id: &str) -> Result<(), AgentClientError> {
    let _ = task_id;
    Ok(())
  }
}

// =========================== Agent 客户端实现 ===============================
use log::{debug, error, info, trace, warn};
use std::time::Duration;

// 复用 agent-manager 的数据模型
pub use agent_manager::models::{AgentInfo, AgentStatus, AgentTag};

/// Agent 搜索请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSearchRequest {
  pub task_id: String,
  pub query: String,
  pub context_lines: usize,
  pub path_filter: Option<String>,
  pub scope: SearchScope,
}

/// Agent 消息格式（NDJSON 流式传输）
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "lowercase")]
pub enum AgentMessage {
  /// 搜索结果
  Result(crate::service::search::SearchResult),
  /// 错误
  Error(String),
  /// 完成
  Complete,
}

/// Agent 客户端（实现 SearchService）
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
  /// 创建新的 Agent 客户端（使用 Agent ID 作为标识符）
  pub fn new(agent_id: String, endpoint: String) -> Self {
    // 如果endpoint不包含协议，自动添加http://
    let full_endpoint = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
      endpoint
    } else {
      format!("http://{}", endpoint)
    };

    Self {
      agent_id,
      endpoint: full_endpoint,
      client: reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .unwrap(),
      timeout: Duration::from_secs(60),
    }
  }

  /// 通过 Agent ID 创建客户端（需要查找实际的 HTTP endpoint）
  pub async fn new_by_agent_id(agent_id: String) -> Result<Self, AgentClientError> {
    use agent_manager::get_global_agent_manager;

    if let Some(manager) = get_global_agent_manager() {
      // 查找 Agent 信息
      if let Some(agent_info) = manager.get_agent(&agent_id).await {
        // 从标签中获取实际的 HTTP endpoint
        let host_opt = agent_info.get_tag_value("host");
        let port_opt = agent_info.get_tag_value("listen_port");

        if let (Some(host), Some(port)) = (host_opt, port_opt)
          && port.chars().all(|c| c.is_ascii_digit())
        {
          let http_endpoint = format!("http://{}:{}", host, port);
          return Ok(Self::new(agent_id, http_endpoint));
        }
      }
    }

    Err(AgentClientError::Other(format!(
      "无法找到 Agent {} 的 HTTP endpoint",
      agent_id
    )))
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
  pub async fn get_info(&self) -> Result<AgentInfo, AgentClientError> {
    let url = format!("{}/api/v1/info", self.endpoint);
    let response = self
      .client
      .get(&url)
      .timeout(Duration::from_secs(10))
      .send()
      .await
      .map_err(|e| AgentClientError::ConnectionError(format!("获取 Agent 信息失败: {}", e)))?;

    if !response.status().is_success() {
      return Err(AgentClientError::Other(format!(
        "Agent 返回错误: {}",
        response.status()
      )));
    }

    response
      .json()
      .await
      .map_err(|e| AgentClientError::Other(format!("解析 Agent 信息失败: {}", e)))
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
  ) -> Result<SearchResultStream, AgentClientError> {
    info!(
      "向 Agent {} 发送搜索请求: query={}, context_lines={}",
      self.agent_id, query, context_lines
    );

    // 构造搜索请求
    let request = AgentSearchRequest {
      task_id: uuid::Uuid::new_v4().to_string(),
      query: query.to_string(),
      context_lines,
      path_filter: options.path_filter,
      scope: options.scope,
    };

    // 中文调试：打印请求明细（仅在 debug 级别或显式开启“线级”调试时）
    if log::log_enabled!(log::Level::Trace) {
      match serde_json::to_string(&request) {
        Ok(s) => trace!("[Wire] → POST {}/api/v1/search body={}", self.endpoint, s),
        Err(_) => trace!("[Wire] → POST {}/api/v1/search (body序列化失败)", self.endpoint),
      }
    } else {
      debug!(
        "Agent 搜索请求: endpoint={}, task_id={}, ctx={}, has_path_filter={}, scope=...",
        self.endpoint,
        request.task_id,
        request.context_lines,
        request.path_filter.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
      );
    }

    // 发送 POST 请求到 Agent
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
        AgentClientError::ConnectionError(format!("Agent 连接失败: {}", e))
      })?;

    // 中文调试：打印响应状态与头
    let status = response.status();
    trace!("[Wire] ← 状态: {}", status);
    if log::log_enabled!(log::Level::Trace) {
      for (k, v) in response.headers() {
        trace!("[Wire] ← 头: {}: {}", k.as_str(), v.to_str().unwrap_or("<bin>"));
      }
    }

    if !status.is_success() {
      let error_text = response.text().await.unwrap_or_default();
      return Err(AgentClientError::Other(format!(
        "Agent 返回错误: {} - {}",
        status, error_text
      )));
    }

    // 流式接收结果（NDJSON 格式，使用 LinesCodec 处理 UTF-8 和行分割）
    debug!("开始接收 Agent 搜索结果流");

    let agent_id = self.agent_id.clone();
    let mut result_count = 0;

    // 使用 LinesCodec 处理流式 UTF-8 解码和行分割
    use futures::StreamExt;
    use tokio_util::codec::{FramedRead, LinesCodec};
    use tokio_util::io::StreamReader;

    let stream = response.bytes_stream();
    // 将 reqwest::Error 转换为 std::io::Error
    let stream = stream.map(|result| result.map_err(std::io::Error::other));
    let stream_reader = StreamReader::new(stream);
    let mut lines = FramedRead::new(stream_reader, LinesCodec::new());

    // 创建结果流
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);

    // 在后台任务中处理行流
    let agent_id_for_task = agent_id.clone();
    tokio::spawn(async move {
      // 处理正常的行
      while let Some(line_result) = lines.next().await {
        match line_result {
          Ok(line) => {
            if !line.trim().is_empty() {
              debug!("🔍 Server解析到NDJSON行: {}", line);
              if log::log_enabled!(log::Level::Trace) {
                let preview = if line.len() > 512 {
                  format!("{}...", truncate_utf8(&line, 512))
                } else {
                  line.clone()
                };
                trace!("[Wire] ← NDJSON行: {}", preview);
              }

              // 发送到结果流
              if tx.send(line).await.is_err() {
                debug!("结果流接收端已关闭");
                break;
              }
            }
          }
          Err(e) => {
            warn!("Agent {} 行解析失败: {}", agent_id_for_task, e);
            // 继续处理其他行
          }
        }
      }

      // 流结束时，尝试处理最后一行（可能没有换行符）
      // LinesCodec 会自动处理最后一行，无需手动flush

      // 关闭发送端
      drop(tx);
    });

    // 将接收端转换为流
    let line_stream = async_stream::stream! {
      while let Some(line) = rx.recv().await {
        yield line;
      }
    };

    // 将完整行解析为 AgentMessage，再提取结果
    let result_stream = Box::pin(
      line_stream
        .filter_map({
          let agent_id_for_parse = agent_id.clone();
          move |line| {
            let agent_id = agent_id_for_parse.clone();
            async move {
              debug!("🔍 Server尝试解析AgentMessage: {}", line);
              match serde_json::from_str::<AgentMessage>(&line) {
                Ok(AgentMessage::Result(result)) => {
                  debug!(
                    "✅ Server收到Result消息: path={}, lines_count={}",
                    result.path,
                    result.lines.len()
                  );
                  Some(Ok(result))
                }
                Ok(AgentMessage::Error(err)) => {
                  trace!("[Wire] ← Error: {}", err);
                  Some(Err(AgentClientError::Other(format!("Agent 错误: {}", err))))
                }
                Ok(AgentMessage::Complete) => {
                  debug!("Agent {} 搜索完成", agent_id);
                  trace!("[Wire] ← Complete");
                  None
                }
                Err(e) => {
                  warn!("解析 Agent 消息失败: {} (line: {})", e, line);
                  None
                }
              }
            }
          }
        })
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
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_agent_client_creation() {
    let client = AgentClient::new("agent-localhost".to_string(), "localhost:8090".to_string());
    assert_eq!(client.agent_id, "agent-localhost");
    // 端点字段不可见（私有），只验证构造未 panic
  }

  #[test]
  fn test_agent_client_with_standard_format() {
    let client = AgentClient::new("agent-prod-01".to_string(), "192.168.50.146:4001".to_string());
    assert_eq!(client.agent_id, "agent-prod-01");
    // 验证内部endpoint会自动添加http://协议
  }
}
