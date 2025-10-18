// Agent 别名模块：对外暴露与远程 Agent 搜索相关的类型
// 便于在仅保留 Agent 能力的场景下使用更贴切的命名空间

// Agent 模块：远程 Agent 搜索能力与统一搜索类型
// 将原 storage 模块中的 Agent 客户端与搜索类型迁移至此

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// 对外暴露的公共类型与 trait

/// 搜索范围
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchScope {
  /// 搜索指定目录
  Directory { path: String, recursive: bool },
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

/// 搜索进度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchProgress {
  pub task_id: String,
  pub processed_files: usize,
  pub matched_files: usize,
  pub total_files: Option<usize>,
  pub status: SearchStatus,
}

/// 搜索状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchStatus {
  Pending,
  Running,
  Completed,
  Failed(String),
  Cancelled,
}

/// 服务能力
#[derive(Debug, Clone, Default)]
pub struct ServiceCapabilities {
  /// 支持进度查询
  pub supports_progress: bool,
  /// 支持取消
  pub supports_cancellation: bool,
  /// 支持流式返回
  pub supports_streaming: bool,
  /// 最大并发搜索数
  pub max_concurrent_searches: usize,
}

/// 搜索结果流
pub type SearchResultStream =
  Box<dyn Stream<Item = Result<crate::service::search::SearchResult, StorageError>> + Send + Unpin>;

/// Agent/搜索相关错误统一
#[derive(Debug, Error)]
pub enum StorageError {
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

// 从 utils::storage 错误类型转换
impl From<crate::utils::storage::StorageError> for StorageError {
  fn from(e: crate::utils::storage::StorageError) -> Self {
    match e {
      crate::utils::storage::StorageError::Io(e) => Self::Io(e),
      crate::utils::storage::StorageError::InvalidBaseUrl(s) => Self::Other(format!("Invalid base URL: {}", s)),
      crate::utils::storage::StorageError::S3Build => Self::Other("S3 client build error".to_string()),
      crate::utils::storage::StorageError::S3GetObject(s) => Self::Other(format!("S3 get object: {}", s)),
      crate::utils::storage::StorageError::S3ToStream(s) => Self::Other(format!("S3 to_stream: {}", s)),
      crate::utils::storage::StorageError::S3ListObjects(s) => Self::Other(format!("S3 list objects: {}", s)),
      crate::utils::storage::StorageError::Regex(s) => Self::QueryParseError(s),
      crate::utils::storage::StorageError::ConnectionTimeout => Self::Timeout,
    }
  }
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
  ) -> Result<SearchResultStream, StorageError>;

  /// 获取搜索能力（可选）
  fn capabilities(&self) -> ServiceCapabilities {
    ServiceCapabilities::default()
  }

  /// 获取搜索进度（可选）
  async fn get_progress(&self, task_id: &str) -> Result<Option<SearchProgress>, StorageError> {
    let _ = task_id;
    Ok(None)
  }

  /// 取消搜索（可选）
  async fn cancel(&self, task_id: &str) -> Result<(), StorageError> {
    let _ = task_id;
    Ok(())
  }
}

// =========================== Agent 客户端实现 ===============================
use futures::StreamExt;
use log::{debug, error, info, warn};
use std::time::Duration;

