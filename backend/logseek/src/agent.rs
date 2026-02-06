// Agent 别名模块：对外暴露与远程 Agent 搜索相关的类型
// 便于在仅保留 Agent 能力的场景下使用更贴切的命名空间

use crate::service::search::{SearchEvent, SearchResult};
use crate::utils::strings::truncate_utf8;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};

// 对外暴露的公共类型与 trait

pub use crate::domain::config::Target;

// 重新导出 opsbox-core 中的 Agent 类型
pub use opsbox_core::agent::{AgentClient, AgentClientError, AgentInfo, AgentStatus, AgentTag};

/// 搜索选项
#[derive(Debug, Clone)]
pub struct SearchOptions {
  /// 路径过滤 - 基础过滤（旧字段，对应 ORL filter）
  pub path_filter: Option<String>,
  /// 路径过滤 - 包含列表（用户指定，与关系）
  pub path_includes: Vec<String>,
  /// 路径过滤 - 排除列表（用户指定，非关系）
  pub path_excludes: Vec<String>,
  /// 搜索目标
  pub target: Target,
  /// 超时时间（秒）
  pub timeout_secs: Option<u64>,
  /// 最大结果数
  pub max_results: Option<usize>,
  /// 强制指定的编码（如 enc:gbk）
  pub encoding: Option<String>,
}

impl Default for SearchOptions {
  fn default() -> Self {
    // 从全局配置读取超时时间
    let timeout_secs = if let Some(t) = crate::utils::tuning::get() {
      t.io_timeout_sec.clamp(5, 300)
    } else {
      std::env::var("LOGSEEK_IO_TIMEOUT_SEC")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(60)
        .clamp(5, 300)
    };

    Self {
      path_filter: None,
      path_includes: Vec::new(),
      path_excludes: Vec::new(),
      target: Target::Dir {
        path: ".".to_string(),
        recursive: true,
      },
      timeout_secs: Some(timeout_secs),
      max_results: None,
      encoding: None,
    }
  }
}

/// 搜索结果流
pub type SearchResultStream =
  Box<dyn Stream<Item = Result<SearchResult, AgentClientError>> + Send + Unpin>;

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
use std::time::Duration;
use tracing::{debug, error, info, trace, warn};

/// Agent 搜索请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSearchRequest {
  pub task_id: String,
  pub query: String,
  pub context_lines: usize,
  pub path_filter: Option<String>,
  pub path_includes: Vec<String>,
  pub path_excludes: Vec<String>,
  pub target: Target,
  pub encoding: Option<String>,
}

