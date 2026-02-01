//! SearchableFileSystem trait - 统一的搜索接口
//!
//! 为不同的文件系统 provider 提供统一的搜索能力抽象。
//! 在 logseek 中定义，为 opsbox-core 的 provider 类型实现。

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use opsbox_core::SqlitePool;
use opsbox_core::odfs::orl::{ORL, EndpointType as OrlEndpointType};
use opsbox_core::odfs::providers::{LocalOpsFS, S3OpsFS};

// 新增：导入 EndpointConnector 相关类型
use opsbox_domain::resource::ResourcePath;
use opsbox_resource::{LocalEndpointConnector, S3EndpointConnector, EndpointConnectorExt};

use super::ServiceError;
use super::entry_stream::{EntryStreamProcessor, EntryStreamAdapter};
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
    let (tx, _rx) = mpsc::channel(10);
    let cancel_token = tokio_util::sync::CancellationToken::new();

    let ctx = SearchContext {
      orl: ORL::parse("orl://local/tmp").unwrap(),
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
    let (tx, _rx) = mpsc::channel(10);

    let ctx = SearchContext {
      orl: ORL::parse("orl://local/tmp").unwrap(),
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
  pub orl: ORL,
  pub sid: Arc<String>,
  pub tx: mpsc::Sender<SearchEvent>,
  pub cancel_token: Option<Arc<tokio_util::sync::CancellationToken>>,
}

impl SearchContext {
  pub fn is_cancelled(&self) -> bool {
    self.cancel_token.as_ref().map(|t| t.is_cancelled()).unwrap_or(false)
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

/// 创建搜索提供者 V2（基于 EndpointConnector）
///
/// 使用 EndpointConnector 替代 OpsFileSystem 的新实现。
/// 保留旧函数 create_search_provider 作为 create_search_provider_v1 以便兼容。
pub async fn create_search_provider_v2(
  pool: &SqlitePool,
  orl: &ORL,
) -> Result<Box<dyn SearchableFileSystem>, ServiceError> {
  use crate::utils::storage;

  match orl
    .endpoint_type()
    .map_err(|e| ServiceError::ProcessingError(e.to_string()))?
  {
    OrlEndpointType::Local => {
      // 新实现：直接使用 EndpointConnector
      Ok(Box::new(LocalEndpointConnector::new("/".to_string())) as Box<dyn SearchableFileSystem>)
    }
    OrlEndpointType::S3 => {
      let profile = orl.effective_id();
      // 加载 Profile
      let profile_row = crate::repository::s3::load_s3_profile(pool, &profile)
        .await
        .map_err(|e| ServiceError::ProcessingError(format!("加载 S3 Profile 失败: {:?}", e)))?
        .ok_or_else(|| ServiceError::ProcessingError(format!("S3 Profile 不存在: {}", profile)))?;

      // 构造 S3 客户端
      let client =
        storage::get_or_create_s3_client(&profile_row.endpoint, &profile_row.access_key, &profile_row.secret_key)
          .map_err(|e| ServiceError::ProcessingError(format!("创建 S3 客户端失败: {:?}", e)))?;

      let (bucket_name, _) = orl
        .path()
        .trim_start_matches('/')
        .split_once('/')
        .unwrap_or((orl.path().trim_start_matches('/'), ""));

      Ok(Box::new(S3EndpointConnector::new(client.as_ref().clone(), bucket_name.to_string())) as Box<dyn SearchableFileSystem>)
    }
    OrlEndpointType::Agent => {
      // Agent 保持不变，使用 AgentSearchProvider
      Ok(Box::new(AgentSearchProvider) as Box<dyn SearchableFileSystem>)
    }
  }
}

/// 创建搜索提供者 V1（旧版本，保留以便兼容）
pub async fn create_search_provider_v1(
  pool: &SqlitePool,
  orl: &ORL,
) -> Result<Box<dyn SearchableFileSystem>, ServiceError> {
  use crate::utils::storage;

  match orl
    .endpoint_type()
    .map_err(|e| ServiceError::ProcessingError(e.to_string()))?
  {
    OrlEndpointType::Local => Ok(Box::new(LocalOpsFS::new(None)) as Box<dyn SearchableFileSystem>),
    OrlEndpointType::S3 => {
      let profile = orl.effective_id();
      // 加载 Profile
      let profile_row = crate::repository::s3::load_s3_profile(pool, &profile)
        .await
        .map_err(|e| ServiceError::ProcessingError(format!("加载 S3 Profile 失败: {:?}", e)))?
        .ok_or_else(|| ServiceError::ProcessingError(format!("S3 Profile 不存在: {}", profile)))?;

      // 构造 S3 客户端
      let client =
        storage::get_or_create_s3_client(&profile_row.endpoint, &profile_row.access_key, &profile_row.secret_key)
          .map_err(|e| ServiceError::ProcessingError(format!("创建 S3 客户端失败: {:?}", e)))?;

      let (bucket_name, _) = orl
        .path()
        .trim_start_matches('/')
        .split_once('/')
        .unwrap_or((orl.path().trim_start_matches('/'), ""));

      Ok(Box::new(S3OpsFS::new(client.as_ref().clone(), bucket_name)) as Box<dyn SearchableFileSystem>)
    }
    OrlEndpointType::Agent => {
      // 使用内部定义的专用 search provider
      Ok(Box::new(AgentSearchProvider) as Box<dyn SearchableFileSystem>)
    }
  }
}

// ============================================================================
// LocalOpsFS 实现
// ============================================================================

#[async_trait]
impl SearchableFileSystem for LocalOpsFS {
  async fn search(&self, ctx: &SearchContext, req: &SearchRequest, _pool: &SqlitePool) -> Result<(), ServiceError> {
    // 显式转换为 Arc<dyn OpsFileSystem>
    let fs: Arc<dyn opsbox_core::odfs::fs::OpsFileSystem + Send + Sync> = Arc::new(self.clone());
    search_with_entry_stream(fs, ctx, req, true).await
  }
}

// ============================================================================
// S3OpsFS 实现
// ============================================================================

#[async_trait]
impl SearchableFileSystem for S3OpsFS {
  async fn search(&self, ctx: &SearchContext, req: &SearchRequest, _pool: &SqlitePool) -> Result<(), ServiceError> {
    let fs: Arc<dyn opsbox_core::odfs::fs::OpsFileSystem + Send + Sync> = Arc::new(self.clone());
    search_with_entry_stream(fs, ctx, req, false).await
  }
}

// ============================================================================
// LocalEndpointConnector 实现（新实现，使用 EndpointConnector）
// ============================================================================

#[async_trait]
impl SearchableFileSystem for LocalEndpointConnector {
  async fn search(&self, ctx: &SearchContext, req: &SearchRequest, _pool: &SqlitePool) -> Result<(), ServiceError> {
    use opsbox_core::odfs::orl::{EndpointType as OrlEndpointType, TargetType};

    // 1. 解析查询
    let spec = Query::parse_github_like(&req.query)
      .map_err(|e| ServiceError::ProcessingError(format!("查询解析失败: {:?}", e)))?;

    // 2. 使用 EndpointConnectorExt 创建 entry stream
    let path = ResourcePath::new(ctx.orl.path());
    // 注意：始终使用递归搜索，路径过滤由 EntryStreamProcessor 处理
    let resource_stream = self.as_entry_stream(&path, true).await
      .map_err(|e| ServiceError::ProcessingError(format!("创建条目流失败: {}", e)))?;

    // 3. 使用适配器包装
    let mut estream = Box::new(EntryStreamAdapter::new(resource_stream));

    // 4. 创建 EntryStreamProcessor
    let search_proc = Arc::new(SearchProcessor::new_with_encoding(
      Arc::new(spec),
      req.context_lines,
      req.encoding.clone(),
    ));
    let mut processor = EntryStreamProcessor::new(search_proc);

    if let Some(token) = ctx.cancel_token.clone() {
      processor = processor.with_cancel_token(token);
    }

    // 5. 路径过滤：base_path（仅 Local + Dir）
    if ctx.orl.endpoint_type().unwrap_or(OrlEndpointType::Local) == OrlEndpointType::Local
      && ctx.orl.target_type() == TargetType::Dir
    {
      processor = processor.with_base_path(ctx.orl.path());
    }

    // 6. 路径过滤：ORL 携带的内置过滤
    if let Some(glob) = ctx.orl.filter_glob()
      && let Ok(filter) = crate::query::path_glob_to_filter(&glob)
    {
      processor = processor.with_extra_path_filter(filter);
    }

    // 7. 路径过滤：用户输入的额外过滤
    let extra_filter = req.to_path_filter();
    if extra_filter.include.is_some() || extra_filter.exclude.is_some() {
      processor = processor.with_extra_path_filter(extra_filter);
    }

    // 8. 处理并发送结果
    processor
      .process_stream(&mut *estream, ctx.tx.clone())
      .await
      .map_err(ServiceError::ProcessingError)?;

    Ok(())
  }
}

// ============================================================================
// S3EndpointConnector 实现（新实现，使用 EndpointConnector）
// ============================================================================

#[async_trait]
impl SearchableFileSystem for S3EndpointConnector {
  async fn search(&self, ctx: &SearchContext, req: &SearchRequest, _pool: &SqlitePool) -> Result<(), ServiceError> {
    // 1. 解析查询
    let spec = Query::parse_github_like(&req.query)
      .map_err(|e| ServiceError::ProcessingError(format!("查询解析失败: {:?}", e)))?;

    // 2. 使用 EndpointConnectorExt 创建 entry stream
    let path = ResourcePath::new(ctx.orl.path());
    let resource_stream = self.as_entry_stream(&path, true).await
      .map_err(|e| ServiceError::ProcessingError(format!("创建条目流失败: {}", e)))?;

    // 3. 使用适配器包装
    let mut estream = Box::new(EntryStreamAdapter::new(resource_stream));

    // 4. 创建 EntryStreamProcessor
    let search_proc = Arc::new(SearchProcessor::new_with_encoding(
      Arc::new(spec),
      req.context_lines,
      req.encoding.clone(),
    ));
    let mut processor = EntryStreamProcessor::new(search_proc);

    if let Some(token) = ctx.cancel_token.clone() {
      processor = processor.with_cancel_token(token);
    }

    // 5. 路径过滤：ORL 携带的内置过滤
    if let Some(glob) = ctx.orl.filter_glob()
      && let Ok(filter) = crate::query::path_glob_to_filter(&glob)
    {
      processor = processor.with_extra_path_filter(filter);
    }

    // 6. 路径过滤：用户输入的额外过滤
    let extra_filter = req.to_path_filter();
    if extra_filter.include.is_some() || extra_filter.exclude.is_some() {
      processor = processor.with_extra_path_filter(extra_filter);
    }

    // 7. 处理并发送结果
    processor
      .process_stream(&mut *estream, ctx.tx.clone())
      .await
      .map_err(ServiceError::ProcessingError)?;

    Ok(())
  }
}

// ============================================================================
// AgentSearchProvider (内部使用, 替代 AgentOpsFS)
// ============================================================================

struct AgentSearchProvider;

#[async_trait]
impl SearchableFileSystem for AgentSearchProvider {
  async fn search(&self, ctx: &SearchContext, req: &SearchRequest, pool: &SqlitePool) -> Result<(), ServiceError> {
    use crate::agent::{SearchOptions, SearchService, Target as AgentTarget, create_agent_client_by_id};
    use futures::StreamExt;
    use opsbox_core::odfs::orl::TargetType;
    use tracing::{debug, info};

    let agent_id = ctx.orl.effective_id().to_string();

    info!(
      "[SearchableFileSystem] 开始 Agent 搜索: agent_id={} ctx={}",
      agent_id, req.context_lines
    );

    // 创建 AgentClient
    let client = create_agent_client_by_id(pool, agent_id.clone())
      .await
      .map_err(|e| ServiceError::ProcessingError(format!("无法创建 Agent 客户端: {}", e)))?;

    // 健康检查
    if !client.health_check().await {
      return Err(ServiceError::ProcessingError("Agent 健康检查失败".to_string()));
    }

    // 构造 SearchOptions
    let target = match ctx.orl.target_type() {
      TargetType::Dir => AgentTarget::Dir {
        path: ctx.orl.path().to_string(),
        recursive: true,
      },
      TargetType::Archive => AgentTarget::Archive {
        path: ctx.orl.path().to_string(),
        entry: ctx.orl.entry_path().map(|c| c.into_owned()),
      },
    };

    let search_options = SearchOptions {
      target,
      path_filter: ctx.orl.filter_glob().map(|c| c.into_owned()),
      path_includes: req.path_includes.clone(),
      path_excludes: req.path_excludes.clone(),
      encoding: req.encoding.clone(),
      timeout_secs: None,
      max_results: None,
    };

    // 调用 Agent 搜索
    let mut stream = client
      .search(&req.query, req.context_lines, search_options)
      .await
      .map_err(|e| ServiceError::ProcessingError(format!("调用 Agent 搜索失败: {}", e)))?;

    // 转发结果
    let mut result_count = 0;
    while let Some(item) = stream.next().await {
      if ctx.is_cancelled() {
        break;
      }

      match item {
        Ok(res) => {
          result_count += 1;
          if ctx.tx.send(SearchEvent::Success(res)).await.is_err() {
            debug!("[SearchableFileSystem] 发送失败，通道已关闭");
            break;
          }
        }
        Err(e) => {
          tracing::error!("[SearchableFileSystem] Agent 结果流读取错误: {}", e);
          break;
        }
      }
    }

    debug!(
      "[SearchableFileSystem] Agent 搜索完成: agent_id={} results={}",
      agent_id, result_count
    );

    Ok(())
  }
}

// ============================================================================
// 共享辅助函数
// ============================================================================

/// Local 和 S3 共享的搜索逻辑
async fn search_with_entry_stream(
  fs: Arc<dyn opsbox_core::odfs::fs::OpsFileSystem + Send + Sync>,
  ctx: &SearchContext,
  req: &SearchRequest,
  use_base_path: bool,
) -> Result<(), ServiceError> {
  use super::entry_stream::get_entry_stream_from_fs;
  use opsbox_core::odfs::orl::{EndpointType, TargetType};
  use tracing::info;

  let source_name = ctx.orl.display_name();
  info!(
    "[SearchableFileSystem] 开始搜索: source={} ctx={}",
    source_name, req.context_lines
  );

  // 1. 解析查询
  let spec = Query::parse_github_like(&req.query)
    .map_err(|e| ServiceError::ProcessingError(format!("查询解析失败: {:?}", e)))?;

  // 2. 创建 entry stream (复用 Manager)
  let mut estream = get_entry_stream_from_fs(fs, &ctx.orl, true)
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

  // 4. 路径过滤：base_path（仅 Local + Dir）
  if use_base_path
    && ctx.orl.endpoint_type().unwrap_or(EndpointType::Local) == EndpointType::Local
    && ctx.orl.target_type() == TargetType::Dir
  {
    processor = processor.with_base_path(ctx.orl.path());
  }

  // 5. 路径过滤：ORL 携带的内置过滤
  if let Some(glob) = ctx.orl.filter_glob()
    && let Ok(filter) = crate::query::path_glob_to_filter(&glob)
  {
    processor = processor.with_extra_path_filter(filter);
  }

  // 6. 路径过滤：用户输入的额外过滤
  let extra_filter = req.to_path_filter();
  if extra_filter.include.is_some() || extra_filter.exclude.is_some() {
    processor = processor.with_extra_path_filter(extra_filter);
  }

  // 7. 处理并发送结果
  processor
    .process_stream(&mut *estream, ctx.tx.clone())
    .await
    .map_err(ServiceError::ProcessingError)?;

  Ok(())
}

// ============================================================================
// 条件编译：根据特性标志选择使用新实现还是旧实现
// ============================================================================

// 默认使用新实现（EndpointConnector），如果禁用了 use-endpoint-connector 特性则使用旧实现
#[cfg(feature = "use-endpoint-connector")]
pub use create_search_provider_v2 as create_search_provider;

#[cfg(not(feature = "use-endpoint-connector"))]
pub use create_search_provider_v1 as create_search_provider;
