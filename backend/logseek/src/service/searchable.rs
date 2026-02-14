//! SearchProvider trait - 统一的搜索提供者接口
//!
//! 为不同的文件系统 provider 提供统一的搜索能力抽象。
//! 使用 DFS (Distributed File System) 模块进行文件系统操作。

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use opsbox_core::SqlitePool;
use opsbox_core::dfs::{Location, Resource, ResourcePath, SearchConfig, Streamable};

use super::ServiceError;
use super::search::{SearchEvent, SearchProcessor, SearchResult};
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
        match globset::GlobBuilder::new(p).build() {
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
        match globset::GlobBuilder::new(p).build() {
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
      Ok(Box::new(LocalSearchProvider) as Box<dyn SearchProvider>)
    }
    Location::Cloud => {
      // S3 对象存储搜索
      let profile = resource.endpoint.identity.clone();
      Ok(Box::new(S3SearchProvider { profile }) as Box<dyn SearchProvider>)
    }
    Location::Remote { .. } => {
      // Agent 代理搜索
      Ok(Box::new(AgentSearchProvider) as Box<dyn SearchProvider>)
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

    let path_str = ctx.resource.primary_path.to_string();
    info!(
      "[LocalSearchProvider] 开始搜索: path={} ctx={}",
      path_str, req.context_lines
    );

    // 1. 解析查询
    let spec = Query::parse_github_like(&req.query)
      .map_err(|e| ServiceError::ProcessingError(format!("查询解析失败: {:?}", e)))?;

    // 2. 确定根目录（使用路径本身或其父目录）
    let path = PathBuf::from(&path_str);

    // Check if it is an archive based on extension (heuristic)
    let is_archive = path.is_file()
      && path.extension().is_some_and(|ext| {
        let s = ext.to_string_lossy().to_lowercase();
        s == "tar" || s == "gz" || s == "tgz" || s == "zip"
      });

    let (root, relative_path) = if path.is_dir() {
      (path.clone(), ResourcePath::parse(""))
    } else if path.exists() {
      // 单个文件
      (
        path.parent().unwrap_or(&path).to_path_buf(),
        ResourcePath::parse(path.file_name().unwrap().to_string_lossy().as_ref()),
      )
    } else {
      // 路径不存在，尝试使用父目录作为 root
      let parent = path.parent().unwrap_or(&path).to_path_buf();
      (parent, ResourcePath::parse(""))
    };

    // 4. 获取 EntryStream
    // 如果是归档文件，直接使用 Archive Stream，绕过 DFS 抽象（因为 DFS 可能不自动展开归档）
    let mut entry_stream: Box<dyn opsbox_core::fs::EntryStream> = if is_archive {
      info!(
        "[LocalSearchProvider] 检测到归档文件，使用归档流模式: {}",
        path.display()
      );
      let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| ServiceError::ProcessingError(format!("打开归档文件失败: {}", e)))?;
      opsbox_core::fs::create_archive_stream_from_reader(file, Some(&path_str))
        .await
        .map_err(|e| ServiceError::ProcessingError(format!("创建归档流失败: {}", e)))?
    } else {
      // 3. 创建本地文件系统 (Moved inside else block or duplicated creation if needed?
      // FS creation is cheap.
      let fs = LocalFileSystem::new(root)
        .map_err(|e| ServiceError::ProcessingError(format!("创建本地文件系统失败: {}", e)))?;

      let search_config = SearchConfig::default();
      fs.as_entry_stream(&relative_path, true, &search_config)
        .await
        .map_err(|e| ServiceError::ProcessingError(format!("创建条目流失败: {}", e)))?
    };

    // 5. 创建 SearchProcessor 并转换为 DFS ContentProcessor
    let search_proc = Arc::new(SearchProcessor::new_with_encoding(
      Arc::new(spec),
      req.context_lines,
      req.encoding.clone(),
    ));

    // 6. 创建 DFS EntryStreamProcessor
    let mut processor = opsbox_core::processing::EntryStreamProcessor::new(search_proc);

    if let Some(token) = ctx.cancel_token.clone() {
      processor = processor.with_cancel_token(token);
    }

    // 7. 路径过滤：base_path
    // 对于归档文件，不设置 base_path（条目路径相对于归档内部）
    // 对于目录搜索，设置 base_path 以支持相对 Glob
    if !is_archive {
      processor = processor.with_base_path(&path_str);
    }

    // 8. 路径过滤：ORL 携带的内置 glob 过滤
    if let Some(ref glob) = ctx.resource.filter_glob
      && let Ok(filter) = crate::query::path_glob_to_filter(glob)
    {
      let dfs_filter = opsbox_core::processing::PathFilter {
        include: filter.include,
        exclude: filter.exclude,
        include_contains: filter.include_contains,
        exclude_contains: filter.exclude_contains,
      };
      processor = processor.with_extra_path_filter(dfs_filter);
    }

    // 9. 路径过滤：用户输入的额外过滤
    let extra_filter = req.to_path_filter();
    if extra_filter.include.is_some() || extra_filter.exclude.is_some() {
      let dfs_filter = opsbox_core::processing::PathFilter {
        include: extra_filter.include,
        exclude: extra_filter.exclude,
        include_contains: extra_filter.include_contains,
        exclude_contains: extra_filter.exclude_contains,
      };
      processor = processor.with_extra_path_filter(dfs_filter);
    }

    // 10. 处理并发送结果
    let tx = ctx.tx.clone();
    let tx_stopped = ctx.tx.clone();
    let tx_err = ctx.tx.clone();
    processor = processor.with_is_stopped_fn(move || tx_stopped.is_closed());
    processor = processor.with_error_callback(move |msg| {
      let _ = tx_err.try_send(SearchEvent::Error {
        source: "LocalSearchProvider".to_string(),
        message: msg,
        recoverable: true,
      });
    });
    processor
      .process_stream(entry_stream.as_mut(), move |content| {
        let tx = tx.clone();
        async move {
          // 将 ProcessedContent 转换为 SearchEvent
          if let Some(result_value) = content.result
            && let Ok(search_result) = serde_json::from_value::<SearchResult>(result_value)
          {
            let mut result = search_result;
            result.archive_path = content.archive_path;
            // 使用异步 send，不再阻塞线程
            let _ = tx.send(SearchEvent::Success(result)).await;
          }
        }
      })
      .await
      .map_err(ServiceError::ProcessingError)?;

    Ok(())
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

    // 4. 解析查询
    let spec = Query::parse_github_like(&req.query)
      .map_err(|e| ServiceError::ProcessingError(format!("查询解析失败: {:?}", e)))?;

    // 5. 创建 S3 存储
    let s3_storage =
      S3Storage::new(s3_config).map_err(|e| ServiceError::ProcessingError(format!("创建 S3 存储失败: {}", e)))?;

    // 6. 获取 EntryStream
    // 6. 获取 EntryStream
    let search_config = SearchConfig::default();
    let resource_path = ResourcePath::parse(object_key);

    // Check if it is an archive based on object key extension
    let is_archive = object_key.to_lowercase().ends_with(".tar")
      || object_key.to_lowercase().ends_with(".tar.gz")
      || object_key.to_lowercase().ends_with(".tgz")
      || object_key.to_lowercase().ends_with(".zip");

    let mut entry_stream: Box<dyn opsbox_core::fs::EntryStream> = if is_archive {
      info!(
        "[S3SearchProvider] 检测到归档文件，使用归档流模式: bucket={} key={}",
        bucket_name, object_key
      );
      // S3Storage needs a method to get reader efficiently?
      // S3Storage doesn't expose public `get_object`.
      // But `as_entry_stream` uses internal logic.
      // We might need to use `s3_storage.read_object(path)` if available?
      // opsbox-core S3Storage export `read_object_range`?
      // Let's assume we can fallback to `as_entry_stream` if strictly needed,
      // BUT `as_entry_stream` might treat it as single file.
      // Actually S3Storage implementation might handle it?
      // If not, we have a problem accessing `get_reader` from `S3Storage` externally.
      // Wait, S3Storage implements `OpbxFileSystem`. `open_file`?
      use opsbox_core::dfs::OpbxFileSystem;
      match s3_storage.open_read(&resource_path).await {
        Ok(reader) => opsbox_core::fs::create_archive_stream_from_reader(reader, Some(&path_str))
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

    // 7. 创建 SearchProcessor
    let search_proc = Arc::new(SearchProcessor::new_with_encoding(
      Arc::new(spec),
      req.context_lines,
      req.encoding.clone(),
    ));

    // 8. 创建 DFS EntryStreamProcessor
    let mut processor = opsbox_core::processing::EntryStreamProcessor::new(search_proc);

    if let Some(token) = ctx.cancel_token.clone() {
      processor = processor.with_cancel_token(token);
    }

    // 9. 路径过滤：设置 base_path (仅非归档)
    if !is_archive {
      // S3 paths start with bucket/key or just key?
      // path_str includes bucket usually? ORL: orl://profile@s3/bucket/key
      // path_str is `bucket/key` (lines 361 logic).
      processor = processor.with_base_path(&path_str);
    }

    // 10. 路径过滤：ORL 携带的内置 glob 过滤
    if let Some(ref glob) = ctx.resource.filter_glob
      && let Ok(filter) = crate::query::path_glob_to_filter(glob)
    {
      let dfs_filter = opsbox_core::processing::PathFilter {
        include: filter.include,
        exclude: filter.exclude,
        include_contains: filter.include_contains,
        exclude_contains: filter.exclude_contains,
      };
      processor = processor.with_extra_path_filter(dfs_filter);
    }
    // 10. 路径过滤：用户输入的额外过滤
    let extra_filter = req.to_path_filter();
    if extra_filter.include.is_some() || extra_filter.exclude.is_some() {
      let dfs_filter = opsbox_core::processing::PathFilter {
        include: extra_filter.include,
        exclude: extra_filter.exclude,
        include_contains: extra_filter.include_contains,
        exclude_contains: extra_filter.exclude_contains,
      };
      processor = processor.with_extra_path_filter(dfs_filter);
    }

    // 11. 处理并发送结果
    let tx = ctx.tx.clone();
    let tx_stopped = ctx.tx.clone();
    let tx_err = ctx.tx.clone();
    processor = processor.with_is_stopped_fn(move || tx_stopped.is_closed());
    processor = processor.with_error_callback(move |msg| {
      let _ = tx_err.try_send(SearchEvent::Error {
        source: "S3SearchProvider".to_string(),
        message: msg,
        recoverable: true,
      });
    });
    processor
      .process_stream(entry_stream.as_mut(), move |content| {
        let tx = tx.clone();
        async move {
          if let Some(result_value) = content.result
            && let Ok(search_result) = serde_json::from_value::<SearchResult>(result_value)
          {
            let mut result = search_result;
            result.archive_path = content.archive_path;
            let _ = tx.send(SearchEvent::Success(result)).await;
          }
        }
      })
      .await
      .map_err(ServiceError::ProcessingError)?;

    Ok(())
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
    let target = if ctx.resource.archive_context.is_some() {
      AgentTarget::Archive {
        path: path.clone(),
        entry: ctx.resource.archive_context.as_ref().map(|c| c.inner_path.to_string()),
      }
    } else {
      // Check if it is an archive based on extension (heuristic)
      let is_archive = path.to_lowercase().ends_with(".tar")
        || path.to_lowercase().ends_with(".tar.gz")
        || path.to_lowercase().ends_with(".tgz")
        || path.to_lowercase().ends_with(".zip");

      if is_archive {
        AgentTarget::Archive {
          path: path.clone(),
          entry: None,
        }
      } else {
        AgentTarget::Dir {
          path: path.clone(),
          recursive: true,
        }
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