/// 中文注释：是否启用与 Agent 通讯的“线级”调试日志（打印请求/响应细节、NDJSON行等）
/// 通过环境变量 LOGSEEK_AGENT_DEBUG_WIRE=1 启用
fn wire_debug_enabled() -> bool {
  std::env::var("LOGSEEK_AGENT_DEBUG_WIRE")
    .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE"))
    .unwrap_or(false)
}

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
  /// 进度更新
  Progress(SearchProgress),
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
  /// 创建新的 Agent 客户端
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

    // 构造搜索请求
    let request = AgentSearchRequest {
      task_id: uuid::Uuid::new_v4().to_string(),
      query: query.to_string(),
      context_lines,
      path_filter: options.path_filter,
      scope: options.scope,
    };

    // 中文调试：打印请求明细（仅在 debug 级别或显式开启“线级”调试时）
    if wire_debug_enabled() {
      match serde_json::to_string(&request) {
        Ok(s) => debug!("[Wire] → POST {}/api/v1/search body={}", self.endpoint, s),
        Err(_) => debug!("[Wire] → POST {}/api/v1/search (body序列化失败)", self.endpoint),
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
        StorageError::ConnectionError(format!("Agent 连接失败: {}", e))
      })?;

    // 中文调试：打印响应状态与头
    let status = response.status();
    if wire_debug_enabled() {
      debug!("[Wire] ← 状态: {}", status);
      for (k, v) in response.headers() {
        debug!("[Wire] ← 头: {}: {}", k.as_str(), v.to_str().unwrap_or("<bin>"));
      }
    }

    if !status.is_success() {
      let error_text = response.text().await.unwrap_or_default();
      return Err(StorageError::Other(format!(
        "Agent 返回错误: {} - {}",
        status, error_text
      )));
    }

    // 流式接收结果（NDJSON 格式，按换行组装，避免跨 chunk 破碎）
    debug!("开始接收 Agent 搜索结果流");

    let byte_stream = response.bytes_stream();
    let agent_id = self.agent_id.clone();
    let mut result_count = 0;

    // 先将字节流组装为“完整的行”（按字节分割，避免 UTF-8 跨 chunk 破碎）
    use futures::stream;
    let agent_id_for_scan = agent_id.clone();
    let line_stream = byte_stream
      .scan(Vec::<u8>::new(), move |buf, chunk_result| {
        let agent_id = agent_id_for_scan.clone();
        let mut out: Option<Vec<String>> = None;
        match chunk_result {
          Ok(chunk) => {
            if wire_debug_enabled() {
              debug!("[Wire] ← 收到字节块: {} bytes (Agent {})", chunk.len(), agent_id);
            }
            buf.extend_from_slice(&chunk);
            debug!("🔍 Server缓冲区当前大小: {} bytes", buf.len());
            let mut lines: Vec<String> = Vec::new();
            loop {
              if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                debug!("🔍 Server找到换行符位置: {}", pos);
                // 取出一行（包含 \n）
                let mut line_bytes: Vec<u8> = buf.drain(..=pos).collect();
                // 去掉结尾的 \n
                if line_bytes.last() == Some(&b'\n') {
                  let _ = line_bytes.pop();
                }
                // 去掉可选的 \r
                if line_bytes.last() == Some(&b'\r') {
                  let _ = line_bytes.pop();
                }
                if line_bytes.is_empty() {
                  continue;
                }
                match String::from_utf8(line_bytes) {
                  Ok(s) => {
                    if !s.trim().is_empty() {
                      debug!("🔍 Server解析到NDJSON行: {}", s);
                      if wire_debug_enabled() {
                        let preview = if s.len() > 512 {
                          format!("{}...", &s[..512])
                        } else {
                          s.clone()
                        };
                        debug!("[Wire] ← NDJSON行: {}", preview);
                      }
                      lines.push(s);
                    }
                  }
                  Err(e) => {
                    warn!("Agent {} 行UTF-8解析失败，已跳过: {}", agent_id, e);
                  }
                }
              } else {
                debug!("🔍 Server缓冲区中没有找到换行符，等待更多数据");
                break;
              }
            }
            if !lines.is_empty() {
              debug!("🔍 Server处理了 {} 行NDJSON", lines.len());
              out = Some(lines);
            } else if !buf.is_empty() {
              // 处理最后一个chunk没有换行符的情况
              debug!("🔍 Server处理最后一个chunk，缓冲区大小: {} bytes", buf.len());
              // 使用更安全的UTF-8解析，处理不完整的序列
              match String::from_utf8_lossy(buf) {
                s if !s.trim().is_empty() => {
                  debug!("🔍 Server解析到最后的NDJSON行: {}", s);
                  out = Some(vec![s.to_string()]);
                }
                _ => {
                  debug!("🔍 Server最后的chunk为空或只包含空白字符");
                }
              }
            } else {
              debug!("🔍 Server没有处理任何行");
            }
          }
          Err(e) => {
            error!("Agent {} 流读取错误: {}", agent_id, e);
            // 继续消费后续 chunk
            out = Some(Vec::new());
          }
        }
        std::future::ready(out)
      })
      .flat_map(stream::iter);

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
                Ok(AgentMessage::Progress(progress)) => {
                  debug!(
                    "Agent {} 进度: {}/{} 文件",
                    agent_id, progress.processed_files, progress.matched_files
                  );
                  if wire_debug_enabled() {
                    debug!(
                      "[Wire] ← Progress: task_id={} status={:?}",
                      progress.task_id, progress.status
                    );
                  }
                  None
                }
                Ok(AgentMessage::Error(err)) => {
                  if wire_debug_enabled() {
                    debug!("[Wire] ← Error: {}", err);
                  }
                  Some(Err(StorageError::Other(format!("Agent 错误: {}", err))))
                }
                Ok(AgentMessage::Complete) => {
                  debug!("Agent {} 搜索完成", agent_id);
                  if wire_debug_enabled() {
                    debug!("[Wire] ← Complete");
                  }
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
    // 端点字段不可见（私有），只验证构造未 panic
    let _ = client.capabilities();
  }
}
