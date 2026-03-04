//! 统一搜索执行器
//!
//! 为 Local/S3/Agent 提供统一的搜索执行入口，避免重复实现。
//!
//! # 设计目标
//! - 保留 Agent 的白名单/路径解析逻辑（安全边界由调用方负责）
//! - 统一 Local/S3/Agent 的"查询解析 + 路径过滤 + EntryStream 处理"内核
//! - 减少重复代码，避免行为漂移

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use opsbox_core::fs::EntryStream;

use super::entry_stream::EntryStreamProcessor;
use super::error::ServiceError;
use super::search::{SearchEvent, SearchProcessor};
use crate::query::{PathFilter, Query};

/// 搜索执行器配置
#[derive(Clone)]
pub struct SearchRunnerConfig {
  /// 搜索查询字符串
  pub query: String,
  /// 预解析的查询规范（可选，用于复用解析结果）
  pub query_spec: Option<Arc<Query>>,
  /// 上下文行数
  pub context_lines: usize,
  /// 指定编码（可选）
  pub encoding: Option<String>,
  /// 基础路径（用于相对路径过滤，可选）
  pub base_path: Option<PathBuf>,
  /// 额外的路径过滤器列表
  pub extra_filters: Vec<PathFilter>,
}

impl SearchRunnerConfig {
  /// 创建新的搜索配置
  pub fn new(query: impl Into<String>) -> Self {
    Self {
      query: query.into(),
      query_spec: None,
      context_lines: 0,
      encoding: None,
      base_path: None,
      extra_filters: Vec::new(),
    }
  }

  /// 设置上下文行数
  pub fn with_context_lines(mut self, lines: usize) -> Self {
    self.context_lines = lines;
    self
  }

  /// 设置编码（可选版本）
  pub fn with_encoding_opt(mut self, encoding: Option<String>) -> Self {
    self.encoding = encoding;
    self
  }

  /// 设置基础路径（可选版本）
  pub fn with_base_path_opt(mut self, path: Option<PathBuf>) -> Self {
    self.base_path = path;
    self
  }

  /// 添加多个额外路径过滤器
  pub fn with_extra_filters(mut self, filters: Vec<PathFilter>) -> Self {
    self.extra_filters.extend(filters);
    self
  }

  /// 设置预解析的查询规范（用于复用解析结果，避免重复解析）
  pub fn with_query_spec(mut self, spec: Arc<Query>) -> Self {
    self.query_spec = Some(spec);
    self
  }
}

/// 统一搜索执行入口
///
/// 该函数封装了"查询解析 + 创建处理器 + 路径过滤 + EntryStream 处理"的核心逻辑，
/// 供 Local/S3/Agent 三端复用。
///
/// # Arguments
/// * `entry_stream` - 条目流（由调用方创建）
/// * `config` - 搜索配置
/// * `result_tx` - 结果发送通道
/// * `cancel_token` - 取消令牌（可选）
/// * `source_name` - 来源名称（用于日志和错误信息）
///
/// # Returns
/// * `Ok(())` - 搜索完成
/// * `Err(ServiceError)` - 搜索过程中发生错误
///
/// # Example
/// ```ignore
/// use logseek::service::search_runner::{run_search, SearchRunnerConfig};
///
/// let config = SearchRunnerConfig::new("error")
///     .with_context_lines(2);
///
/// run_search(
///     &mut entry_stream,
///     config,
///     result_tx,
///     Some(cancel_token),
///     "LocalSearchProvider",
/// ).await?;
/// ```
pub async fn run_search(
  entry_stream: &mut dyn EntryStream,
  config: SearchRunnerConfig,
  result_tx: mpsc::Sender<SearchEvent>,
  cancel_token: Option<Arc<tokio_util::sync::CancellationToken>>,
  source_name: &str,
) -> Result<(), ServiceError> {
  let query_str = config.query.clone();
  info!(
    "[{}] 开始搜索: query={} ctx={}",
    source_name, query_str, config.context_lines
  );

  // 1. 获取查询规范（复用预解析的或重新解析）
  let spec = if let Some(pre_parsed) = config.query_spec {
    pre_parsed
  } else {
    Arc::new(
      Query::parse_github_like(&config.query)
        .map_err(|e| ServiceError::ProcessingError(format!("查询解析失败: {:?}", e)))?,
    )
  };

  // 2. 创建搜索处理器
  let processor = Arc::new(SearchProcessor::new_with_encoding(
    spec,
    config.context_lines,
    config.encoding.clone(),
  ));

  // 3. 创建 EntryStreamProcessor
  let mut stream_processor = EntryStreamProcessor::new(processor);

  // 4. 设置取消令牌
  if let Some(token) = cancel_token {
    stream_processor = stream_processor.with_cancel_token(token);
  }

  // 5. 设置基础路径（用于相对路径过滤）
  if let Some(ref base_path) = config.base_path {
    stream_processor = stream_processor.with_base_path(base_path.clone());
  }

  // 6. 添加额外路径过滤器
  for filter in &config.extra_filters {
    stream_processor = stream_processor.with_extra_path_filter(filter.clone());
  }

  // 7. 执行搜索
  let result = stream_processor.process_stream(entry_stream, result_tx).await;

  if let Err(e) = &result {
    warn!("[{}] 搜索执行失败: {}", source_name, e);
  } else {
    debug!("[{}] 搜索完成: query={}", source_name, query_str);
  }

  result.map_err(ServiceError::ProcessingError)
}

