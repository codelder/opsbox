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
pub mod routes;  // 保留以保持向后兼容

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

use opsbox_core::{Result, SqlitePool};

/// 导出 router 函数（接收数据库连接池）
pub fn router(db_pool: SqlitePool) -> axum::Router {
    routes::router(db_pool)
}

/// 初始化 LogSeek 模块数据库 schema
pub async fn init_schema(db_pool: &SqlitePool) -> Result<()> {
    repository::settings::init_schema(db_pool).await
}
