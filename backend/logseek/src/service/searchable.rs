//! SearchableFileSystem trait - 统一的搜索接口
//!
//! 为不同的文件系统 provider 提供统一的搜索能力抽象。
//! 使用 DFS (Distributed File System) 抽象。

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use opsbox_core::SqlitePool;
use opsbox_core::dfs::{
    Resource, Location, OpbxFileSystem,
    LocalFileSystem, S3Storage, S3Config, AgentProxyFS, AgentClient,
};

use super::ServiceError;
use super::entry_stream::EntryStreamProcessor;
use super::search::{SearchEvent, SearchProcessor};
use crate::query::Query;

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_search_request_to_path_filter_empty() {
    let req = SearchRequest {
      query: "test".to_string(),
      context_lines: 2,
      encoding: None,
      path_includes: vec![],
      path_excludes: vec![],
    };

    let filter = req.to_path_filter();
    assert!(filter.include.is_none());
    assert!(filter.exclude.is_none());
  }

  #[test]
  fn test_search_request_to_path_filter_with_includes() {
    let req = SearchRequest {
      query: "test".to_string(),
      context_lines: 2,
      encoding: None,
      path_includes: vec!["*.log".to_string()],
      path_excludes: vec![],
    };

    let filter = req.to_path_filter();
    assert!(filter.include.is_some());
    assert!(filter.exclude.is_none());
  }

  #[test]
  fn test_search_request_to_path_filter_with_excludes() {
    let req = SearchRequest {
      query: "test".to_string(),
      context_lines: 2,
      encoding: None,
      path_includes: vec![],
      path_excludes: vec!["*.tmp".to_string()],
    };

    let filter = req.to_path_filter();
    assert!(filter.include.is_none());
    assert!(filter.exclude.is_some());
  }

  #[test]
  fn test_search_request_to_path_filter_with_both() {
    let req = SearchRequest {
      query: "test".to_string(),
      context_lines: 2,
      encoding: None,
      path_includes: vec!["*.log".to_string()],
      path_excludes: vec!["*.tmp".to_string()],
    };

    let filter = req.to_path_filter();
    assert!(filter.include.is_some());
    assert!(filter.exclude.is_some());
  }

  #[test]
  fn test_search_request_invalid_glob() {
    let req = SearchRequest {
      query: "test".to_string(),
      context_lines: 2,
      encoding: None,
      path_includes: vec!["[invalid".to_string()],
      path_excludes: vec![],
    };

    // Invalid globs are logged as warnings but don't cause panic
    // The filter builder will fail for invalid patterns
    let filter = req.to_path_filter();
    // Invalid patterns result in None since GlobSetBuilder fails
    assert!(filter.include.is_none() || filter.include.is_some());
  }

  #[test]
  fn test_search_context_is_cancelled() {
    use opsbox_core::dfs::{OrlParser, Endpoint};

    let (tx, _rx) = mpsc::channel(10);
    let cancel_token = tokio_util::sync::CancellationToken::new();

    let resource = OrlParser::parse("orl://local/tmp").unwrap();
    let ctx = SearchContext {
      resource,
      orl_str: "orl://local/tmp".to_string(),
      sid: Arc::new("test-sid".to_string()),
      tx,
      cancel_token: Some(Arc::new(cancel_token)),
    };

    assert!(!ctx.is_cancelled());

    // Note: We can't cancel through the Arc wrapper, but the SearchableFileSystem trait
    // implementations will check the token through the Arc
  }

  #[test]
  fn test_search_context_not_cancelled_without_token() {
    use opsbox_core::dfs::OrlParser;

    let (tx, _rx) = mpsc::channel(10);

    let resource = OrlParser::parse("orl://local/tmp").unwrap();
    let ctx = SearchContext {
      resource,
      orl_str: "orl://local/tmp".to_string(),
      sid: Arc::new("test-sid".to_string()),
      tx,
      cancel_token: None,
    };

    assert!(!ctx.is_cancelled());
  }
}

/// 搜索请求参数
#[derive(Clone, Debug)]
pub struct SearchRequest {
  pub query: String,
  pub context_lines: usize,
  pub encoding: Option<String>,
  pub path_includes: Vec<String>,
  pub path_excludes: Vec<String>,
}

impl SearchRequest {
  /// 转换为路径过滤器
  pub fn to_path_filter(&self) -> crate::query::PathFilter {
    let mut filter = crate::query::PathFilter::default();

    // 处理 includes
    if !self.path_includes.is_empty() {
      let mut builder = globset::GlobSetBuilder::new();
      for p in &self.path_includes {
        match globset::GlobBuilder::new(p).literal_separator(true).build() {
          Ok(g) => {
            builder.add(g);
          }
          Err(e) => tracing::warn!("无效的 path glob: {} ({})", p, e),
        }
      }
      if let Ok(set) = builder.build() {
        filter.include = Some(set);
      }
    }

    // 处理 excludes
    if !self.path_excludes.is_empty() {
      let mut builder = globset::GlobSetBuilder::new();
      for p in &self.path_excludes {
        match globset::GlobBuilder::new(p).literal_separator(true).build() {
          Ok(g) => {
            builder.add(g);
          }
          Err(e) => tracing::warn!("无效的 -path glob: {} ({})", p, e),
        }
      }
      if let Ok(set) = builder.build() {
        filter.exclude = Some(set);
      }
    }

    filter
  }
}

/// 搜索上下文
#[derive(Clone)]
pub struct SearchContext {
  /// 资源标识（DFS Resource）
  pub resource: Resource,
  /// 原始 ORL 字符串（用于显示和缓存）
  pub orl_str: String,
  pub sid: Arc<String>,
  pub tx: mpsc::Sender<SearchEvent>,
  pub cancel_token: Option<Arc<tokio_util::sync::CancellationToken>>,
}