/// 从 glob 字符串和用户过滤构建路径过滤器列表
///
/// 该函数用于将 ORL 携带的 glob 过滤和用户输入的 path_includes/path_excludes
/// 统一转换为 PathFilter 列表。
///
/// # Arguments
/// * `orl_glob` - ORL 携带的 glob 过滤（可选）
/// * `path_includes` - 用户指定的包含模式列表
/// * `path_excludes` - 用户指定的排除模式列表
///
/// # Returns
/// * 路径过滤器列表
pub fn build_path_filters(
  orl_glob: Option<&str>,
  path_includes: &[String],
  path_excludes: &[String],
) -> Vec<PathFilter> {
  let mut filters = Vec::new();

  // ORL glob 过滤
  if let Some(glob) = orl_glob
    && let Ok(filter) = crate::query::path_glob_to_filter(glob)
  {
    filters.push(filter);
  }

  // 用户 path_includes/path_excludes
  if let Some(user_filter) = crate::query::combine_path_filters(path_includes, path_excludes) {
    filters.push(user_filter);
  }

  filters
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_search_runner_config_builder() {
    let config = SearchRunnerConfig::new("error")
      .with_context_lines(3)
      .with_encoding_opt(Some("GBK".to_string()))
      .with_base_path_opt(Some(PathBuf::from("/var/log")));

    assert_eq!(config.query, "error");
    assert_eq!(config.context_lines, 3);
    assert_eq!(config.encoding, Some("GBK".to_string()));
    assert_eq!(config.base_path, Some(PathBuf::from("/var/log")));
  }

  #[test]
  fn test_search_runner_config_with_filters() {
    let filter = PathFilter::default();
    let config = SearchRunnerConfig::new("test").with_extra_filters(vec![filter.clone()]);

    assert_eq!(config.extra_filters.len(), 1);
  }

  #[test]
  fn test_search_runner_config_with_query_spec() {
    let spec = Arc::new(Query::parse_github_like("error").unwrap());
    let config = SearchRunnerConfig::new("error").with_query_spec(spec.clone());

    assert!(config.query_spec.is_some());
    assert!(Arc::ptr_eq(config.query_spec.as_ref().unwrap(), &spec));
  }

  #[test]
  fn test_build_path_filters_empty() {
    let filters = build_path_filters(None, &[], &[]);
    assert!(filters.is_empty());
  }

  #[test]
  fn test_build_path_filters_with_orl_glob() {
    let filters = build_path_filters(Some("*.log"), &[], &[]);
    assert_eq!(filters.len(), 1);
  }

  #[test]
  fn test_build_path_filters_with_user_filters() {
    let filters = build_path_filters(None, &["*.log".to_string()], &["*.tmp".to_string()]);
    assert_eq!(filters.len(), 1);
    assert!(filters[0].include.is_some());
    assert!(filters[0].exclude.is_some());
  }

  #[test]
  fn test_build_path_filters_combined() {
    let filters = build_path_filters(Some("*.log"), &["error*".to_string()], &["*.tmp".to_string()]);
    assert_eq!(filters.len(), 2);
  }
}
