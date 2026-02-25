//! SearchProvider trait - 统一的搜索提供者接口
//!
//! 为不同的文件系统 provider 提供统一的搜索能力抽象。
//! 使用 DFS (Distributed File System) 模块进行文件系统操作。

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use opsbox_core::SqlitePool;
use opsbox_core::dfs::archive::infer_archive_from_path;
use opsbox_core::dfs::{Location, Resource};

use super::ServiceError;
use super::entry_stream::create_search_entry_stream_from_resource;
use super::search::SearchEvent;

/// 搜索请求参数
#[derive(Clone, Debug)]
pub struct SearchRequest {
  pub query: String,
  pub context_lines: usize,
  pub encoding: Option<String>,
  pub path_includes: Vec<String>,
  pub path_excludes: Vec<String>,
}

impl SearchRequest {}

/// 搜索上下文
#[derive(Clone)]
pub struct SearchContext {
  pub resource: Resource,
  pub sid: Arc<String>,
  pub tx: mpsc::Sender<SearchEvent>,
  pub cancel_token: Option<Arc<tokio_util::sync::CancellationToken>>,
}

impl SearchContext {
  pub fn is_cancelled(&self) -> bool {
    self.cancel_token.as_ref().map(|t| t.is_cancelled()).unwrap_or(false)
  }
}

/// 搜索提供者 trait
///
/// 为不同的存储 provider 提供统一的搜索接口
#[async_trait]
pub trait SearchProvider: Send + Sync {
  /// 执行搜索
  ///
  /// # Arguments
  /// * `ctx` - 搜索上下文（包含 Resource、SID、发送通道等）
  /// * `req` - 搜索请求参数
  /// * `pool` - 数据库连接池（用于获取配置）
  async fn search(&self, ctx: &SearchContext, req: &SearchRequest, pool: &SqlitePool) -> Result<(), ServiceError>;
}

// ============================================================================
// 工厂函数
// ============================================================================

/// 创建搜索提供者
pub async fn create_search_provider(
  _pool: &SqlitePool,
  resource: &Resource,
) -> Result<Box<dyn SearchProvider>, ServiceError> {
  match &resource.endpoint.location {
    Location::Local => {
      // Local 文件系统搜索
      Ok(Box::new(LocalSearchProvider))
    }
    Location::Cloud => {
      // S3 对象存储搜索
      let profile = resource.endpoint.identity.clone();
      Ok(Box::new(S3SearchProvider { profile }))
    }
    Location::Remote { .. } => {
      // Agent 代理搜索
      Ok(Box::new(AgentSearchProvider))
    }
  }
}

// ============================================================================
// LocalSearchProvider - 本地文件系统搜索
// ============================================================================

struct LocalSearchProvider;

#[async_trait]
impl SearchProvider for LocalSearchProvider {
  async fn search(&self, ctx: &SearchContext, req: &SearchRequest, pool: &SqlitePool) -> Result<(), ServiceError> {
    use tracing::info;

    use super::search_runner::{self, SearchRunnerConfig};

    let path_str = ctx.resource.primary_path.to_string();
    info!(
      "[LocalSearchProvider] 开始搜索: path={} ctx={}",
      path_str, req.context_lines
    );

    // 归档判定在 SearchExecutor 分发前完成，这里直接读取资源上下文并统一创建流
    let is_archive = ctx.resource.is_archive();
    let mut entry_stream = create_search_entry_stream_from_resource(pool, &ctx.resource)
      .await
      .map_err(ServiceError::ProcessingError)?;

    // 1. 构建搜索配置
    let base_path = if !is_archive {
      Some(PathBuf::from(&path_str))
    } else {
      None
    };

    let extra_filters = search_runner::build_path_filters(
      ctx.resource.filter_glob.as_deref(),
      &req.path_includes,
      &req.path_excludes,
    );

    let config = SearchRunnerConfig::new(&req.query)
      .with_context_lines(req.context_lines)
      .with_encoding_opt(req.encoding.clone())
      .with_base_path_opt(base_path)
      .with_extra_filters(extra_filters);

    // 2. 执行搜索
    search_runner::run_search(
      entry_stream.as_mut(),
      config,
      ctx.tx.clone(),
      ctx.cancel_token.clone(),
      "LocalSearchProvider",
    )
    .await
  }
}

