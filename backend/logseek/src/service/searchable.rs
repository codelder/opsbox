//! SearchableFileSystem trait - 统一的搜索接口
//!
//! 为不同的文件系统 provider 提供统一的搜索能力抽象。
//! 在 logseek 中定义，为 opsbox-core 的 provider 类型实现。

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use opsbox_core::odfs::providers::{LocalOpsFS, S3OpsFS};
use opsbox_core::odfs::orl::ORL;
use opsbox_core::SqlitePool;

use super::entry_stream::EntryStreamProcessor;
use super::search::{SearchEvent, SearchProcessor};
use super::ServiceError;
use crate::query::Query;

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
                    Ok(g) => { builder.add(g); }
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
                    Ok(g) => { builder.add(g); }
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
    pub sid: String,
    pub tx: mpsc::Sender<SearchEvent>,
    pub cancel_token: Option<tokio_util::sync::CancellationToken>,
}

impl SearchContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.as_ref().map_or(false, |t| t.is_cancelled())
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
    async fn search(
        &self,
        ctx: &SearchContext,
        req: &SearchRequest,
        pool: &SqlitePool,
    ) -> Result<(), ServiceError>;
}

// ============================================================================
// 工厂函数
// ============================================================================

/// 创建搜索提供者
pub async fn create_search_provider(
    pool: &SqlitePool,
    orl: &ORL,
) -> Result<Box<dyn SearchableFileSystem>, ServiceError> {
    use opsbox_core::odfs::orl::EndpointType;
    use crate::utils::storage;

    match orl.endpoint_type().map_err(|e| ServiceError::ProcessingError(e.to_string()))? {
        EndpointType::Local => {
            Ok(Box::new(LocalOpsFS::new(None)) as Box<dyn SearchableFileSystem>)
        }
        EndpointType::S3 => {
            let profile = orl.effective_id();
            // 加载 Profile
            let profile_row = crate::repository::s3::load_s3_profile(pool, &profile)
                .await
                .map_err(|e| ServiceError::ProcessingError(format!("加载 S3 Profile 失败: {:?}", e)))?
                .ok_or_else(|| ServiceError::ProcessingError(format!("S3 Profile 不存在: {}", profile)))?;

            // 构造 S3 客户端
            let client = storage::get_or_create_s3_client(
                &profile_row.endpoint,
                &profile_row.access_key,
                &profile_row.secret_key,
            )
            .map_err(|e| ServiceError::ProcessingError(format!("创建 S3 客户端失败: {:?}", e)))?;

            let (bucket_name, _) = orl
                .path()
                .trim_start_matches('/')
                .split_once('/')
                .unwrap_or((orl.path().trim_start_matches('/'), ""));

            Ok(Box::new(S3OpsFS::new(client.as_ref().clone(), bucket_name)) as Box<dyn SearchableFileSystem>)
        }
        EndpointType::Agent => {
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
    async fn search(
        &self,
        ctx: &SearchContext,
        req: &SearchRequest,
        _pool: &SqlitePool,
    ) -> Result<(), ServiceError> {
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
    async fn search(
        &self,
        ctx: &SearchContext,
        req: &SearchRequest,
        _pool: &SqlitePool,
    ) -> Result<(), ServiceError> {
        let fs: Arc<dyn opsbox_core::odfs::fs::OpsFileSystem + Send + Sync> = Arc::new(self.clone());
        search_with_entry_stream(fs, ctx, req, false).await
    }
}

// ============================================================================
// AgentSearchProvider (内部使用, 替代 AgentOpsFS)
// ============================================================================

struct AgentSearchProvider;

#[async_trait]
impl SearchableFileSystem for AgentSearchProvider {
    async fn search(
        &self,
        ctx: &SearchContext,
        req: &SearchRequest,
        pool: &SqlitePool,
    ) -> Result<(), ServiceError> {
        use crate::agent::{create_agent_client_by_id, SearchOptions, SearchService, Target as AgentTarget};
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
    use opsbox_core::odfs::orl::{EndpointType, TargetType};
    use tracing::info;
    use super::entry_stream::get_entry_stream_from_fs;

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
    if let Some(glob) = ctx.orl.filter_glob() {
        if let Ok(filter) = crate::query::path_glob_to_filter(&glob) {
            processor = processor.with_extra_path_filter(filter);
        }
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
        .map_err(|e| ServiceError::ProcessingError(e))?;

    Ok(())
}
