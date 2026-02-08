//! DFS (Distributed File System) 模块
//!
//! 这个模块实现了 OpsBox 分布式文件系统的核心领域模型，包括：
//!
//! - **基础维度**: Location, StorageBackend, AccessMethod
//! - **端点概念**: Endpoint
//! - **路径抽象**: ResourcePath
//! - **归档概念**: ArchiveType, ArchiveContext
//! - **资源概念**: Resource
//! - **文件系统抽象**: OpbxFileSystem trait
//! - **文件系统创建**: create_fs 函数
//! - **文件系统实现**: LocalFileSystem
//! - **ORL 解析**: OrlParser

pub mod archive;
pub mod endpoint;
pub mod factory;
pub mod filesystem;
pub mod impls;
pub mod orl_parser;
pub mod path;
pub mod resource;

// 重新导出核心类型
pub use endpoint::{AccessMethod, Endpoint, Location, StorageBackend};
pub use archive::{ArchiveContext, ArchiveType};
pub use path::ResourcePath;
pub use resource::Resource;
pub use filesystem::{DirEntry, FileMetadata, FsError, OpbxFileSystem};
pub use factory::{create_fs, FsConfig};
pub use impls::{AgentClient, AgentProxyFS, ArchiveFileSystem, LocalFileSystem, S3Config, S3Storage};
pub use orl_parser::{OrlParser, OrlParseError};
