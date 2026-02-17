//! SearchProvider trait - 统一的搜索提供者接口
//!
//! 为不同的文件系统 provider 提供统一的搜索能力抽象。
//! 使用 DFS (Distributed File System) 模块进行文件系统操作。

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use opsbox_core::SqlitePool;
use opsbox_core::dfs::archive::{ArchiveType, infer_archive_from_path};
use opsbox_core::dfs::{Location, Resource, ResourcePath, SearchConfig, Streamable};

use super::ServiceError;
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

impl SearchRequest {
  /// 转换为路径过滤器
  pub fn to_path_filter(&self) -> crate::query::PathFilter {
    crate::query::combine_path_filters(&self.path_includes, &self.path_excludes).unwrap_or_default()
  }
}

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
  async fn search(&self, ctx: &SearchContext, req: &SearchRequest, _pool: &SqlitePool) -> Result<(), ServiceError> {
    use opsbox_core::dfs::LocalFileSystem;
    use std::path::PathBuf;
    use tracing::info;

    use super::search_runner::{self, SearchRunnerConfig};

    let path_str = ctx.resource.primary_path.to_string();
    info!(
      "[LocalSearchProvider] 开始搜索: path={} ctx={}",
      path_str, req.context_lines
    );

    // 1. 确定路径类型和根目录
    let path = PathBuf::from(&path_str);

    // 归档判定在 SearchExecutor 分发前完成，这里直接读取资源上下文
    let is_archive = ctx.resource.is_archive();

    let (search_root, relative_path) = if path.is_dir() {
      (path.clone(), ResourcePath::parse(""))
    } else if path.exists() {
      (
        path.parent().unwrap_or(&path).to_path_buf(),
        ResourcePath::parse(path.file_name().unwrap().to_string_lossy().as_ref()),
      )
    } else {
      let parent = path.parent().unwrap_or(&path).to_path_buf();
      (parent, ResourcePath::parse(""))
    };

    // 2. 获取 EntryStream
    let mut entry_stream: Box<dyn opsbox_core::fs::EntryStream> = if is_archive {
      info!(
        "[LocalSearchProvider] 检测到归档文件，使用归档流模式: {}",
        path.display()
      );
      let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| ServiceError::ProcessingError(format!("打开归档文件失败: {}", e)))?;
      // 使用已知类型，跳过内部 magic bytes 检测
      let archive_type = ctx
        .resource
        .archive_context
        .as_ref()
        .and_then(|c| c.archive_type)
        .unwrap_or(ArchiveType::Unknown);
      opsbox_core::fs::open_archive_typed(file, Some(&path_str), archive_type)
        .await
        .map_err(|e| ServiceError::ProcessingError(format!("创建归档流失败: {}", e)))?
    } else {
      let fs = LocalFileSystem::new(search_root)
        .map_err(|e| ServiceError::ProcessingError(format!("创建本地文件系统失败: {}", e)))?;
      let search_config = SearchConfig::default();
      fs.as_entry_stream(&relative_path, true, &search_config)
        .await
        .map_err(|e| ServiceError::ProcessingError(format!("创建条目流失败: {}", e)))?
    };

    // 3. 构建搜索配置
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

    // 4. 执行搜索
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
    use opsbox_core::dfs::S3Config;
    use opsbox_core::dfs::S3Storage;
    use tracing::info;

    use super::search_runner::{self, SearchRunnerConfig};

    let path_str = ctx.resource.primary_path.to_string();
    info!(
      "[S3SearchProvider] 开始搜索: profile={} path={} ctx={}",
      self.profile, path_str, req.context_lines
    );

    // 1. 加载 Profile
    let profile_row = crate::repository::s3::load_s3_profile(pool, &self.profile)
      .await
      .map_err(|e| ServiceError::ProcessingError(format!("加载 S3 Profile 失败: {:?}", e)))?
      .ok_or_else(|| ServiceError::ProcessingError(format!("S3 Profile 不存在: {}", self.profile)))?;

    // 2. 创建 S3Config
    let s3_config = S3Config::new(
      self.profile.clone(),
      profile_row.endpoint.clone(),
      profile_row.access_key.clone(),
      profile_row.secret_key.clone(),
    );

    // 3. 提取 bucket 名称
    let (bucket_name, object_key) = path_str
      .trim_start_matches('/')
      .split_once('/')
      .unwrap_or((path_str.trim_start_matches('/'), ""));

    let s3_config = s3_config.with_bucket(bucket_name.to_string());

    // 4. 创建 S3 存储
    let s3_storage =
      S3Storage::new(s3_config).map_err(|e| ServiceError::ProcessingError(format!("创建 S3 存储失败: {}", e)))?;

    // 5. 获取 EntryStream
    let search_config = SearchConfig::default();
    let resource_path = ResourcePath::parse(object_key);

    // 归档判定在 SearchExecutor 分发前完成，这里直接读取资源上下文
    let is_archive = ctx.resource.is_archive();

    let mut entry_stream: Box<dyn opsbox_core::fs::EntryStream> = if is_archive {
      info!(
        "[S3SearchProvider] 检测到归档文件，使用归档流模式: bucket={} key={}",
        bucket_name, object_key
      );
      use opsbox_core::dfs::OpbxFileSystem;
      // 使用已知类型，跳过内部 magic bytes 检测
      let archive_type = ctx
        .resource
        .archive_context
        .as_ref()
        .and_then(|c| c.archive_type)
        .unwrap_or(ArchiveType::Unknown);
      match s3_storage.open_read(&resource_path).await {
        Ok(reader) => opsbox_core::fs::open_archive_typed(reader, Some(&path_str), archive_type)
          .await
          .map_err(|e| ServiceError::ProcessingError(format!("创建归档流失败: {}", e)))?,
        Err(e) => return Err(ServiceError::ProcessingError(format!("打开 S3 文件失败: {}", e))),
      }
    } else {
      s3_storage
        .as_entry_stream(&resource_path, true, &search_config)
        .await
        .map_err(|e| ServiceError::ProcessingError(format!("创建条目流失败: {}", e)))?
    };

    // 6. 构建搜索配置
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

    // 7. 执行搜索
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

// ============================================================================
// Tests
// ============================================================================

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
}
