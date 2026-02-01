//! # Agent Context
//!
//! 代理管理领域模型。
//!
//! ## 核心概念
//!
//! - **Agent**: Agent 聚合根，封装代理行为
//! - **AgentConnection**: 连接信息值对象
//! - **AgentTag**: 标签值对象
//! - **AgentCapabilities**: 能力值对象

mod value_objects;
mod entities;

pub use value_objects::*;
pub use entities::*;
