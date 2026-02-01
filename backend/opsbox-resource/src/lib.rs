//! # OpsBox Resource Implementation Layer
//!
//! 资源访问实现层，委托给现有的 OpsFileSystem 基础设施。
//!
//! ## 架构
//!
//! ```text
//! 应用层
//!     |
//!     v
//! opsbox-domain (EndpointConnector trait)
//!     |
//!     v
//! opsbox-resource (具体实现) -> opsbox-core::odfs (OpsFileSystem)
//! ```

pub mod local;
pub mod s3;
pub mod agent;
pub mod archive;

pub use local::LocalEndpointConnector;
pub use s3::S3EndpointConnector;
pub use agent::AgentEndpointConnector;
