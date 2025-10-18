//! OpsBox 核心共享库
//!
//! 提供所有模块共享的功能：
//! - 统一错误处理
//! - 数据库连接管理
//! - 标准 HTTP 响应
//! - 共享中间件

pub mod database;
pub mod error;
pub mod middleware;
pub mod module;
pub mod response;

// 重新导出常用类型
pub use database::{DatabaseConfig, health_check, init_pool, run_migration};
pub use error::{AppError, Result};
pub use module::{Module, get_all_modules};
pub use response::{SuccessResponse, created, no_content, ok, ok_message, ok_with_message};

// 重新导出 sqlx 类型以供模块使用
pub use sqlx::sqlite::SqlitePool;
