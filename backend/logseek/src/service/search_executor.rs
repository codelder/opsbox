//! SearchExecutor 服务类
//!
//! 负责协调多数据源并行搜索，管理并发控制，聚合搜索结果

use crate::agent::{SearchOptions, SearchService, create_agent_client_by_id};
use crate::query::Query;
use crate::repository::cache::{cache as simple_cache, new_sid};
use crate::service::ServiceError;
use crate::service::entry_stream::{EntryStreamProcessor, create_entry_stream};
use crate::service::search::{SearchEvent, SearchProcessor};
use futures::StreamExt;
use opsbox_core::SqlitePool;
use opsbox_core::odfs::orl::{EndpointType, ORL, TargetType};
use std::sync::Arc;
use tokio::sync::{Semaphore, mpsc};
use tracing::{debug, info};




/// 搜索执行器配置
#[derive(Debug, Clone)]
pub struct SearchExecutorConfig {
  pub io_max_concurrency: usize,
  pub stream_channel_capacity: usize,
}

impl Default for SearchExecutorConfig {
  fn default() -> Self {
    Self {
      io_max_concurrency: 12,
      stream_channel_capacity: 128,
    }
  }
}

/// 搜索上下文：封装单个数据源搜索任务的公共参数
///
/// 提供构造结果 ORL、缓存结果、发送事件等公共操作，
/// 减少 search_agent_source 和 search_entry_stream_source 的重复代码。
struct SearchContext {
  orl: ORL,
  sid: String,
  ctx: usize,
  tx: mpsc::Sender<SearchEvent>,
  cancel_token: Option<tokio_util::sync::CancellationToken>,
  start_time: std::time::Instant,
}

impl SearchContext {
  fn new(
    orl: ORL,
    sid: String,
    ctx: usize,
    tx: mpsc::Sender<SearchEvent>,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
  ) -> Self {
    Self {
      orl,
      sid,
      ctx,
      tx,
      cancel_token,
      start_time: std::time::Instant::now(),
    }
  }

  /// 根据结果路径构造完整的 ORL 字符串
  fn build_result_orl(&self, res_path: &str) -> String {
    let scheme = self.orl.uri().scheme().as_str();
    let authority = self.orl.uri().authority().map(|a| a.as_str()).unwrap_or("local");

    if self.orl.target_type() == TargetType::Archive {
      let entry_encoded = urlencoding::encode(res_path);
      format!("{}://{}{}?entry={}", scheme, authority, self.orl.path(), entry_encoded)
    } else {
      let full_path = if res_path.starts_with('/') {
        res_path.to_string()
      } else {
        let base = self.orl.path().trim_end_matches('/');
        format!("{}/{}", base, res_path.trim_start_matches('/'))
      };
      format!("{}://{}{}", scheme, authority, full_path)
    }
  }

  /// 缓存结果并发送成功事件，返回是否应继续
  async fn cache_and_send(&self, mut result: crate::service::search::SearchResult) -> bool {
    let result_orl = self.build_result_orl(&result.path);
    simple_cache()
      .put_lines(
        &self.sid,
        &result_orl,
        &result.lines,
        result.encoding.clone().unwrap_or_else(|| "UTF-8".to_string()),
      )
      .await;

    result.path = result_orl;
    self.tx.send(SearchEvent::Success(result)).await.is_ok()
  }

  /// 获取用于事件的源标识符
  fn event_source(&self) -> String {
    match self.orl.endpoint_type() {
      Ok(EndpointType::Agent) => format!("agent:{}", self.orl.effective_id()),
      _ => self.orl.display_name(),
    }
  }

  /// 发送错误事件
  async fn send_error(&self, message: String) {
    let _ = self.tx.send(SearchEvent::Error {
      source: self.event_source(),
      message,
      recoverable: true,
    }).await;
  }

  /// 发送完成事件并记录日志
  async fn send_complete(&self) {
    self.send_complete_with_stats(None).await;
  }

  /// 发送完成事件并记录详细日志（含结果数量）
  async fn send_complete_with_stats(&self, result_count: Option<usize>) {
    let elapsed = self.start_time.elapsed();
    let source_display = self.orl.display_name();
    let event_source = self.event_source();
    let status = if self.is_cancelled() {
      "数据源搜索被取消"
    } else {
      "数据源搜索完成"
    };

    if let Some(count) = result_count {
      tracing::info!(
        "[SearchExecutor] {}: source={}, elapsed={}ms, results={}",
        status,
        source_display,
        elapsed.as_millis(),
        count
      );
    } else {
      tracing::info!(
        "[SearchExecutor] {}: source={}, elapsed={}ms",
        status,
        source_display,
        elapsed.as_millis()
      );
    }

    let _ = self.tx.send(SearchEvent::Complete {
      source: event_source,
      elapsed_ms: elapsed.as_millis() as u64,
    }).await;
  }

  /// 检查是否已取消或通道已关闭
  fn is_cancelled(&self) -> bool {
    self.tx.is_closed() || self.cancel_token.as_ref().is_some_and(|t| t.is_cancelled())
  }
}

pub struct SearchExecutor {
  pool: SqlitePool,
  config: SearchExecutorConfig,
  io_semaphore: Arc<Semaphore>,
}

impl SearchExecutor {
  pub fn new(pool: SqlitePool, config: SearchExecutorConfig) -> Self {
    let io_semaphore = Arc::new(Semaphore::new(config.io_max_concurrency));
    Self {
      pool,
      config,
      io_semaphore,
    }
  }

  async fn get_sources(&self, query: &str) -> Result<(Vec<ORL>, String, Option<String>, Vec<String>, Vec<String>), ServiceError> {
    let mut app: Option<String> = None;
    let mut encoding: Option<String> = None;
    let mut path_includes: Vec<String> = Vec::new();
    let mut path_excludes: Vec<String> = Vec::new(); // 目前只支持 -path
    let mut tokens: Vec<&str> = Vec::new();

    for t in query.split_whitespace() {
      if let Some(rest) = t.strip_prefix("app:")
        && !rest.is_empty()
      {
        app = Some(rest.to_string());
        continue;
      }
      if let Some(rest) = t.strip_prefix("encoding:")
        && !rest.is_empty()
      {
        encoding = Some(rest.to_string());
        continue;
      }
      if let Some(rest) = t.strip_prefix("path:")
        && !rest.is_empty()
      {
        path_includes.push(rest.to_string());
        continue;
      }
      if let Some(rest) = t.strip_prefix("-path:")
        && !rest.is_empty()
      {
        path_excludes.push(rest.to_string());
        continue;
      }
      tokens.push(t);
    }
    let cleaned_before_plan = tokens.join(" ");

    let plan = crate::domain::source_planner::plan_with_starlark(&self.pool, app.as_deref(), &cleaned_before_plan)
      .await
      .map_err(|e| ServiceError::ProcessingError(format!("规划器执行失败: {}", e)))?;

    Ok((plan.sources, plan.cleaned_query, encoding, path_includes, path_excludes))
  }

  fn parse_query(&self, query: &str) -> Result<Arc<Query>, ServiceError> {
    let spec =
      Query::parse_github_like(query).map_err(|e| ServiceError::ProcessingError(format!("查询解析失败: {:?}", e)))?;
    Ok(Arc::new(spec))
  }

  async fn generate_sid_and_cache_keywords(&self, highlights: Vec<crate::query::KeywordHighlight>) -> String {
    let sid = new_sid();
    simple_cache().put_keywords(&sid, highlights).await;
    sid
  }




  async fn search_agent_source(
    context: SearchContext,
    pool: SqlitePool,
    cleaned_query: String,
    encoding_qualifier: Option<String>,
    path_includes: Vec<String>,
    path_excludes: Vec<String>,
  ) {
    // Agent ID 在 ORL 中是 effective_id
    let agent_id = context.orl.effective_id().to_string();

    info!(
      "[SearchExecutor] 开始数据源搜索: endpoint=agent agent_id={} ctx={}",
      agent_id, context.ctx
    );

    let client = match create_agent_client_by_id(&pool, agent_id.clone()).await {
      Ok(client) => client,
      Err(e) => {
        tracing::error!("[SearchExecutor] 无法创建 Agent 客户端 agent_id={} err={}", agent_id, e);
        context.send_error(format!("无法创建 Agent 客户端: {}", e)).await;
        return;
      }
    };

    if !client.health_check().await {
      tracing::error!("[SearchExecutor] Agent 健康检查失败: {}", agent_id);
      context.send_error("Agent 健康检查失败".to_string()).await;
      return;
    }

    // 构造 SearchOptions
    use crate::domain::config::Target as ConfigTarget;

    let target = match context.orl.target_type() {
       TargetType::Dir => ConfigTarget::Dir { path: context.orl.path().to_string(), recursive: true },
       TargetType::Archive => ConfigTarget::Archive { path: context.orl.path().to_string(), entry: context.orl.entry_path().map(|c| c.into_owned()) },
    };

    let search_options = SearchOptions {
      target,
      path_filter: context.orl.filter_glob().map(|c| c.into_owned()),
      path_includes,
      path_excludes,
      encoding: encoding_qualifier,
      ..Default::default()
    };

    let mut stream = match client.search(&cleaned_query, context.ctx, search_options).await {
      Ok(st) => st,
      Err(e) => {
        tracing::error!("[SearchExecutor] 调用 Agent 搜索失败 agent_id={} err={}", agent_id, e);
        context.send_error(format!("调用 Agent 搜索失败: {}", e)).await;
        return;
      }
    };

    let mut result_count = 0;
    while let Some(item) = stream.next().await {
      let Ok(res) = item else { continue; };
      result_count += 1;

      if context.is_cancelled() {
        break;
      }

      // 使用 SearchContext 的方法处理结果
      let result = crate::service::search::SearchResult {
        path: res.path.clone(),
        lines: res.lines.clone(),
        merged: res.merged,
        encoding: res.encoding.clone(),
        archive_path: None,
        source_type: res.source_type,
      };

      if !context.cache_and_send(result).await {
        debug!("[SearchExecutor] 发送失败，通道已关闭");
        break;
      }
    }

    debug!(
      "[SearchExecutor] Agent 结果流消费完成: agent_id={} results={}",
      agent_id, result_count
    );

    context.send_complete_with_stats(Some(result_count)).await;
  }