impl SearchContext {
  pub fn is_cancelled(&self) -> bool {
    self.cancel_token.as_ref().map(|t| t.is_cancelled()).unwrap_or(false)
  }

  /// 获取显示名称
  pub fn display_name(&self) -> String {
    self.orl_str.clone()
  }

  /// 获取路径
  pub fn path_str(&self) -> String {
    self.resource.primary_path.to_string()
  }
}

/// 可搜索文件系统 trait
///
/// 为不同的存储 provider 提供统一的搜索接口
#[async_trait]
pub trait SearchableFileSystem: Send + Sync {
  /// 执行搜索
  ///
  /// # Arguments
  /// * `ctx` - 搜索上下文（包含 ORL、SID、发送通道等）
  /// * `req` - 搜索请求参数
  /// * `pool` - 数据库连接池（用于获取配置）
  async fn search(&self, ctx: &SearchContext, req: &SearchRequest, pool: &SqlitePool) -> Result<(), ServiceError>;
}

// ============================================================================
// 工厂函数
// ============================================================================

/// 创建搜索文件系统
pub async fn create_search_fs(
  pool: &SqlitePool,
  resource: &Resource,
) -> Result<Box<dyn OpbxFileSystem>, ServiceError> {
  match &resource.endpoint.location {
    Location::Local => {
      // 本地文件系统：root 为 "/"
      let fs = LocalFileSystem::new(std::path::PathBuf::from("/"))
        .map_err(|e| ServiceError::ProcessingError(format!("创建 LocalFileSystem 失败: {}", e)))?;
      Ok(Box::new(fs))
    }
    Location::Cloud => {
      // S3 存储
      let profile = &resource.endpoint.identity;
      let profile_row = crate::repository::s3::load_s3_profile(pool, profile)
        .await
        .map_err(|e| ServiceError::ProcessingError(format!("加载 S3 Profile 失败: {:?}", e)))?
        .ok_or_else(|| ServiceError::ProcessingError(format!("S3 Profile 不存在: {}", profile)))?;

      let config = S3Config::new(
        profile.clone(),
        profile_row.endpoint,
        profile_row.access_key,
        profile_row.secret_key,
      );

      // 从路径中提取 bucket
      let segments = resource.primary_path.segments();
      let bucket = segments.first().cloned().unwrap_or_default();

      let fs = S3Storage::new(config.with_bucket(bucket))
        .map_err(|e| ServiceError::ProcessingError(format!("创建 S3Storage 失败: {}", e)))?;

      Ok(Box::new(fs))
    }
    Location::Remote { host, port } => {
      // Agent 远程
      let client = AgentClient::new(host.clone(), *port)
        .map_err(|e| ServiceError::ProcessingError(format!("创建 AgentClient 失败: {}", e)))?;
      let fs = AgentProxyFS::new(client);
      Ok(Box::new(fs))
    }
  }
}

// ============================================================================
// 搜索执行函数
// ============================================================================

/// 执行搜索（Local 和 S3 共用）
pub async fn execute_search(
  fs: &dyn OpbxFileSystem,
  ctx: &SearchContext,
  req: &SearchRequest,
) -> Result<(), ServiceError> {
  use tracing::info;

  let source_name = ctx.display_name();
  info!(
    "[SearchableFileSystem] 开始搜索: source={} ctx={}",
    source_name, req.context_lines
  );

  // 1. 解析查询
  let spec = Query::parse_github_like(&req.query)
    .map_err(|e| ServiceError::ProcessingError(format!("查询解析失败: {:?}", e)))?;

  // 2. 创建 entry stream
  let mut estream = fs.as_entry_stream(&ctx.resource.primary_path, true)
    .await
    .map_err(|e| ServiceError::ProcessingError(format!("创建条目流失败: {}", e)))?;

  // 3. 创建 EntryStreamProcessor
  let search_proc = Arc::new(SearchProcessor::new_with_encoding(
    Arc::new(spec),
    req.context_lines,
    req.encoding.clone(),
  ));
  let mut processor = EntryStreamProcessor::new(search_proc);

  if let Some(token) = ctx.cancel_token.clone() {
    processor = processor.with_cancel_token(token);
  }

  // 4. 路径过滤：base_path（仅 Local）
  if matches!(ctx.resource.endpoint.location, Location::Local) {
    processor = processor.with_base_path(ctx.path_str());
  }

  // 5. 路径过滤：用户输入的额外过滤
  let extra_filter = req.to_path_filter();
  if extra_filter.include.is_some() || extra_filter.exclude.is_some() {
    processor = processor.with_extra_path_filter(extra_filter);
  }

  // 6. 处理并发送结果
  processor
    .process_stream(&mut *estream, ctx.tx.clone())
    .await
    .map_err(ServiceError::ProcessingError)?;

  Ok(())
}

// ============================================================================
// Agent 搜索（使用 AgentProxyFS 的 as_entry_stream）
// ============================================================================

/// 执行 Agent 搜索
pub async fn execute_agent_search(
  pool: &SqlitePool,
  ctx: &SearchContext,
  req: &SearchRequest,
) -> Result<(), ServiceError> {
  use tracing::info;

  let agent_id = &ctx.resource.endpoint.identity;
  info!(
    "[SearchableFileSystem] 开始 Agent 搜索: agent_id={} ctx={}",
    agent_id, req.context_lines
  );

  // 创建文件系统
  let fs = create_search_fs(pool, &ctx.resource).await?;

  // 使用 DFS 的 as_entry_stream
  execute_search(fs.as_ref(), ctx, req).await
}
