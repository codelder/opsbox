//! OpsBox Agent 库
//!
//! 导出用于测试的类型和函数

pub mod api;
pub mod config;
pub mod path;
pub mod routes;
pub mod search;
pub mod server;

// 重新导出需要的类型
pub use api::{AppError, AppState, LogConfigResponse, SuccessResponse, UpdateLogLevelRequest, UpdateRetentionRequest};
pub use opsbox_core::error::Result;
pub use config::AgentConfig;