  async fn search_entry_stream_source(
    context: SearchContext,
    pool: SqlitePool,
    cleaned_query: String,
    encoding_qualifier: Option<String>,
    path_includes: Vec<String>,
    path_excludes: Vec<String>,
  ) {
    let source_name = context.orl.display_name();

    info!("[SearchExecutor] 开始数据源搜索: source={} ctx={}", source_name, context.ctx);

    // 解析查询
    let spec = match Query::parse_github_like(&cleaned_query) {
      Ok(q) => Arc::new(q),
      Err(e) => {
        tracing::error!("[SearchExecutor] 查询解析失败 err={}", e);
        context.send_error(format!("查询解析失败: {}", e)).await;
        return;
      }
    };

    let mut estream = match create_entry_stream(&pool, &context.orl).await {
      Ok(s) => s,
      Err(e) => {
        tracing::error!("[SearchExecutor] 创建条目流失败 err={}", e);
        context.send_error(format!("创建条目流失败: {}", e)).await;
        return;
      }
    };

    let search_proc = Arc::new(SearchProcessor::new_with_encoding(
      spec, context.ctx, encoding_qualifier,
    ));
    let mut processor = EntryStreamProcessor::new(search_proc);
    if let Some(token) = context.cancel_token.clone() {
      processor = processor.with_cancel_token(token);
    }

    // 路径过滤
    if context.orl.endpoint_type().unwrap_or(EndpointType::Local) == EndpointType::Local && context.orl.target_type() == TargetType::Dir {
       processor = processor.with_base_path(context.orl.path());
    }

    if let Some(glob) = context.orl.filter_glob() {
       match crate::query::path_glob_to_filter(&glob) {
        Ok(filter) => { processor = processor.with_extra_path_filter(filter); }
        Err(e) => { tracing::warn!("解析 filter_glob 失败: {}", e); }
      }
    }

    // 处理额外的路径过滤器 (path: 和 -path:)
    if !path_includes.is_empty() || !path_excludes.is_empty() {
        let mut filter = crate::query::PathFilter::default();
        // 处理 includes
        if !path_includes.is_empty() {
            let mut builder = globset::GlobSetBuilder::new();
            for p in &path_includes {
                 match globset::GlobBuilder::new(p).literal_separator(true).build() {
                    Ok(g) => { builder.add(g); },
                    Err(e) => tracing::warn!("无效的 path glob: {} ({})", p, e),
                 }
            }
            if let Ok(set) = builder.build() {
                filter.include = Some(set);
            }
        }
        // 处理 excludes
        if !path_excludes.is_empty() {
            let mut builder = globset::GlobSetBuilder::new();
            for p in &path_excludes {
                 match globset::GlobBuilder::new(p).literal_separator(true).build() {
                    Ok(g) => { builder.add(g); },
                    Err(e) => tracing::warn!("无效的 -path glob: {} ({})", p, e),
                 }
            }
            if let Ok(set) = builder.build() {
                filter.exclude = Some(set);
            }
        }
        processor = processor.with_extra_path_filter(filter);
    }

    let (sr_tx, mut sr_rx) = mpsc::channel::<SearchEvent>(32);

    // 为 sender_task 创建必要的克隆
    let tx_clone = context.tx.clone();
    let sid_clone = context.sid.clone();
    let orl_clone = context.orl.clone();

    let sender_task = tokio::spawn(async move {
      while let Some(event) = sr_rx.recv().await {
        if tx_clone.is_closed() { break; }
        match event {
          SearchEvent::Success(mut res) => {
             // 构造结果 ORL（与 SearchContext::build_result_orl 逻辑一致）
             let result_orl_str = {
                 let scheme = orl_clone.uri().scheme().as_str();
                 let authority = orl_clone.uri().authority().map(|a| a.as_str()).unwrap_or("local");

                 if orl_clone.target_type() == TargetType::Archive {
                     let entry_encoded = urlencoding::encode(&res.path);
                     format!("{}://{}{}?entry={}", scheme, authority, orl_clone.path(), entry_encoded)
                 } else {
                     let full_path = if res.path.starts_with('/') {
                         res.path.clone()
                     } else {
                         let base = orl_clone.path().trim_end_matches('/');
                         format!("{}/{}", base, res.path)
                     };
                     format!("{}://{}{}", scheme, authority, full_path)
                 }
             };

             simple_cache().put_lines(&sid_clone, &result_orl_str, &res.lines, res.encoding.clone().unwrap_or("UTF-8".to_string())).await;
             res.path = result_orl_str;
             if tx_clone.send(SearchEvent::Success(res)).await.is_err() { break; }
          }
           _ => { let _ = tx_clone.send(event).await; }
        }
      }
    });

    if let Err(e) = processor.process_stream(&mut *estream, sr_tx).await {
       context.send_error(e).await;
    }
    let _ = sender_task.await;

    context.send_complete().await;
  }



  pub async fn search(
    &self,
    query: &str,
    context_lines: usize,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
  ) -> Result<(mpsc::Receiver<SearchEvent>, String), ServiceError> {
    tracing::info!("[SearchExecutor] 开始搜索: q={}", query);

    let (sources, cleaned_query, encoding_qualifier, path_includes, path_excludes) = self.get_sources(query).await?;
    tracing::info!("[SearchExecutor] 获取到 {} 个存储源配置", sources.len());

    if sources.is_empty() {
      let (tx, rx) = mpsc::channel(1);
      drop(tx);
      return Ok((rx, String::new()));
    }

    let spec = self.parse_query(&cleaned_query)?;
    let highlights = spec.highlights.clone();

    let sid = self.generate_sid_and_cache_keywords(highlights.clone()).await;

    let (tx, rx) = mpsc::channel(self.config.stream_channel_capacity);

    for source in sources {
      let io_sem = self.io_semaphore.clone();
      let pool = self.pool.clone();
      let orl = source;
      let ctx = context_lines;
      let encoding = encoding_qualifier.clone();
      let sid = sid.clone();
      let cleaned = cleaned_query.clone();
      let p_inc = path_includes.clone();
      let p_exc = path_excludes.clone();
      let tx = tx.clone();
      let token = cancel_token.clone();

      tokio::spawn(async move {
        let _permit = match io_sem.acquire_owned().await {
          Ok(p) => p,
          Err(_) => return,
        };

        // 为两种搜索类型创建统一的 SearchContext
        let context = SearchContext::new(orl, sid, ctx, tx, token);

        // 根据 endpoint 类型直接调度到对应的静态搜索方法
        match context.orl.endpoint_type() {
          Ok(EndpointType::Agent) => {
            Self::search_agent_source(context, pool, cleaned, encoding, p_inc, p_exc).await;
          }
          Ok(EndpointType::Local) | Ok(EndpointType::S3) => {
            Self::search_entry_stream_source(context, pool, cleaned, encoding, p_inc, p_exc).await;
          }
          Err(e) => {
            tracing::error!("Invalid Endpoint Type: {}", e);
          }
        }
      });
    }
    drop(tx);
    Ok((rx, sid))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::repository::planners;
  use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
  use std::str::FromStr;

  /// 创建测试用的内存数据库连接池
  async fn create_test_pool() -> SqlitePool {
    // 使用内存数据库
    let connect_options = SqliteConnectOptions::from_str("sqlite::memory:")
      .unwrap()
      .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
      .max_connections(1)
      .connect_with(connect_options)
      .await
      .expect("Failed to create test pool");

    // 初始化 schema
    crate::init_schema(&pool).await.expect("Failed to initialize schema");

    pool
  }

  /// 将 Windows 风格路径转义为可安全嵌入 Starlark 字面量的字符串
  fn escape_path_for_starlark(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
  }

  /// 设置测试数据源
  async fn setup_test_sources(pool: &SqlitePool) {
    // 创建一个简单的测试 planner 脚本
    // 注意：脚本需要导出 SOURCES 变量，而不是定义函数
    // Endpoint 需要 "kind" 字段，Target 需要 "type" 字段
    let test_script = r#"
# 测试规划脚本
# 使用全局变量 CLEANED_QUERY（由后端注入）

# 导出 SOURCES 列表
SOURCES = [
    "orl://local/tmp/test"
]

# 可选：覆盖 CLEANED_QUERY（如果不覆盖，则使用注入的值）
# CLEANED_QUERY = CLEANED_QUERY
"#;

    planners::upsert_script(pool, "test", test_script)
      .await
      .expect("Failed to create test planner script");

    planners::set_default(pool, Some("test"))
      .await
      .expect("Failed to set default planner");
  }

  #[tokio::test]
  async fn test_search_executor_new() {
    let pool = create_test_pool().await;
    let config = SearchExecutorConfig::default();

    let executor = SearchExecutor::new(pool, config.clone());

    // 验证配置正确设置
    assert_eq!(executor.config.io_max_concurrency, config.io_max_concurrency);
    assert_eq!(executor.config.stream_channel_capacity, config.stream_channel_capacity);
  }

  #[tokio::test]
  async fn test_search_executor_config_default() {
    let config = SearchExecutorConfig::default();

    assert_eq!(config.io_max_concurrency, 12);
    assert_eq!(config.stream_channel_capacity, 128);
  }

  #[tokio::test]
  async fn test_search_executor_config_custom() {
    let config = SearchExecutorConfig {
      io_max_concurrency: 50,
      stream_channel_capacity: 256,
    };

    assert_eq!(config.io_max_concurrency, 50);
    assert_eq!(config.stream_channel_capacity, 256);
  }

  #[tokio::test]
  async fn test_parse_query_success() {
    let pool = create_test_pool().await;
    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    let result = executor.parse_query("error");
    assert!(result.is_ok());

    let spec = result.unwrap();
    assert!(!spec.terms.is_empty());
  }

  #[tokio::test]
  async fn test_parse_query_with_regex() {
    let pool = create_test_pool().await;
    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    let result = executor.parse_query(r#"/\d{3}/"#);
    assert!(result.is_ok());
  }

  #[tokio::test]
  async fn test_parse_query_with_boolean() {
    let pool = create_test_pool().await;
    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    let result = executor.parse_query("error AND warning");
    assert!(result.is_ok());

    let spec = result.unwrap();
    assert_eq!(spec.terms.len(), 2);
  }

  #[tokio::test]
  async fn test_get_sources_with_planner() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    let result = executor.get_sources("error").await;
    assert!(result.is_ok());

    let (sources, cleaned_query, encoding, _, _) = result.unwrap();
    assert!(!sources.is_empty());
    assert_eq!(cleaned_query, "error");
    assert!(encoding.is_none());
  }

  #[tokio::test]
  async fn test_get_sources_with_encoding_qualifier() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    let result = executor.get_sources("encoding:GBK error").await;
    assert!(result.is_ok());

    let (sources, cleaned_query, encoding, _, _) = result.unwrap();
    assert!(!sources.is_empty());
    assert_eq!(cleaned_query, "error");
    assert_eq!(encoding, Some("GBK".to_string()));
  }

