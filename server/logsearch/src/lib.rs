pub mod routes;
pub use routes::router;

pub mod query;
pub mod renderer;
mod search;
pub mod storage;

// BBIP 文件路径生成与查询字符串处理服务
pub mod bbip_service;
