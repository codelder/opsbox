//! API 类型定义
//!
//! 定义 API 请求和响应的类型

use std::sync::Arc;

use crate::config::AgentConfig;

// 从 opsbox-core 重新导出共享类型
#[allow(unused_imports)]
pub use opsbox_core::error::{AppError, Result};
pub use opsbox_core::logging::repository::LogConfigResponse;
pub use opsbox_core::logging::{UpdateLogLevelRequest, UpdateRetentionRequest};
pub use opsbox_core::response::SuccessResponse;

/// 应用状态
#[derive(Clone)]
pub struct AppState {
  pub config: Arc<AgentConfig>,
}

// 使用 opsbox-core 的 SuccessResponse<T>，T=() 表示无数据
// pub use opsbox_core::response::SuccessResponse; 已在上面重新导出