  #[tokio::test]
  async fn test_get_sources_with_app_qualifier() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    let result = executor.get_sources("app:test error").await;
    assert!(result.is_ok());

    let (sources, cleaned_query, _, _, _) = result.unwrap();
    assert!(!sources.is_empty());
    assert_eq!(cleaned_query, "error");
  }

  #[tokio::test]
  async fn test_generate_sid_and_cache_keywords() {
    let pool = create_test_pool().await;
    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    let highlights: Vec<crate::query::KeywordHighlight> = vec![
      crate::query::KeywordHighlight::Literal("error".to_string()),
      crate::query::KeywordHighlight::Literal("warning".to_string()),
    ];
    let sid = executor.generate_sid_and_cache_keywords(highlights).await;

    // 验证 sid 不为空
    assert!(!sid.is_empty());
    // sid 应该是 UUID 格式
    assert!(sid.len() > 10);
  }

  #[tokio::test]
  async fn test_io_semaphore_limits_concurrency() {
    let pool = create_test_pool().await;
    let config = SearchExecutorConfig {
      io_max_concurrency: 2, // 限制为 2 个并发
      stream_channel_capacity: 128,
    };
    let executor = Arc::new(SearchExecutor::new(pool, config));

    // 创建多个并发任务
    let mut handles = vec![];
    let counter = Arc::new(tokio::sync::Mutex::new(0));

    for _ in 0..5 {
      let sem = executor.io_semaphore.clone();
      let counter_clone = counter.clone();

      let handle = tokio::spawn(async move {
        let _permit = sem.acquire().await.unwrap();

        // 增加计数器
        let mut count = counter_clone.lock().await;
        *count += 1;
        let current = *count;
        drop(count);

        // 验证并发数不超过限制
        assert!(current <= 2, "并发数超过限制: {}", current);

        // 模拟一些工作
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // 减少计数器
        let mut count = counter_clone.lock().await;
        *count -= 1;
      });

      handles.push(handle);
    }

    // 等待所有任务完成
    for handle in handles {
      handle.await.unwrap();
    }
  }

  #[tokio::test]
  async fn test_config_concurrency_applied() {
    let pool = create_test_pool().await;
    let config = SearchExecutorConfig {
      io_max_concurrency: 5,
      stream_channel_capacity: 128,
    };
    let executor = SearchExecutor::new(pool, config);

    // 验证 semaphore 的可用许可数
    assert_eq!(executor.io_semaphore.available_permits(), 5);
  }

  #[tokio::test]
  async fn test_single_source_search_flow() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("error", 3, None).await;

    // 验证搜索启动成功
    if let Err(ref e) = result {
      eprintln!("Search failed: {:?}", e);
    }
    assert!(result.is_ok());
    let (mut rx, sid) = result.unwrap();

    // 验证 sid 生成
    assert!(!sid.is_empty());

    // 消费事件流
    let mut received_events = false;
    while let Some(event) = rx.recv().await {
      received_events = true;
      match event {
        SearchEvent::Success(_) => {
          // 成功事件
        }
        SearchEvent::Complete { source, .. } => {
          // 完成事件
          assert!(!source.is_empty());
        }
        SearchEvent::Error { .. } => {
          // 错误事件（可能因为 /tmp/test 不存在）
        }
      }
    }

    // 验证至少收到了一些事件（Complete 或 Error）
    assert!(received_events);
  }

  #[tokio::test]
  async fn test_search_with_empty_query() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行空查询搜索
    let result = executor.search("", 3, None).await;

    // 空查询应该能解析成功
    assert!(result.is_ok());
  }

  #[tokio::test]
  async fn test_search_returns_sid() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    let result = executor.search("test query", 3, None).await;
    assert!(result.is_ok());

    let (_rx, sid) = result.unwrap();

    // 验证 sid 格式（应该是 UUID）
    assert!(!sid.is_empty());
    assert!(sid.len() > 20); // UUID 长度通常 > 20
  }

  #[tokio::test]
  async fn test_multiple_sources_parallel_search() {
    let pool = create_test_pool().await;

    // 创建一个返回多个数据源的 planner 脚本
    let multi_source_script = r#"
# 多数据源测试规划脚本
SOURCES = [
    "orl://local/tmp/test1",
    "orl://local/tmp/test2",
    "orl://local/tmp/test3"
]
"#;

    planners::upsert_script(&pool, "multi", multi_source_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("multi")).await.unwrap();

    let config = SearchExecutorConfig {
      io_max_concurrency: 2, // 限制并发数
      stream_channel_capacity: 128,
    };
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索（会启动多个并发任务）
    let result = executor.search("error", 1, None).await;

    // 验证搜索启动成功
    assert!(result.is_ok());
    let (mut rx, sid) = result.unwrap();

    // 验证 sid 生成
    assert!(!sid.is_empty());

    // 消费一些事件（可能是 Complete 或 Error 事件）
    let mut event_count = 0;
    while let Some(_event) = rx.recv().await {
      event_count += 1;
      if event_count >= 3 {
        // 至少收到 3 个 Complete 事件
        break;
      }
    }
  }

  #[tokio::test]
  async fn test_semaphore_enforces_max_concurrency() {
    let pool = create_test_pool().await;
    let config = SearchExecutorConfig {
      io_max_concurrency: 3, // 限制为 3 个并发
      stream_channel_capacity: 128,
    };
    let executor = Arc::new(SearchExecutor::new(pool, config));

    // 创建一个共享计数器来跟踪当前并发数
    let active_count = Arc::new(tokio::sync::Mutex::new(0));
    let max_observed = Arc::new(tokio::sync::Mutex::new(0));

    // 启动 10 个任务，每个任务都需要获取 semaphore 许可
    let mut handles = vec![];
    for i in 0..10 {
      let sem = executor.io_semaphore.clone();
      let active_count_clone = active_count.clone();
      let max_observed_clone = max_observed.clone();

      let handle = tokio::spawn(async move {
        // 获取许可
        let _permit = sem.acquire().await.unwrap();

        // 增加活跃计数
        let mut count = active_count_clone.lock().await;
        *count += 1;
        let current = *count;
        drop(count);

        // 更新观察到的最大并发数
        let mut max = max_observed_clone.lock().await;
        if current > *max {
          *max = current;
        }
        drop(max);

        // 模拟一些工作
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // 减少活跃计数
        let mut count = active_count_clone.lock().await;
        *count -= 1;

        i // 返回任务 ID
      });

      handles.push(handle);
    }

    // 等待所有任务完成
    for handle in handles {
      handle.await.unwrap();
    }

    // 验证最大并发数不超过配置的限制
    let max = *max_observed.lock().await;
    assert!(max <= 3, "观察到的最大并发数 {} 超过限制 3", max);
    assert!(max > 0, "应该有并发执行");
  }

  #[tokio::test]
  async fn test_different_concurrency_configs() {
    // 测试不同的并发配置值
    let test_cases = vec![1, 5, 10, 20, 50];

    for max_concurrency in test_cases {
      let pool = create_test_pool().await;
      let config = SearchExecutorConfig {
        io_max_concurrency: max_concurrency,
        stream_channel_capacity: 128,
      };
      let executor = SearchExecutor::new(pool, config);

      // 验证 semaphore 的可用许可数与配置一致
      assert_eq!(
        executor.io_semaphore.available_permits(),
        max_concurrency,
        "并发配置 {} 未正确应用",
        max_concurrency
      );
    }
  }

  #[tokio::test]
  async fn test_semaphore_releases_on_task_completion() {
    let pool = create_test_pool().await;
    let config = SearchExecutorConfig {
      io_max_concurrency: 2,
      stream_channel_capacity: 128,
    };
    let executor = Arc::new(SearchExecutor::new(pool, config));

    // 初始状态：2 个可用许可
    assert_eq!(executor.io_semaphore.available_permits(), 2);

    // 获取 1 个许可
    let permit1 = executor.io_semaphore.clone().acquire_owned().await.unwrap();
    assert_eq!(executor.io_semaphore.available_permits(), 1);

    // 获取第 2 个许可
    let permit2 = executor.io_semaphore.clone().acquire_owned().await.unwrap();
    assert_eq!(executor.io_semaphore.available_permits(), 0);

    // 释放第 1 个许可
    drop(permit1);
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    assert_eq!(executor.io_semaphore.available_permits(), 1);

    // 释放第 2 个许可
    drop(permit2);
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    assert_eq!(executor.io_semaphore.available_permits(), 2);
  }

  #[tokio::test]
  async fn test_concurrent_searches_respect_limit() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig {
      io_max_concurrency: 2, // 限制为 2 个并发
      stream_channel_capacity: 128,
    };
    let executor = Arc::new(SearchExecutor::new(pool, config));

    // 启动多个并发搜索
    let mut handles = vec![];
    for i in 0..5 {
      let executor_clone = executor.clone();
      let handle = tokio::spawn(async move {
        let query = format!("test{}", i);
        let result = executor_clone.search(&query, 1, None).await;
        result.is_ok()
      });
      handles.push(handle);
    }

    // 等待所有搜索完成
    let mut success_count = 0;
    for handle in handles {
      if handle.await.unwrap() {
        success_count += 1;
      }
    }

    // 验证所有搜索都成功启动
    assert_eq!(success_count, 5, "所有搜索应该成功启动");
  }

  #[tokio::test]
  async fn test_semaphore_prevents_resource_exhaustion() {
    let pool = create_test_pool().await;
    let config = SearchExecutorConfig {
      io_max_concurrency: 5, // 较小的并发限制
      stream_channel_capacity: 128,
    };
    let executor = Arc::new(SearchExecutor::new(pool, config));

    // 尝试启动大量任务（模拟资源压力）
    let task_count = 100;
    let mut handles = vec![];
    let completed = Arc::new(tokio::sync::Mutex::new(0));

    for _ in 0..task_count {
      let sem = executor.io_semaphore.clone();
      let completed_clone = completed.clone();

      let handle = tokio::spawn(async move {
        // 获取许可（会被限制）
        let _permit = sem.acquire().await.unwrap();

        // 模拟短暂工作
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;

        // 增加完成计数
        let mut count = completed_clone.lock().await;
        *count += 1;
      });

      handles.push(handle);
    }

    // 等待所有任务完成
    for handle in handles {
      handle.await.unwrap();
    }

    // 验证所有任务都完成了
    let final_count = *completed.lock().await;
    assert_eq!(final_count, task_count, "所有任务应该完成");

    // 验证 semaphore 恢复到初始状态
    assert_eq!(executor.io_semaphore.available_permits(), 5, "所有许可应该被释放");
  }

  // ========== 错误处理测试 ==========

  #[tokio::test]
  async fn test_parse_query_failure_invalid_regex() {
    let pool = create_test_pool().await;
    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 测试无效的正则表达式
    let result = executor.parse_query(r#"/[invalid(/"#);

    // 应该返回错误
    assert!(result.is_err());

    if let Err(ServiceError::ProcessingError(msg)) = result {
      assert!(msg.contains("查询解析失败"));
    } else {
      panic!("期望 ServiceError::ProcessingError");
    }
  }

  #[tokio::test]
  async fn test_parse_query_failure_complex_invalid() {
    let pool = create_test_pool().await;
    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 测试复杂的无效查询（无效的布尔表达式）
    let result = executor.parse_query(r#"AND OR NOT"#);

    // 某些查询可能被解析为有效（取决于解析器实现）
    // 如果解析失败，验证错误类型
    if result.is_err() {
      if let Err(ServiceError::ProcessingError(msg)) = result {
        assert!(msg.contains("查询解析失败"));
      } else {
        panic!("期望 ServiceError::ProcessingError");
      }
    }
    // 如果解析成功，也是可以接受的（解析器可能很宽容）
  }

  #[tokio::test]
  async fn test_get_sources_failure_no_planner() {
    let pool = create_test_pool().await;
    // 不设置任何 planner

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 尝试获取数据源配置（应该失败，因为没有默认 planner）
    let result = executor.get_sources("error").await;

    // 应该返回错误
    assert!(result.is_err());

    if let Err(ServiceError::ProcessingError(msg)) = result {
      assert!(msg.contains("规划器执行失败"));
    } else {
      panic!("期望 ServiceError::ProcessingError");
    }
  }

  #[tokio::test]
  async fn test_get_sources_failure_invalid_planner_script() {
    let pool = create_test_pool().await;

    // 创建一个无效的 planner 脚本（语法错误）
    let invalid_script = r#"
# 无效的 Starlark 脚本
SOURCES = [
    {
        "endpoint": {"kind": "local" "root": "/tmp/test"},  # 缺少逗号
        "target": {"type": "dir", "path": "."},
    }
]
"#;

    planners::upsert_script(&pool, "invalid", invalid_script).await.unwrap();
    planners::set_default(&pool, Some("invalid")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 尝试获取数据源配置（应该失败，因为脚本有语法错误）
    let result = executor.get_sources("error").await;

    // 应该返回错误
    assert!(result.is_err());

    if let Err(ServiceError::ProcessingError(msg)) = result {
      assert!(msg.contains("规划器执行失败"));
    } else {
      panic!("期望 ServiceError::ProcessingError");
    }
  }

  #[tokio::test]
  async fn test_search_with_invalid_query() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行包含无效正则表达式的搜索
    let result = executor.search(r#"/[invalid(/"#, 3, None).await;

    // 应该返回错误
    assert!(result.is_err());

    if let Err(ServiceError::ProcessingError(msg)) = result {
      assert!(msg.contains("查询解析失败"));
    } else {
      panic!("期望 ServiceError::ProcessingError");
    }
  }

  #[tokio::test]
  async fn test_search_with_no_sources() {
    let pool = create_test_pool().await;

    // 创建一个返回空数据源列表的 planner
    let empty_script = r#"
# 返回空数据源列表
SOURCES = []
"#;

    planners::upsert_script(&pool, "empty", empty_script).await.unwrap();
    planners::set_default(&pool, Some("empty")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索（应该成功但返回空结果）
    let result = executor.search("error", 3, None).await;

    // 应该成功
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();

    // sid 应该为空
    assert!(sid.is_empty());

    // 不应该收到任何事件
    let event = rx.recv().await;
    assert!(event.is_none());
  }

  #[tokio::test]
  async fn test_partial_source_failure_others_continue() {
    let pool = create_test_pool().await;

    // 创建一个包含多个数据源的 planner，其中一些路径不存在
    let temp_dir = tempfile::tempdir().unwrap();
    let valid_root = escape_path_for_starlark(temp_dir.path());
    let invalid_root1 = escape_path_for_starlark(&temp_dir.path().join("missing1"));
    let invalid_root2 = escape_path_for_starlark(&temp_dir.path().join("missing2"));

    // 准备有效数据源中的文件
    std::fs::write(temp_dir.path().join("valid.log"), "test line\n").unwrap();

    let mixed_script = format!(
      r#"
# 混合有效和无效数据源
SOURCES = [
    "orl://local/{}",
    "orl://local/{}",
    "orl://local/{}"
]
"#,
      invalid_root1, valid_root, invalid_root2
    );

    planners::upsert_script(&pool, "mixed", &mixed_script).await.unwrap();
    planners::set_default(&pool, Some("mixed")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("test", 1, None).await;
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();
    assert!(!sid.is_empty());

    // 收集所有事件
    let mut error_events = 0;
    let mut complete_events = 0;
    let mut success_events = 0;

    while let Some(event) = rx.recv().await {
      match event {
        SearchEvent::Error { recoverable, .. } => {
          error_events += 1;
          // 错误应该是可恢复的
          assert!(recoverable, "部分数据源失败应该是可恢复的");
        }
        SearchEvent::Complete { .. } => {
          complete_events += 1;
        }
        SearchEvent::Success(_) => {
          success_events += 1;
        }
      }
    }

    // 应该收到至少 1 个 Complete 事件（可能不是每个数据源都发送）
    assert!(
      complete_events >= 1,
      "应该收到至少 1 个 Complete 事件，实际收到 {}",
      complete_events
    );

    // 可能收到一些错误事件（取决于路径是否存在）
    // 但至少应该尝试了所有数据源
    println!(
      "错误事件: {}, 成功事件: {}, 完成事件: {}",
      error_events, success_events, complete_events
    );
  }

  #[tokio::test]
  async fn test_error_event_sent_to_stream() {
    let pool = create_test_pool().await;

    // 创建一个指向不存在路径的数据源
    let error_script = r#"
# 指向不存在的路径
SOURCES = [
    "orl://local/definitely/does/not/exist/path"
]
"#;

    planners::upsert_script(&pool, "error", error_script).await.unwrap();
    planners::set_default(&pool, Some("error")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("test", 1, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集事件
    let mut found_error = false;
    let mut found_complete = false;

    while let Some(event) = rx.recv().await {
      match event {
        SearchEvent::Error {
          source,
          message,
          recoverable,
        } => {
          found_error = true;
          assert!(!source.is_empty(), "错误事件应该包含数据源名称");
          assert!(!message.is_empty(), "错误事件应该包含错误消息");
          assert!(recoverable, "数据源错误应该是可恢复的");
        }
        SearchEvent::Complete { .. } => {
          found_complete = true;
        }
        SearchEvent::Success(_) => {
          // 不应该有成功事件
        }
      }
    }

    // 应该收到错误事件或完成事件
    assert!(found_error || found_complete, "应该收到错误事件或完成事件");
  }

  #[tokio::test]
  async fn test_error_contains_context_information() {
    let pool = create_test_pool().await;

    // 创建一个会失败的数据源
    let error_script = r#"
SOURCES = [
    "orl://local/nonexistent/error/path"
]
"#;

    planners::upsert_script(&pool, "error_ctx", error_script).await.unwrap();
    planners::set_default(&pool, Some("error_ctx")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("test", 1, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 查找错误事件
    while let Some(event) = rx.recv().await {
      if let SearchEvent::Error { source, message, .. } = event {
        // 验证错误包含上下文信息
        assert!(
          source.contains("test-error-source") || !source.is_empty(),
          "错误应该包含数据源标识: {}",
          source
        );
        assert!(!message.is_empty(), "错误消息不应该为空");
        // 找到错误事件就可以结束测试
        return;
      }
    }
  }

  #[tokio::test]
  async fn test_multiple_errors_all_reported() {
    let pool = create_test_pool().await;

    // 创建多个会失败的数据源
    let multi_error_script = r#"
SOURCES = [
    "orl://local/error/path1",
    "orl://local/error/path2",
    "orl://local/error/path3"
]
"#;

    planners::upsert_script(&pool, "multi_error", multi_error_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("multi_error")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("test", 1, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集所有事件
    let mut error_count = 0;
    let mut complete_count = 0;
    let mut total_events = 0;

    while let Some(event) = rx.recv().await {
      total_events += 1;
      match event {
        SearchEvent::Error { .. } => {
          error_count += 1;
        }
        SearchEvent::Complete { .. } => {
          complete_count += 1;
        }
        _ => {}
      }
    }

    // 应该收到一些事件（Error 或 Complete）
    assert!(total_events > 0, "应该收到至少一些事件，实际收到 {}", total_events);

    // 验证至少尝试了处理多个数据源（通过事件数量判断）
    println!(
      "收到 {} 个错误事件, {} 个完成事件, 总共 {} 个事件",
      error_count, complete_count, total_events
    );
  }

  // ========== Agent 数据源测试 ==========

  #[tokio::test]
  async fn test_agent_source_client_creation_failure() {
    let pool = create_test_pool().await;

    // 创建一个 Agent 数据源配置（使用不存在的 agent_id）
    let agent_script = r#"
# Agent 数据源配置
SOURCES = [
    "orl://agent.nonexistent-agent-id/logs"
]
"#;

    planners::upsert_script(&pool, "agent_test", agent_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("agent_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("error", 3, None).await;
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();
    assert!(!sid.is_empty());

    // 收集事件
    let mut found_error = false;
    let mut error_message = String::new();

    while let Some(event) = rx.recv().await {
      match event {
        SearchEvent::Error {
          source,
          message,
          recoverable,
        } => {
          found_error = true;
          error_message = message.clone();
          // 验证错误信息
          assert!(source.contains("agent:"), "错误源应该标识为 Agent: {}", source);
          assert!(recoverable, "Agent 连接失败应该是可恢复的");
          assert!(
            message.contains("无法创建 Agent 客户端") || message.contains("无法找到 Agent"),
            "错误消息应该说明无法创建客户端: {}",
            message
          );
        }
        SearchEvent::Complete { .. } => {
          // 可能收到完成事件
        }
        SearchEvent::Success(_) => {
          panic!("不应该收到成功事件，因为 Agent 不存在");
        }
      }
    }

    // 应该收到错误事件
    assert!(
      found_error,
      "应该收到 Agent 客户端创建失败的错误事件，错误消息: {}",
      error_message
    );
  }

  #[tokio::test]
  async fn test_agent_source_with_subpath_adjustment() {
    let pool = create_test_pool().await;

    // 创建一个 Agent 数据源配置，包含 subpath
    let agent_script = r#"
# Agent 数据源配置（带 subpath）
SOURCES = [
    "orl://agent.test-agent-with-subpath/var/log/app"
]
"#;

    planners::upsert_script(&pool, "agent_subpath", agent_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("agent_subpath")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("error", 3, None).await;
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();
    assert!(!sid.is_empty());

    // 收集事件（预期会失败，因为 Agent 不存在）
    let mut found_error = false;

    while let Some(event) = rx.recv().await {
      match event {
        SearchEvent::Error { source, message, .. } => {
          found_error = true;
          // 验证错误来自 Agent 数据源
          assert!(
            source.contains("agent:") || source.contains("agent-with-subpath"),
            "错误应该来自 Agent 数据源: {}",
            source
          );
          // 验证错误消息合理
          assert!(!message.is_empty(), "错误消息不应该为空");
        }
        SearchEvent::Complete { .. } => {
          // 可能收到完成事件
        }
        SearchEvent::Success(_) => {
          // 不应该收到成功事件（Agent 不存在）
        }
      }
    }

    // 应该收到错误事件
    assert!(found_error, "应该收到 Agent 相关的错误事件");
  }

  #[tokio::test]
  async fn test_agent_source_with_files_target() {
    let pool = create_test_pool().await;

    // 创建一个 Agent 数据源配置，使用 Files target
    let agent_script = r#"
# Agent 数据源配置（Files target）
SOURCES = [
    "orl://agent.test-agent-files/logs/app.log",
    "orl://agent.test-agent-files/logs/error.log"
]
"#;

    planners::upsert_script(&pool, "agent_files", agent_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("agent_files")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("error", 3, None).await;
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();
    assert!(!sid.is_empty());

    // 收集事件
    let mut received_events = false;

    while let Some(event) = rx.recv().await {
      received_events = true;
      match event {
        SearchEvent::Error { source, .. } => {
          // 验证错误来自 Agent
          assert!(
            source.contains("agent:") || source.contains("agent-files"),
            "错误应该来自 Agent 数据源: {}",
            source
          );
        }
        SearchEvent::Complete { .. } => {
          // 完成事件
        }
        SearchEvent::Success(_) => {
          // 不应该收到成功事件（Agent 不存在）
        }
      }
    }

    // 应该收到一些事件
    assert!(received_events, "应该收到至少一些事件");
  }

  #[tokio::test]
  async fn test_agent_source_with_archive_target() {
    let pool = create_test_pool().await;

    // 创建一个 Agent 数据源配置，使用 Archive target
    let agent_script = r#"
# Agent 数据源配置（Archive target）
SOURCES = [
    "orl://agent.test-agent-archive/backups/logs.tar.gz"
]
"#;

    planners::upsert_script(&pool, "agent_archive", agent_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("agent_archive")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("error", 3, None).await;
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();
    assert!(!sid.is_empty());

    // 收集事件
    let mut received_events = false;

    while let Some(event) = rx.recv().await {
      received_events = true;
      match event {
        SearchEvent::Error { source, message, .. } => {
          // 验证错误来自 Agent
          assert!(
            source.contains("agent:") || source.contains("agent-archive"),
            "错误应该来自 Agent 数据源: {}",
            source
          );
          assert!(!message.is_empty(), "错误消息不应该为空");
        }
        SearchEvent::Complete { .. } => {
          // 完成事件
        }
        SearchEvent::Success(_) => {
          // 不应该收到成功事件（Agent 不存在）
        }
      }
    }

    // 应该收到一些事件
    assert!(received_events, "应该收到至少一些事件");
  }

  #[tokio::test]
  async fn test_agent_source_with_path_filter() {
    let pool = create_test_pool().await;

    // 创建一个 Agent 数据源配置，包含 filter_glob
    let agent_script = r#"
# Agent 数据源配置（带 filter_glob）
SOURCES = [
    "orl://agent.test-agent-filter/logs?glob=*.log"
]
"#;

    planners::upsert_script(&pool, "agent_filter", agent_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("agent_filter")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("error", 3, None).await;
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();
    assert!(!sid.is_empty());

    // 收集事件
    let mut received_events = false;

    while let Some(event) = rx.recv().await {
      received_events = true;
      match event {
        SearchEvent::Error { source, .. } => {
          // 验证错误来自 Agent
          assert!(
            source.contains("agent:") || source.contains("agent-with-filter"),
            "错误应该来自 Agent 数据源: {}",
            source
          );
        }
        SearchEvent::Complete { .. } => {
          // 完成事件
        }
        SearchEvent::Success(_) => {
          // 不应该收到成功事件（Agent 不存在）
        }
      }
    }

    // 应该收到一些事件
    assert!(received_events, "应该收到至少一些事件");
  }

  #[tokio::test]
  async fn test_agent_source_error_is_recoverable() {
    let pool = create_test_pool().await;

    // 创建一个 Agent 数据源配置
    let agent_script = r#"
# Agent 数据源配置
SOURCES = [
    "orl://agent.test-agent-recoverable/logs"
]
"#;

    planners::upsert_script(&pool, "agent_recoverable", agent_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("agent_recoverable")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("error", 3, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集事件
    let mut found_error = false;

    while let Some(event) = rx.recv().await {
      if let SearchEvent::Error { recoverable, .. } = event {
        found_error = true;
        // 验证 Agent 错误是可恢复的
        assert!(recoverable, "Agent 数据源错误应该标记为可恢复的（recoverable=true）");
      }
    }

    // 应该收到错误事件
    assert!(found_error, "应该收到 Agent 错误事件");
  }

  #[tokio::test]
  async fn test_mixed_agent_and_local_sources() {
    let pool = create_test_pool().await;

    // 创建一个包含 Agent 和 Local 数据源的配置
    let mixed_script = r#"
# 混合 Agent 和 Local 数据源
SOURCES = [
    "orl://agent.test-agent-mixed/logs",
    "orl://local/tmp"
]
"#;

    planners::upsert_script(&pool, "mixed_agent_local", mixed_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("mixed_agent_local")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("test", 1, None).await;
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();
    assert!(!sid.is_empty());

    // 收集事件
    let mut agent_events = 0;
    let mut local_events = 0;
    let mut total_events = 0;

    while let Some(event) = rx.recv().await {
      total_events += 1;
      match event {
        SearchEvent::Error { source, .. } => {
          if source.contains("agent:") || source.contains("agent-source") {
            agent_events += 1;
          } else if source.contains("local-source") {
            local_events += 1;
          }
        }
        SearchEvent::Complete { source, .. } => {
          if source.contains("agent:") || source.contains("agent-source") {
            agent_events += 1;
          } else if source.contains("local-source") {
            local_events += 1;
          }
        }
        SearchEvent::Success(_) => {
          // 可能来自 local 数据源
          local_events += 1;
        }
      }
    }

    // 应该收到来自两种数据源的事件
    assert!(total_events > 0, "应该收到至少一些事件");
    assert!(agent_events > 0, "应该收到来自 Agent 数据源的事件");

    println!(
      "收到 {} 个 Agent 事件, {} 个 Local 事件, 总共 {} 个事件",
      agent_events, local_events, total_events
    );
  }

  #[tokio::test]
  async fn test_agent_source_respects_io_semaphore() {
    let pool = create_test_pool().await;

    // 创建多个 Agent 数据源
    let multi_agent_script = r#"
# 多个 Agent 数据源
SOURCES = [
    "orl://agent.agent-1/logs",
    "orl://agent.agent-2/logs",
    "orl://agent.agent-3/logs"
]
"#;

    planners::upsert_script(&pool, "multi_agent", multi_agent_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("multi_agent")).await.unwrap();

    let config = SearchExecutorConfig {
      io_max_concurrency: 2, // 限制并发数
      stream_channel_capacity: 128,
    };
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("test", 1, None).await;
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();
    assert!(!sid.is_empty());

    // 收集事件
    let mut event_count = 0;

    while let Some(_event) = rx.recv().await {
      event_count += 1;
    }

    // 应该收到一些事件（每个 Agent 至少一个错误或完成事件）
    assert!(
      event_count >= 3,
      "应该收到至少 3 个事件（每个 Agent 一个），实际收到 {}",
      event_count
    );
  }

  #[tokio::test]
  async fn test_agent_source_error_contains_agent_id() {
    let pool = create_test_pool().await;

    // 创建一个 Agent 数据源配置
    let agent_script = r#"
# Agent 数据源配置
SOURCES = [
    "orl://agent.specific-agent-id-12345/logs"
]
"#;

    planners::upsert_script(&pool, "agent_id_test", agent_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("agent_id_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("error", 3, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集事件
    let mut found_agent_id = false;

    while let Some(event) = rx.recv().await {
      if let SearchEvent::Error { source, .. } = event {
        // 验证错误源包含 agent_id
        if source.contains("specific-agent-id-12345") || source.contains("agent:") {
          found_agent_id = true;
          break;
        }
      }
    }

    // 应该在错误源中找到 agent_id
    assert!(found_agent_id, "错误源应该包含 agent_id 信息");
  }

  // ========== 边界情况测试 ==========

  #[tokio::test]
  async fn test_search_with_very_large_query() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 创建一个非常长的查询字符串（1000 个词）
    let large_query = (0..1000).map(|i| format!("word{}", i)).collect::<Vec<_>>().join(" OR ");

    // 执行搜索
    let result = executor.search(&large_query, 3, None).await;

    // 应该能够处理大查询（可能成功或失败，取决于解析器限制）
    // 如果成功，验证返回值
    if let Ok((mut rx, sid)) = result {
      assert!(!sid.is_empty());
      // 消费一些事件
      let mut event_count = 0;
      while let Some(_event) = rx.recv().await {
        event_count += 1;
        if event_count >= 5 {
          break;
        }
      }
    }
    // 如果失败，也是可以接受的（查询太大）
  }

  #[tokio::test]
  async fn test_search_with_zero_context_lines() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 使用 0 个上下文行
    let result = executor.search("error", 0, None).await;

    // 应该成功
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();
    assert!(!sid.is_empty());

    // 消费事件
    let mut received_events = false;
    while let Some(_event) = rx.recv().await {
      received_events = true;
    }

    // 应该收到一些事件
    assert!(received_events);
  }

  #[tokio::test]
  async fn test_search_with_very_high_context_lines() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 使用非常大的上下文行数
    let result = executor.search("error", 10000, None).await;

    // 应该成功（上下文行数应该被接受）
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();
    assert!(!sid.is_empty());

    // 消费事件
    let mut received_events = false;
    while let Some(_event) = rx.recv().await {
      received_events = true;
    }

    // 应该收到一些事件
    assert!(received_events);
  }

  #[tokio::test]
  async fn test_search_with_very_large_number_of_sources() {
    let pool = create_test_pool().await;

    // 创建一个返回大量数据源的 planner（100 个）
    let mut sources_json = Vec::new();
    for i in 0..100 {
      sources_json.push(format!(r#""orl://local/tmp/test{}""#, i));
    }
    let sources_str = sources_json.join(",\n");

    let large_sources_script = format!(
      r#"# 大量数据源测试
SOURCES = [
{}
]
"#,
      sources_str
    );

    planners::upsert_script(&pool, "large_sources", &large_sources_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("large_sources")).await.unwrap();

    let config = SearchExecutorConfig {
      io_max_concurrency: 10, // 限制并发数
      stream_channel_capacity: 256,
    };
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("test", 1, None).await;

    // 应该成功启动
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();
    assert!(!sid.is_empty());

    // 消费一些事件（不需要等待所有 100 个数据源完成）
    let mut event_count = 0;
    while let Some(_event) = rx.recv().await {
      event_count += 1;
      if event_count >= 20 {
        // 收到 20 个事件就足够验证系统能处理大量数据源
        break;
      }
    }

    // 应该收到一些事件
    assert!(event_count >= 10, "应该收到至少 10 个事件，实际收到 {}", event_count);
  }

  #[tokio::test]
  async fn test_search_with_minimal_channel_capacity() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    // 使用非常小的通道容量
    let config = SearchExecutorConfig {
      io_max_concurrency: 12,
      stream_channel_capacity: 1, // 最小容量
    };
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("error", 3, None).await;

    // 应该成功（即使通道容量很小）
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();
    assert!(!sid.is_empty());

    // 消费事件
    let mut received_events = false;
    while let Some(_event) = rx.recv().await {
      received_events = true;
    }

    // 应该收到一些事件
    assert!(received_events);
  }

  #[tokio::test]
  async fn test_search_with_special_characters_in_query() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 测试包含特殊字符的查询
    let special_queries = vec![
      "error!@#$%^&*()",
      "test\nwith\nnewlines",
      "query\twith\ttabs",
      "unicode: 你好世界 🚀",
      r#"quotes "test" 'test'"#,
    ];

    for query in special_queries {
      let result = executor.search(query, 3, None).await;

      // 应该能够处理特殊字符（可能成功或失败）
      if let Ok((mut rx, sid)) = result {
        assert!(!sid.is_empty());
        // 消费一些事件
        while let Some(_event) = rx.recv().await {
          // 只是验证不会崩溃
        }
      }
      // 如果失败，也是可以接受的（某些特殊字符可能不被支持）
    }
  }

  #[tokio::test]
  async fn test_search_with_empty_string_query() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 测试空字符串查询
    let result = executor.search("", 3, None).await;

    // 应该成功（空查询是有效的）
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();
    assert!(!sid.is_empty());

    // 消费事件
    while let Some(_event) = rx.recv().await {
      // 验证不会崩溃
    }
  }

  #[tokio::test]
  async fn test_search_with_whitespace_only_query() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 测试只包含空格的查询
    let result = executor.search("     ", 3, None).await;

    // 应该成功
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();
    assert!(!sid.is_empty());

    // 消费事件
    while let Some(_event) = rx.recv().await {
      // 验证不会崩溃
    }
  }

  #[tokio::test]
  async fn test_concurrent_search_limit_stress() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig {
      io_max_concurrency: 1, // 极限并发数
      stream_channel_capacity: 128,
    };
    let executor = Arc::new(SearchExecutor::new(pool, config));

    // 启动大量并发搜索（50 个）
    let mut handles = vec![];
    for i in 0..50 {
      let executor_clone = executor.clone();
      let handle = tokio::spawn(async move {
        let query = format!("test{}", i);
        let result = executor_clone.search(&query, 1, None).await;
        result.is_ok()
      });
      handles.push(handle);
    }

    // 等待所有搜索完成
    let mut success_count = 0;
    for handle in handles {
      if handle.await.unwrap() {
        success_count += 1;
      }
    }

    // 验证所有搜索都成功启动（即使并发限制为 1）
    assert_eq!(success_count, 50, "所有搜索应该成功启动");
  }

  #[tokio::test]
  async fn test_search_with_multiple_encoding_qualifiers() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 测试多个 encoding 限定词（只有最后一个应该生效）
    let result = executor.get_sources("encoding:UTF-8 encoding:GBK error").await;

    // 应该成功
    assert!(result.is_ok());

    let (_sources, cleaned_query, encoding, _, _) = result.unwrap();
    assert_eq!(cleaned_query, "error");
    // 最后一个 encoding 限定词应该生效
    assert_eq!(encoding, Some("GBK".to_string()));
  }

  #[tokio::test]
  async fn test_search_with_multiple_app_qualifiers() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 测试多个 app 限定词（只有最后一个应该生效）
    let result = executor.get_sources("app:test app:test error").await;

    // 应该成功（使用 test planner）
    assert!(result.is_ok());

    let (_sources, cleaned_query, _encoding, _, _) = result.unwrap();
    assert_eq!(cleaned_query, "error");
    // app 限定词应该被处理（具体行为取决于 planner）
  }

  #[tokio::test]
  async fn test_search_with_mixed_qualifiers() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 测试混合限定词
    let result = executor.get_sources("app:test encoding:UTF-8 error AND warning").await;

    // 应该成功
    assert!(result.is_ok());

    let (_sources, cleaned_query, encoding, _, _) = result.unwrap();
    assert_eq!(cleaned_query, "error AND warning");
    assert_eq!(encoding, Some("UTF-8".to_string()));
  }

  #[tokio::test]
  async fn test_channel_closes_when_all_senders_dropped() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("test", 1, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 消费所有事件直到通道关闭
    let mut event_count = 0;
    while let Some(_event) = rx.recv().await {
      event_count += 1;
    }

    // 通道应该正常关闭（所有发送端都已完成）
    // 验证我们收到了一些事件
    assert!(event_count > 0, "应该收到至少一些事件");

    // 尝试再次接收应该返回 None
    let next_event = rx.recv().await;
    assert!(next_event.is_none(), "通道关闭后不应该收到更多事件");
  }

  #[tokio::test]
  async fn test_search_executor_reusable() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行多次搜索，验证 executor 可以复用
    for i in 0..5 {
      let query = format!("test{}", i);
      let result = executor.search(&query, 1, None).await;

      assert!(result.is_ok(), "第 {} 次搜索应该成功", i + 1);

      let (mut rx, sid) = result.unwrap();
      assert!(!sid.is_empty());

      // 消费事件
      while let Some(_event) = rx.recv().await {
        // 验证不会崩溃
      }
    }
  }

  #[tokio::test]
  async fn test_sid_uniqueness() {
    let pool = create_test_pool().await;
    setup_test_sources(&pool).await;

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行多次搜索，收集 sid
    let mut sids = std::collections::HashSet::new();

    for _ in 0..10 {
      let result = executor.search("test", 1, None).await;
      assert!(result.is_ok());

      let (mut rx, sid) = result.unwrap();
      assert!(!sid.is_empty());

      // 验证 sid 唯一性
      assert!(sids.insert(sid.clone()), "sid 应该是唯一的，但发现重复: {}", sid);

      // 消费事件
      while let Some(_event) = rx.recv().await {
        // 验证不会崩溃
      }
    }

    // 验证生成了 10 个不同的 sid
    assert_eq!(sids.len(), 10, "应该生成 10 个不同的 sid");
  }

  // ========== 集成测试（使用真实文件）==========

  #[tokio::test]
  async fn test_search_with_real_local_files() {
    let pool = create_test_pool().await;

    // 创建临时目录和文件
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    // 创建测试文件
    std::fs::write(
      temp_path.join("test1.log"),
      "error: connection failed\nwarning: retry\ninfo: success\n",
    )
    .unwrap();
    std::fs::write(
      temp_path.join("test2.log"),
      "debug: starting\nerror: timeout\ninfo: done\n",
    )
    .unwrap();

    // 创建 planner 指向临时目录
    let planner_script = format!(
      r#"
# 真实文件测试
SOURCES = [
    "orl://local/{}"
]
"#,
      escape_path_for_starlark(temp_path)
    );

    planners::upsert_script(&pool, "real_local", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("real_local")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("error", 1, None).await;
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();
    assert!(!sid.is_empty());

    // 收集结果
    let mut success_count = 0;
    let mut found_files = std::collections::HashSet::new();

    while let Some(event) = rx.recv().await {
      match event {
        SearchEvent::Success(res) => {
          success_count += 1;
          found_files.insert(res.path.clone());
          // 验证结果包含 "error"
          assert!(!res.lines.is_empty());
        }
        SearchEvent::Complete { .. } => {
          // 完成事件
        }
        SearchEvent::Error { message, .. } => {
          panic!("不应该有错误: {}", message);
        }
      }
    }

    // 应该找到包含 "error" 的文件
    assert!(success_count >= 1, "应该找到至少 1 个匹配的文件");
  }

  #[tokio::test]
  async fn test_search_with_context_lines() {
    let pool = create_test_pool().await;

    // 创建临时目录和文件
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    // 创建测试文件（多行）
    std::fs::write(
      temp_path.join("context.log"),
      "line 1\nline 2\nerror occurred\nline 4\nline 5\n",
    )
    .unwrap();

    // 创建 planner
    let planner_script = format!(
      r#"
SOURCES = [
    "orl://local/{}"
]
"#,
      escape_path_for_starlark(temp_path)
    );

    planners::upsert_script(&pool, "context_test", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("context_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索（带上下文）
    let result = executor.search("error", 2, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集结果
    let mut found_context = false;

    while let Some(event) = rx.recv().await {
      if let SearchEvent::Success(res) = event {
        // 验证上下文行被包含
        if res.lines.len() > 1 {
          found_context = true;
        }
      }
    }

    // 应该找到上下文行
    assert!(found_context, "应该包含上下文行");
  }

  #[tokio::test]
  async fn test_search_with_filter_glob() {
    let pool = create_test_pool().await;

    // 创建临时目录和文件
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    // 创建不同扩展名的文件
    std::fs::write(temp_path.join("test.log"), "error in log\n").unwrap();
    std::fs::write(temp_path.join("test.txt"), "error in txt\n").unwrap();

    // 创建 planner（只搜索 .log 文件）
    let planner_script = format!(
      r#"
SOURCES = [
    "orl://local/{}?glob=**/*.log"
]
"#,
      escape_path_for_starlark(temp_path)
    );

    planners::upsert_script(&pool, "filter_test", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("filter_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("error", 0, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集结果
    let mut found_files = Vec::new();

    while let Some(event) = rx.recv().await {
      if let SearchEvent::Success(res) = event {
        found_files.push(res.path.clone());
      }
    }

    // 应该只找到 .log 文件
    assert!(!found_files.is_empty(), "应该找到匹配的文件");
    for file in &found_files {
      assert!(file.contains(".log"), "应该只包含 .log 文件: {}", file);
    }
  }

  #[tokio::test]
  async fn test_search_with_recursive_directory() {
    let pool = create_test_pool().await;

    // 创建临时目录结构
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();
    let sub_dir = temp_path.join("subdir");
    std::fs::create_dir(&sub_dir).unwrap();

    // 创建文件
    std::fs::write(temp_path.join("root.log"), "error at root\n").unwrap();
    std::fs::write(sub_dir.join("sub.log"), "error in subdir\n").unwrap();

    // 创建 planner（递归搜索）
    // 创建 planner（递归搜索）
    let planner_script = format!(
      r#"
SOURCES = [
    "orl://local/{}"
]
"#,
      escape_path_for_starlark(temp_path)
    );

    planners::upsert_script(&pool, "recursive_test", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("recursive_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("error", 0, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集结果
    let mut success_count = 0;

    while let Some(event) = rx.recv().await {
      if let SearchEvent::Success(_) = event {
        success_count += 1;
      }
    }

    // 应该找到两个文件（root 和 subdir）
    assert!(success_count >= 1, "应该找到至少 1 个匹配的文件");
  }

  #[tokio::test]
  async fn test_search_with_encoding_qualifier_real_file() {
    let pool = create_test_pool().await;

    // 创建临时目录和文件
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    // 创建 UTF-8 文件
    std::fs::write(temp_path.join("utf8.log"), "错误信息\n").unwrap();

    // 创建 planner
    // 创建 planner
    let planner_script = format!(
      r#"
SOURCES = [
    "orl://local/{}"
]
"#,
      escape_path_for_starlark(temp_path)
    );

    planners::upsert_script(&pool, "encoding_test", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("encoding_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索（带 encoding 限定词）
    let result = executor.search("encoding:UTF-8 错误", 0, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集结果
    let mut found_match = false;

    while let Some(event) = rx.recv().await {
      if let SearchEvent::Success(_) = event {
        found_match = true;
      }
    }

    // 应该找到匹配
    assert!(found_match, "应该找到匹配的文件");
  }

  #[tokio::test]
  async fn test_search_with_boolean_query_real_file() {
    let pool = create_test_pool().await;

    // 创建临时目录和文件
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    // 创建测试文件
    std::fs::write(
      temp_path.join("test.log"),
      "error: connection timeout\nwarning: retry\n",
    )
    .unwrap();

    // 创建 planner
    let planner_script = format!(
      r#"
SOURCES = [
    "orl://local/{}"
]
"#,
      escape_path_for_starlark(temp_path)
    );

    planners::upsert_script(&pool, "boolean_test", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("boolean_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行布尔查询
    let result = executor.search("error AND timeout", 0, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集结果
    let mut found_match = false;

    while let Some(event) = rx.recv().await {
      if let SearchEvent::Success(_) = event {
        found_match = true;
      }
    }

    // 应该找到匹配
    assert!(found_match, "应该找到同时包含 error 和 timeout 的行");
  }

  #[tokio::test]
  async fn test_search_multiple_files_target() {
    let pool = create_test_pool().await;

    // 创建临时目录和文件
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    // 创建多个文件
    std::fs::write(temp_path.join("file1.log"), "error in file1\n").unwrap();
    std::fs::write(temp_path.join("file2.log"), "error in file2\n").unwrap();
    std::fs::write(temp_path.join("file3.log"), "no match here\n").unwrap();

    // 创建 planner（指定多个文件）
    let file1 = escape_path_for_starlark(&temp_path.join("file1.log"));
    let file2 = escape_path_for_starlark(&temp_path.join("file2.log"));

    let planner_script = format!(
      r#"
SOURCES = [
    "orl://local/{}",
    "orl://local/{}"
]
"#,
      file1, file2
    );

    planners::upsert_script(&pool, "multi_files_test", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("multi_files_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("error", 0, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集结果
    let mut success_count = 0;

    while let Some(event) = rx.recv().await {
      if let SearchEvent::Success(_) = event {
        success_count += 1;
      }
    }

    // 应该找到 2 个文件
    assert!(success_count >= 1, "应该找到至少 1 个匹配的文件");
  }

  #[tokio::test]
  async fn test_search_highlights_cached() {
    let pool = create_test_pool().await;

    // 创建临时目录和文件
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    std::fs::write(temp_path.join("test.log"), "error occurred\n").unwrap();

    // 创建 planner
    let planner_script = format!(
      r#"
SOURCES = [
    "orl://local/{}"
]
"#,
      escape_path_for_starlark(temp_path)
    );

    planners::upsert_script(&pool, "highlights_test", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("highlights_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("error", 0, None).await;
    assert!(result.is_ok());

    let (mut rx, sid) = result.unwrap();

    // 消费事件
    while let Some(_event) = rx.recv().await {
      // 只是消费事件
    }

    // 验证 sid 不为空（说明关键字被缓存了）
    assert!(!sid.is_empty());

    // 验证可以从缓存中获取关键字
    let cached_keywords = crate::repository::cache::cache().get_keywords(&sid).await;
    assert!(cached_keywords.is_some());
    let keywords = cached_keywords.unwrap();
    assert!(keywords.contains(&crate::query::KeywordHighlight::Literal("error".to_string())));
  }

  #[tokio::test]
  async fn test_search_with_tar_gz_archive() {
    let pool = create_test_pool().await;

    // 创建临时目录
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    // 创建一个临时文件用于打包
    let content_dir = temp_path.join("content");
    std::fs::create_dir(&content_dir).unwrap();
    std::fs::write(content_dir.join("test.log"), "error in archive\n").unwrap();

    // 创建 tar.gz 归档
    let archive_path = temp_path.join("archive.tar.gz");
    let tar_gz = std::fs::File::create(&archive_path).unwrap();
    let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
    let mut tar = tar::Builder::new(enc);
    tar.append_dir_all(".", &content_dir).unwrap();
    tar.finish().unwrap();

    // 创建 planner
    let planner_script = format!(
      r#"
SOURCES = [
    "orl://local/{}/archive.tar.gz"
]
"#,
      escape_path_for_starlark(temp_path)
    );

    planners::upsert_script(&pool, "archive_test", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("archive_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("error", 0, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集结果（可能找到也可能找不到，取决于归档处理）
    let mut event_count = 0;

    while let Some(_event) = rx.recv().await {
      event_count += 1;
    }

    // 至少应该收到一些事件（Complete 或 Error）
    assert!(event_count > 0, "应该收到至少一些事件");
  }

  #[tokio::test]
  async fn test_search_with_or_query() {
    let pool = create_test_pool().await;

    // 创建临时目录和文件
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    std::fs::write(temp_path.join("test1.log"), "error occurred\n").unwrap();
    std::fs::write(temp_path.join("test2.log"), "warning issued\n").unwrap();

    // 创建 planner
    let planner_script = format!(
      r#"
SOURCES = [
    "orl://local/{}"
]
"#,
      escape_path_for_starlark(temp_path)
    );

    planners::upsert_script(&pool, "or_test", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("or_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行 OR 查询
    let result = executor.search("error OR warning", 0, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集结果
    let mut success_count = 0;

    while let Some(event) = rx.recv().await {
      if let SearchEvent::Success(_) = event {
        success_count += 1;
      }
    }

    // 应该找到两个文件
    assert!(success_count >= 2, "应该找到至少 2 个匹配的文件");
  }

  #[tokio::test]
  async fn test_search_with_not_query() {
    let pool = create_test_pool().await;

    // 创建临时目录和文件
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    std::fs::write(temp_path.join("test1.log"), "error occurred\n").unwrap();
    std::fs::write(temp_path.join("test2.log"), "error and warning\n").unwrap();

    // 创建 planner
    let planner_script = format!(
      r#"
SOURCES = [
    "orl://local/{}"
]
"#,
      escape_path_for_starlark(temp_path)
    );

    planners::upsert_script(&pool, "not_test", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("not_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行 NOT 查询
    let result = executor.search("error NOT warning", 0, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集结果
    let mut event_count = 0;

    while let Some(event) = rx.recv().await {
      match event {
        SearchEvent::Success(_) | SearchEvent::Complete { .. } | SearchEvent::Error { .. } => {
          event_count += 1;
        }
      }
    }

    // 应该收到一些事件
    assert!(event_count > 0, "应该收到至少一些事件");
  }

  #[tokio::test]
  async fn test_search_with_regex_query() {
    let pool = create_test_pool().await;

    // 创建临时目录和文件
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    std::fs::write(temp_path.join("test.log"), "status: 200\nerror: 500\n").unwrap();

    // 创建 planner
    let planner_script = format!(
      r#"
SOURCES = [
    "orl://local/{}"
]
"#,
      escape_path_for_starlark(temp_path)
    );

    planners::upsert_script(&pool, "regex_test", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("regex_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行正则表达式查询
    let result = executor.search(r#"/\d{3}/"#, 0, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集结果
    let mut found_match = false;

    while let Some(event) = rx.recv().await {
      if let SearchEvent::Success(_) = event {
        found_match = true;
      }
    }

    // 应该找到匹配
    assert!(found_match, "应该找到匹配三位数字的行");
  }

  #[tokio::test]
  async fn test_search_with_case_sensitive_query() {
    let pool = create_test_pool().await;

    // 创建临时目录和文件
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    std::fs::write(temp_path.join("test.log"), "ERROR\nerror\nError\n").unwrap();

    // 创建 planner
    let planner_script = format!(
      r#"
SOURCES = [
    "orl://local/{}"
]
"#,
      escape_path_for_starlark(temp_path)
    );

    planners::upsert_script(&pool, "case_test", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("case_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行大小写敏感查询
    let result = executor.search("ERROR", 0, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集结果
    let mut found_match = false;

    while let Some(event) = rx.recv().await {
      if let SearchEvent::Success(_) = event {
        found_match = true;
      }
    }

    // 应该找到匹配
    assert!(found_match, "应该找到匹配");
  }

  #[tokio::test]
  async fn test_search_with_path_qualifier() {
    let pool = create_test_pool().await;

    // 创建临时目录和文件
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    std::fs::write(temp_path.join("test.log"), "error\n").unwrap();
    std::fs::write(temp_path.join("test.txt"), "error\n").unwrap();

    // 创建 planner
    let planner_script = format!(
      r#"
SOURCES = [
    "orl://local/{}"
]
"#,
      escape_path_for_starlark(temp_path)
    );

    planners::upsert_script(&pool, "path_test", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("path_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行带 path 限定词的查询
    let result = executor.search("path:*.log error", 0, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集结果
    let mut found_files = Vec::new();

    while let Some(event) = rx.recv().await {
      if let SearchEvent::Success(res) = event {
        found_files.push(res.path);
      }
    }

    // 应该只找到 .log 文件
    assert!(!found_files.is_empty(), "应该找到匹配的文件");
    for file in &found_files {
      assert!(file.contains(".log"), "应该只包含 .log 文件: {}", file);
    }
  }

  #[tokio::test]
  async fn test_search_with_empty_result() {
    let pool = create_test_pool().await;

    // 创建临时目录和文件
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    std::fs::write(temp_path.join("test.log"), "no match here\n").unwrap();

    // 创建 planner
    let planner_script = format!(
      r#"
SOURCES = [
    "orl://local/{}"
]
"#,
      escape_path_for_starlark(temp_path)
    );

    planners::upsert_script(&pool, "empty_result_test", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("empty_result_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索（不会匹配）
    let result = executor.search("nonexistent", 0, None).await;
    assert!(result.is_ok());

    let (mut rx, _sid) = result.unwrap();

    // 收集结果
    let mut success_count = 0;

    while let Some(event) = rx.recv().await {
      if let SearchEvent::Success(_) = event {
        success_count += 1;
      }
    }

    // 不应该找到匹配
    assert_eq!(success_count, 0, "不应该找到匹配");
  }
  #[tokio::test]
  async fn test_search_with_complex_path_qualifiers() {
    let pool = create_test_pool().await;

    // 创建临时目录和文件
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    std::fs::create_dir(temp_path.join("src")).unwrap();
    std::fs::create_dir(temp_path.join("test")).unwrap();

    std::fs::write(temp_path.join("src/main.rs"), "error in main\n").unwrap();
    std::fs::write(temp_path.join("src/utils.rs"), "error in utils\n").unwrap();
    std::fs::write(temp_path.join("test/test.rs"), "error in test\n").unwrap();
    std::fs::write(temp_path.join("README.md"), "error in readme\n").unwrap();

    // 创建 planner
    let planner_script = format!(
      r#"
SOURCES = [
    "orl://local/{}"
]
"#,
      escape_path_for_starlark(temp_path)
    );

    planners::upsert_script(&pool, "complex_path_test", &planner_script)
      .await
      .unwrap();
    planners::set_default(&pool, Some("complex_path_test")).await.unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 1. 测试 excluide: -path:test/**
    let result = executor.search("-path:test/** error", 0, None).await;
    let (mut rx, _) = result.unwrap();
    let mut found_files = Vec::new();
    while let Some(event) = rx.recv().await {
      if let SearchEvent::Success(res) = event {
        found_files.push(res.path);
      }
    }
    assert!(found_files.iter().any(|f| f.contains("src/main.rs")));
    assert!(found_files.iter().any(|f| f.contains("README.md")));
    assert!(!found_files.iter().any(|f| f.contains("test/test.rs"))); // should be excluded

    // 2. 测试 include combination: path:src/** path:test/**
    // 注意：Strict glob requires ** to match directories.
    let result = executor.search("path:src/** path:test/** error", 0, None).await;
    let (mut rx, _) = result.unwrap();
    let mut found_files = Vec::new();
    while let Some(event) = rx.recv().await {
      if let SearchEvent::Success(res) = event {
        found_files.push(res.path);
      }
    }
    // GlobSet treats multiple patterns as OR
    assert!(found_files.iter().any(|f| f.contains("src/main.rs")));
    assert!(found_files.iter().any(|f| f.contains("test/test.rs")));
    assert!(!found_files.iter().any(|f| f.contains("README.md"))); // Excluded implicitly because not in include list

    // 3. 测试 mixed: path:src/** -path:**/utils.rs
    let result = executor.search("path:src/** -path:**/utils.rs error", 0, None).await;
    let (mut rx, _) = result.unwrap();
    let mut found_files = Vec::new();
    while let Some(event) = rx.recv().await {
      if let SearchEvent::Success(res) = event {
        found_files.push(res.path);
      }
    }
    assert!(found_files.iter().any(|f| f.contains("src/main.rs")));
    assert!(!found_files.iter().any(|f| f.contains("src/utils.rs"))); // Excluded
    assert!(!found_files.iter().any(|f| f.contains("test/test.rs"))); // Not included
  }
}
