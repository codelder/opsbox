//! # Search Context
//!
//! 搜索执行领域模型。
//!
//! ## 核心概念
//!
//! - **SearchSession**: 搜索会话聚合根
//! - **ParsedQuery**: 解析后的查询值对象
//! - **QueryExpression**: 查询表达式树

mod value_objects;
mod entities;

pub use value_objects::*;
pub use entities::*;
