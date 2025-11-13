// ============================================================================
// LogSeek 模块 - 日志搜索服务
// ============================================================================
// 分层架构：
// - routes: API 层，HTTP 路由和处理器
// - service: 服务层，业务逻辑
// - repository: 数据访问层，持久化和缓存
// - domain: 领域层，核心业务模型
// - utils: 工具层，通用功能
// - query: 查询解析器
// ============================================================================

// API 层
pub mod api;
pub mod routes; // 保留以保持向后兼容

// 服务层
pub mod service;

// 数据访问层
pub mod repository;

// 领域层
pub mod domain;

// 工具层
pub mod utils;

// 查询解析器
pub mod query;

// Agent 模块（远程 Agent 搜索能力）
pub mod agent;

use opsbox_core::{Result, SqlitePool};

// 实现 ServiceError 到 AppError 的转换
impl From<service::ServiceError> for opsbox_core::AppError {
  fn from(err: service::ServiceError) -> Self {
    match err {
      service::ServiceError::ConfigError(msg) => opsbox_core::AppError::Config(msg),
      service::ServiceError::ProcessingError(msg) => opsbox_core::AppError::Internal(msg),
      service::ServiceError::SearchFailed { path, error } => {
        opsbox_core::AppError::Internal(format!("搜索失败 - 路径: {}, 错误: {}", path, error))
      }
      service::ServiceError::IoError { path, error } => {
        opsbox_core::AppError::Internal(format!("IO 错误: path={}, error={}", path, error))
      }
      service::ServiceError::ChannelClosed => {
        opsbox_core::AppError::Internal("Channel 已关闭: 接收端已断开连接".to_string())
      }
      service::ServiceError::Repository(repo_err) => {
        // 将 Repository 错误转换为 AppError
        match repo_err {
          repository::RepositoryError::NotFound(msg) => opsbox_core::AppError::NotFound(msg),
          repository::RepositoryError::QueryFailed(msg) => opsbox_core::AppError::Internal(format!("查询失败: {}", msg)),
          repository::RepositoryError::StorageError(msg) => opsbox_core::AppError::ExternalService(format!("对象存储错误: {}", msg)),
          repository::RepositoryError::CacheFailed(msg) => opsbox_core::AppError::Internal(format!("缓存操作失败: {}", msg)),
          repository::RepositoryError::Database(msg) => opsbox_core::AppError::Internal(format!("数据库错误: {}", msg)),
        }
      }
    }
  }
}

/// 导出 router 函数（接收数据库连接池）
pub fn router(db_pool: SqlitePool) -> axum::Router {
  routes::router(db_pool)
}

/// 初始化 LogSeek 模块数据库 schema
pub async fn init_schema(db_pool: &SqlitePool) -> Result<()> {
  // 初始化 S3 配置表
  repository::settings::init_schema(db_pool)
    .await
    .map_err(|e| service::ServiceError::ProcessingError(
      format!("初始化 S3 配置表失败: {}", e)
    ))?;
  // 初始化 LLM 配置表
  repository::llm::init_schema(db_pool)
    .await
    .map_err(|e| service::ServiceError::ProcessingError(
      format!("初始化 LLM 配置表失败: {}", e)
    ))?;
  // 初始化 Planner 脚本表
  repository::planners::init_schema(db_pool)
    .await
    .map_err(|e| service::ServiceError::ProcessingError(
      format!("初始化 Planner 脚本表失败: {}", e)
    ))?;
  Ok(())
}

// ============================================================================
// 模块化架构：LogSeek 模块实现
// ============================================================================

/// LogSeek 模块
#[derive(Default)]
pub struct LogSeekModule;

#[async_trait::async_trait]
impl opsbox_core::Module for LogSeekModule {
  fn name(&self) -> &'static str {
    "LogSeek"
  }

  fn api_prefix(&self) -> &'static str {
    "/api/v1/logseek"
  }

  fn configure(&self) {
    // 从环境变量读取 S3 相关配置（若无则使用合理默认值）
    let s3_max_concurrency = std::env::var("LOGSEEK_S3_MAX_CONCURRENCY")
      .ok()
      .and_then(|v| v.parse().ok())
      .unwrap_or(12);

    let s3_timeout_sec = std::env::var("LOGSEEK_S3_TIMEOUT_SEC")
      .ok()
      .and_then(|v| v.parse().ok())
      .unwrap_or(60);

    let s3_max_retries = std::env::var("LOGSEEK_S3_MAX_RETRIES")
      .ok()
      .and_then(|v| v.parse().ok())
      .unwrap_or(5);

    let tuning = utils::tuning::Tuning {
      s3_max_concurrency,
      s3_timeout_sec,
      s3_max_retries,
    };

    log::debug!("LogSeek 模块配置: {:?}", tuning);
    utils::tuning::set(tuning);
  }

  async fn init_schema(&self, pool: &SqlitePool) -> std::result::Result<(), Box<dyn std::error::Error>> {
    // 使用 ? 操作符自动转换（通过 From<AppError> for Box<dyn Error> 实现）
    init_schema(pool).await?;
    Ok(())
  }

  fn router(&self, pool: SqlitePool) -> axum::Router {
    router(pool)
  }

  fn cleanup(&self) {
    repository::cache::Cache::stop_cleaner();
  }
}

// ✅ 使用 inventory 自动注册模块（编译时收集）
opsbox_core::register_module!(LogSeekModule);
