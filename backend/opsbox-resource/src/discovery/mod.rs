//! 发现端点连接器
//!
//! 提供虚拟发现功能的 EndpointConnector 实现。
//! 用于列出可用的 Agent 和 S3 Profile。

pub mod agent;
pub mod s3;

pub use agent::AgentDiscoveryEndpointConnector;
pub use s3::S3DiscoveryEndpointConnector;
