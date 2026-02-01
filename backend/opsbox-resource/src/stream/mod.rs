//! 条目流抽象
//!
//! 从 opsbox-core 迁移 EntryStream 相关功能。
//!
//! 用于搜索功能的流式文件遍历接口。

use std::io;
use tokio::io::AsyncRead;

pub use self::local::FsEntryStream;
pub use self::s3::S3EntryStream;
pub use self::archive::{TarGzEntryStream, ArchiveEntryStream};

/// 条目来源类型
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum EntrySource {
    /// 普通文件（目录遍历或单文件）
    #[default]
    File,
    /// tar 归档内的条目
    Tar,
    /// tar.gz 归档内的条目
    TarGz,
    /// 纯 gzip 压缩文件（非 tar 归档）
    Gz,
}

/// 条目元数据
#[derive(Clone, Debug)]
pub struct EntryMeta {
    /// 条目路径
    pub path: String,
    /// 归档容器路径（如果条目来自归档内部）
    pub container_path: Option<String>,
    /// 文件大小
    pub size: Option<u64>,
    /// 是否已压缩
    pub is_compressed: bool,
    /// 条目来源类型
    pub source: EntrySource,
}

/// 统一的"条目流"抽象
///
/// 每次产出 (EntryMeta, Reader)，用于流式遍历文件进行搜索。
///
/// 注意：不使用 'static 生命周期以匹配 opsbox_core::fs::EntryStream 的签名。
/// 这样可以确保两个 EntryStream trait 兼容。
#[async_trait::async_trait]
pub trait EntryStream: Send {
    /// 获取下一个条目
    ///
    /// 返回 `None` 表示流已结束。
    async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>>;
}

pub mod local;
pub mod s3;
pub mod archive;
pub mod utils;
