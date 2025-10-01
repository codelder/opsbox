pub mod routes;

pub mod query;
pub mod renderer;
mod search;
pub mod storage;

// BBIP 文件路径生成与查询字符串处理服务
pub mod bbip_service;

pub mod settings;
pub mod simple_cache;

// 自然语言转查询串服务（调用本地 Ollama）
pub mod nl2q;

// 运行时调参（由网关注入，避免使用环境变量注入）
pub mod tuning;

use opsbox_core::{Result, SqlitePool};

/// 导出 router 函数（接收数据库连接池）
pub fn router(db_pool: SqlitePool) -> axum::Router {
    routes::router(db_pool)
}

/// 初始化 LogSeek 模块数据库 schema
pub async fn init_schema(db_pool: &SqlitePool) -> Result<()> {
    settings::init_schema(db_pool).await
}
