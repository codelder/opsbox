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
pub mod routes_agent; // Agent 管理路由

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

// 存储抽象层
pub mod storage;

use opsbox_core::{Result, SqlitePool};

/// 导出 router 函数（接收数据库连接池）
pub fn router(db_pool: SqlitePool) -> axum::Router {
  routes::router(db_pool)
}

/// 初始化 LogSeek 模块数据库 schema
pub async fn init_schema(db_pool: &SqlitePool) -> Result<()> {
  repository::settings::init_schema(db_pool).await
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
    // 从环境变量读取配置
    let s3_max_concurrency = std::env::var("LOGSEEK_S3_MAX_CONCURRENCY")
      .ok()
      .and_then(|v| v.parse().ok())
      .unwrap_or(50);

    let cpu_concurrency = std::env::var("LOGSEEK_CPU_CONCURRENCY")
      .ok()
      .and_then(|v| v.parse().ok())
      .unwrap_or_else(|| num_cpus::get().min(16));

    let stream_ch_cap = std::env::var("LOGSEEK_STREAM_CH_CAP")
      .ok()
      .and_then(|v| v.parse().ok())
      .unwrap_or(32);

    let minio_timeout_sec = std::env::var("LOGSEEK_MINIO_TIMEOUT_SEC")
      .ok()
      .and_then(|v| v.parse().ok())
      .unwrap_or(60);

    let minio_max_attempts = std::env::var("LOGSEEK_MINIO_MAX_ATTEMPTS")
      .ok()
      .and_then(|v| v.parse().ok())
      .unwrap_or(3);

    let tuning = utils::tuning::Tuning {
      s3_max_concurrency,
      cpu_concurrency,
      stream_ch_cap,
      minio_timeout_sec,
      minio_max_attempts,
    };

    log::debug!("LogSeek 模块配置: {:?}", tuning);
    utils::tuning::set(tuning);
  }

  async fn init_schema(&self, pool: &SqlitePool) -> std::result::Result<(), Box<dyn std::error::Error>> {
    // 将 Result<()> 转换为 Result<(), Box<dyn Error>>
    init_schema(pool)
      .await
      .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
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