// ============================================================================
// S3SearchProvider - S3 对象存储搜索
// ============================================================================

struct S3SearchProvider {
  profile: String,
}

#[async_trait]
impl SearchProvider for S3SearchProvider {
  async fn search(&self, ctx: &SearchContext, req: &SearchRequest, pool: &SqlitePool) -> Result<(), ServiceError> {
    use tracing::info;

    use super::search_runner::{self, SearchRunnerConfig};

    let path_str = ctx.resource.primary_path.to_string();
    info!(
      "[S3SearchProvider] 开始搜索: profile={} path={} ctx={}",
      self.profile, path_str, req.context_lines
    );

    // 归档判定在 SearchExecutor 分发前完成，这里统一创建流
    let is_archive = ctx.resource.is_archive();
    let mut entry_stream = create_search_entry_stream_from_resource(pool, &ctx.resource)
      .await
      .map_err(ServiceError::ProcessingError)?;

    // 1. 构建搜索配置
    let base_path = if !is_archive {
      Some(PathBuf::from(&path_str))
    } else {
      None
    };

    let extra_filters = search_runner::build_path_filters(
      ctx.resource.filter_glob.as_deref(),
      &req.path_includes,
      &req.path_excludes,
    );

    let config = SearchRunnerConfig::new(&req.query)
      .with_context_lines(req.context_lines)
      .with_encoding_opt(req.encoding.clone())
      .with_base_path_opt(base_path)
      .with_extra_filters(extra_filters);

    // 2. 执行搜索
    search_runner::run_search(
      entry_stream.as_mut(),
      config,
      ctx.tx.clone(),
      ctx.cancel_token.clone(),
      "S3SearchProvider",
    )
    .await
  }
}

// ============================================================================
// AgentSearchProvider - Agent 代理搜索
// ============================================================================

struct AgentSearchProvider;

#[async_trait]
impl SearchProvider for AgentSearchProvider {
  async fn search(&self, ctx: &SearchContext, req: &SearchRequest, pool: &SqlitePool) -> Result<(), ServiceError> {
    use crate::agent::{SearchOptions, SearchService, Target as AgentTarget, create_agent_client_by_id};
    use futures::StreamExt;
    use tracing::{debug, info};

    let agent_id = ctx.resource.endpoint.identity.clone();
    let path = ctx.resource.primary_path.to_string();

    info!(
      "[AgentSearchProvider] 开始 Agent 搜索: agent_id={} path={} ctx={}",
      agent_id, path, req.context_lines
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
    let is_archive = ctx.resource.is_archive() || infer_archive_from_path(&path).is_some();

    let target = if ctx.resource.archive_context.is_some() {
      AgentTarget::Archive {
        path: path.clone(),
        entry: ctx.resource.archive_context.as_ref().map(|c| c.inner_path.to_string()),
      }
    } else if is_archive {
      AgentTarget::Archive {
        path: path.clone(),
        entry: None,
      }
    } else {
      AgentTarget::Dir {
        path: path.clone(),
        recursive: true,
      }
    };

    let search_options = SearchOptions {
      target,
      path_filter: ctx.resource.filter_glob.clone(),
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
            debug!("[AgentSearchProvider] 发送失败，通道已关闭");
            break;
          }
        }
        Err(e) => {
          tracing::error!("[AgentSearchProvider] Agent 结果流读取错误: {}", e);
          break;
        }
      }
    }

    debug!(
      "[AgentSearchProvider] Agent 搜索完成: agent_id={} results={}",
      agent_id, result_count
    );

    Ok(())
  }
}
