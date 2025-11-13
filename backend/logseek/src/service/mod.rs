// 服务层：业务逻辑和外部服务集成

// pub mod coordinator; // 已弃用的 DataSource 协调器
pub mod entry_stream;
pub mod error;
pub mod nl2q;
pub mod search;
pub mod search_executor;

// 导出错误类型和 Result 别名
pub use error::{Result, ServiceError};
