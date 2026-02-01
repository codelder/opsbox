//! # Resource Context
//!
//! 资源访问领域模型。
//!
//! ## 核心概念
//!
//! - **ResourceIdentifier**: 类型安全的资源标识符（替代字符串 ORL）
//! - **Resource**: 聚合根，封装资源访问行为
//! - **EndpointConnector**: 端点连接器抽象
//! - **EndpointRegistry**: 端点注册表

mod value_objects;
mod entities;

pub use value_objects::*;
pub use entities::*;
