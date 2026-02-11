//! DFS Search 模块
//!
//! 提供基于 DFS 的搜索能力，包括：
//! - ContentProcessor trait：内容处理器抽象
//! - EntryStreamProcessor：条目流处理器
//! - PathFilter：路径过滤器

mod processor;
mod types;

pub use processor::{EntryStreamProcessor, PathFilter};
pub use types::{ContentProcessor, ProcessedContent};