// 扩展：通过 ID 创建 AgentClient 的辅助函数
pub async fn create_agent_client_by_id(
  pool: &opsbox_core::SqlitePool,
  agent_id: String,
) -> Result<AgentClient, AgentClientError> {
  use agent_manager::repository::AgentRepository;

  // 直接创建 repository，不依赖全局 manager
  let repo = AgentRepository::new(pool.clone());

  // 查找 Agent 信息
  if let Some(agent_info) = repo
    .get_agent(&agent_id)
    .await
    .map_err(|e| AgentClientError::Other(format!("数据库查询失败: {}", e)))?
  {
    // 从标签中获取实际的 HTTP endpoint
    let host_opt = agent_info.get_tag_value("host");
    let port_opt = agent_info.get_tag_value("listen_port");

    if let (Some(host), Some(port)) = (host_opt, port_opt)
      && port.chars().all(|c| c.is_ascii_digit())
    {
      let http_endpoint = format!("http://{}:{}", host, port);
      // 从全局配置读取超时时间
      let timeout_secs = if let Some(t) = crate::utils::tuning::get() {
        t.io_timeout_sec.clamp(5, 300)
      } else {
        std::env::var("LOGSEEK_IO_TIMEOUT_SEC")
          .ok()
          .and_then(|s| s.parse::<u64>().ok())
          .unwrap_or(60)
          .clamp(5, 300)
      };
      return Ok(AgentClient::new(
        agent_id,
        http_endpoint,
        Some(Duration::from_secs(timeout_secs)),
      ));
    }
  }

  Err(AgentClientError::Other(format!(
    "无法找到 Agent {} 的 HTTP endpoint",
    agent_id
  )))
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
      path_includes: options.path_includes,
      path_excludes: options.path_excludes,
      target: options.target,
      encoding: options.encoding,
    };

    // 中文调试：打印请求明细（仅在 debug 级别或显式开启“线级”调试时）
    if tracing::enabled!(tracing::Level::TRACE) {
      match serde_json::to_string(&request) {
        Ok(s) => trace!("[Wire] → POST {}/api/v1/search body={}", self.endpoint, s),
        Err(_) => trace!("[Wire] → POST {}/api/v1/search (body序列化失败)", self.endpoint),
      }
    } else {
      debug!(
        "Agent 搜索请求: endpoint={}, task_id={}, ctx={}, has_path_filter={}, target=...",
        self.endpoint,
        request.task_id,
        request.context_lines,
        request.path_filter.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
      );
    }

    // 获取重试配置
    let max_attempts = if let Some(t) = crate::utils::tuning::get() {
      t.io_max_retries.clamp(1, 20)
    } else {
      std::env::var("LOGSEEK_IO_MAX_RETRIES")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(5)
        .clamp(1, 20)
    };

    // 发送 POST 请求到 Agent（带重试，指数退避）
    let url = format!("{}/api/v1/search", self.endpoint);
    let mut attempt = 0u32;
    let response = loop {
      attempt += 1;

      let result = self
        .client
        .post(&url)
        .json(&request)
        .timeout(options.timeout_secs.map(Duration::from_secs).unwrap_or(self.timeout))
        .send()
        .await;

      match result {
        Ok(resp) => break resp,
        Err(e) => {
          if attempt >= max_attempts {
            error!("Agent {} 连接失败（已重试 {} 次）: {}", self.agent_id, attempt - 1, e);
            return Err(AgentClientError::ConnectionError(format!(
              "Agent 连接失败（已重试 {} 次）: {}",
              attempt - 1,
              e
            )));
          }

          // 指数退避：100ms * 2^(attempt-1)
          let backoff_ms = 100u64 * 2u64.pow(attempt - 1);
          warn!(
            "Agent {} 连接失败（第 {}/{} 次尝试），{}ms 后重试: {}",
            self.agent_id, attempt, max_attempts, backoff_ms, e
          );
          tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        }
      }
    };

    // 中文调试：打印响应状态与头
    let status = response.status();
    trace!("[Wire] ← 状态: {}", status);
    if tracing::enabled!(tracing::Level::TRACE) {
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

    // 创建取消令牌
    use tokio_util::sync::CancellationToken;
    let cancel_token = CancellationToken::new();
    let cancel_token_clone = cancel_token.clone();

    // 在后台任务中处理行流
    let agent_id_for_task = agent_id.clone();
    tokio::spawn(async move {
      // 设置总超时（5 分钟）
      let total_timeout = Duration::from_secs(300);
      let start = tokio::time::Instant::now();

      // 处理正常的行
      loop {
        // 检查总超时
        if start.elapsed() > total_timeout {
          warn!(
            "Agent {} 流式接收总超时（{}秒），停止处理",
            agent_id_for_task,
            total_timeout.as_secs()
          );
          break;
        }

        tokio::select! {
          line_result = lines.next() => {
            match line_result {
              Some(Ok(line)) => {
                if !line.trim().is_empty() {
                  debug!("🔍 Server解析到NDJSON行: {}", line);
                  if tracing::enabled!(tracing::Level::TRACE) {
                    let preview = if line.len() > 512 {
                      format!("{}...", truncate_utf8(&line, 512))
                    } else {
                      line.clone()
                    };
                    trace!("[Wire] ← NDJSON行: {}", preview);
                  }

                  // 发送到结果流
                  if tx.send(line).await.is_err() {
                    debug!("结果流接收端已关闭，触发取消");
                    cancel_token_clone.cancel();
                    break;
                  }
                }
              }
              Some(Err(e)) => {
                warn!("Agent {} 行解析失败: {}", agent_id_for_task, e);
                // 继续处理其他行
              }
              None => {
                debug!("Agent {} 流结束", agent_id_for_task);
                break;
              }
            }
          }
          _ = cancel_token_clone.cancelled() => {
            info!("Agent {} 搜索被取消", agent_id_for_task);
            break;
          }
        }
      }

      // 关闭发送端
      drop(tx);
    });

    // 将接收端转换为流
    let line_stream = async_stream::stream! {
      while let Some(line) = rx.recv().await {
        yield line;
      }
    };

    // 将完整行解析为 SearchEvent，提取结果
    let result_stream = Box::pin(
      line_stream
        .filter_map({
          let agent_id_for_parse = agent_id.clone();
          move |line| {
            let agent_id = agent_id_for_parse.clone();
            async move {
              debug!("🔍 Server尝试解析SearchEvent: {}", line);
              match serde_json::from_str::<SearchEvent>(&line) {
                Ok(SearchEvent::Success(result)) => {
                  debug!(
                    "✅ Server接收到Success消息: path={}, lines_count={}",
                    result.path,
                    result.lines.len()
                  );
                  Some(Ok(result))
                }
                Ok(SearchEvent::Error { source, message, .. }) => {
                  trace!("[Wire] ← Error: source={}, message={}", source, message);
                  Some(Err(AgentClientError::Other(format!("Agent 错误: {}", message))))
                }
                Ok(SearchEvent::Complete { source, elapsed_ms }) => {
                  debug!(
                    "Agent {} 搜索完成 (source={}, elapsed={}ms)",
                    agent_id, source, elapsed_ms
                  );
                  trace!("[Wire] ← Complete");
                  None
                }
                Err(e) => {
                  warn!("解析 SearchEvent 失败: {} (line: {})", e, line);
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

    // 创建 Drop guard 来触发取消
    struct CancelOnDrop(CancellationToken);
    impl Drop for CancelOnDrop {
      fn drop(&mut self) {
        self.0.cancel();
      }
    }
    let _cancel_guard = CancelOnDrop(cancel_token);

    // 将 guard 移入流中，确保流被 drop 时触发取消
    let result_stream_with_cancel = result_stream.inspect(move |_| {
      let _ = &_cancel_guard; // 捕获 guard
    });

    Ok(Box::new(result_stream_with_cancel))
  }
}

#[cfg(test)]
mod tests {
  // 保留测试模块占位，实际测试应在集成测试中覆盖
}
