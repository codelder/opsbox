//! Processing 模块 - 内容处理框架
//!
//! 提供统一的内容处理抽象和条目流处理能力。
//!
//! # 核心组件
//!
//! - **ContentProcessor**: 内容处理器 trait，定义如何处理文件内容
//! - **ProcessedContent**: 处理后的内容结构
//! - **EntryStreamProcessor**: 条目流处理器，消费 EntryStream 并调用 ContentProcessor
//! - **PathFilter**: 路径过滤器，支持 glob 模式和字符串包含过滤
//!
//! # 示例
//!
//! ```ignore
//! use opsbox_core::processing::{ContentProcessor, EntryStreamProcessor, PathFilter};
//! use std::sync::Arc;
//!
//! // 实现内容处理器
//! struct MyProcessor;
//!
//! #[async_trait]
//! impl ContentProcessor for MyProcessor {
//!     async fn process_content(
//!         &self,
//!         path: String,
//!         reader: &mut Box<dyn AsyncRead + Send + Unpin>,
//!     ) -> io::Result<Option<ProcessedContent>> {
//!         // 处理文件内容...
//!         Ok(None)
//!     }
//! }
//!
//! // 创建处理器并处理条目流
//! let processor = Arc::new(MyProcessor);
//! let mut stream_processor = EntryStreamProcessor::new(processor);
//! stream_processor.process_stream(&mut entries, |result| async move {
//!     // 处理结果
//! }).await?;
//! ```

mod filter;
mod preload;
mod processor;
mod types;

// 导出核心类型
pub use filter::PathFilter;
pub use preload::{DEFAULT_PRELOAD_BUFFER_SIZE, PreloadResult, preload_entry};
pub use processor::EntryStreamProcessor;
pub use types::{ContentProcessor, ProcessedContent};
