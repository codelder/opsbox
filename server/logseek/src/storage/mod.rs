// ============================================================================
// 存储抽象层 - 统一的存储和搜索接口
// ============================================================================
//
// 本模块提供两种存储模式的抽象：
// 1. DataSource (Pull 模式): 提供数据访问，Server 端执行搜索
// 2. SearchService (Push 模式): 远程执行搜索，返回结果
//

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::AsyncRead;

pub mod agent;
pub mod local;
pub mod targz;
// pub mod minio;  // 待实现

// ============================================================================
// 公共类型定义
// ============================================================================

/// 文件条目
///
/// 表示任何存储源中的一个文件或对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
  /// 文件路径或键
  pub path: String,

  /// 文件元数据
  pub metadata: FileMetadata,
}

/// 文件元数据
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileMetadata {
  /// 文件大小（字节）
  pub size: Option<u64>,

  /// 修改时间（Unix 时间戳）
  pub modified: Option<i64>,

  /// 内容类型
  pub content_type: Option<String>,
}

/// 搜索范围
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchScope {
  /// 搜索指定目录
  Directory { path: String, recursive: bool },

  /// 搜索指定文件列表
  Files { paths: Vec<String> },

  /// 搜索 tar.gz 文件
  TarGz { path: String },

  /// 搜索所有（由服务自己决定）
  All,
}

/// 搜索选项
#[derive(Debug, Clone)]
pub struct SearchOptions {
  /// 路径过滤
  pub path_filter: Option<String>,

  /// 搜索范围
  pub scope: SearchScope,

  /// 超时时间（秒）
  pub timeout_secs: Option<u64>,

  /// 最大结果数
  pub max_results: Option<usize>,
}

impl Default for SearchOptions {
  fn default() -> Self {
    Self {
      path_filter: None,
      scope: SearchScope::All,
      timeout_secs: Some(300),
      max_results: None,
    }
  }
}

/// 搜索进度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchProgress {
  pub task_id: String,
  pub processed_files: usize,
  pub matched_files: usize,
  pub total_files: Option<usize>,
  pub status: SearchStatus,
}

/// 搜索状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchStatus {
  Pending,
  Running,
  Completed,
  Failed(String),
  Cancelled,
}

/// 服务能力
#[derive(Debug, Clone, Default)]
pub struct ServiceCapabilities {
  /// 支持进度查询
  pub supports_progress: bool,

  /// 支持取消
  pub supports_cancellation: bool,

  /// 支持流式返回
  pub supports_streaming: bool,

  /// 最大并发搜索数
  pub max_concurrent_searches: usize,
}

// ============================================================================
// 类型别名
// ============================================================================

/// 文件迭代器
pub type FileIterator = Box<dyn Stream<Item = Result<FileEntry, StorageError>> + Send + Unpin>;

/// 文件读取器
pub type FileReader = Box<dyn AsyncRead + Send + Unpin>;

/// 搜索结果流
pub type SearchResultStream =
  Box<dyn Stream<Item = Result<crate::service::search::SearchResult, StorageError>> + Send + Unpin>;

// ============================================================================
// 存储错误
// ============================================================================

#[derive(Debug, Error)]
pub enum StorageError {
  #[error("IO错误: {0}")]
  Io(#[from] std::io::Error),

  #[error("权限被拒绝: {0}")]
  PermissionDenied(String),

  #[error("文件不存在: {0}")]
  NotFound(String),

  #[error("连接错误: {0}")]
  ConnectionError(String),

  #[error("Agent 不可用: {0}")]
  AgentUnavailable(String),

  #[error("超时")]
  Timeout,

  #[error("任务被取消")]
  Cancelled,

  #[error("查询解析错误: {0}")]
  QueryParseError(String),

  #[error("其他错误: {0}")]
  Other(String),
}

// ============================================================================
// DataSource trait - Pull 模式（Server 端搜索）
// ============================================================================

/// 数据源 trait
///
/// 本地文件系统、Tar.gz、MinIO 等实现此接口
/// 只负责提供数据访问，搜索逻辑由 Server 执行
#[async_trait]
pub trait DataSource: Send + Sync {
  /// 获取数据源类型
  fn source_type(&self) -> &'static str;

  /// 列举所有可搜索的文件
  ///
  /// 返回文件迭代器，Server 会遍历并搜索每个文件
  async fn list_files(&self) -> Result<FileIterator, StorageError>;

  /// 打开指定文件并返回可读流
  ///
  /// Server 会读取内容并执行搜索
  async fn open_file(&self, entry: &FileEntry) -> Result<FileReader, StorageError>;

  /// 可选：批量打开文件（性能优化）
  async fn open_files(&self, entries: &[FileEntry]) -> Result<Vec<FileReader>, StorageError> {
    let mut readers = Vec::new();
    for entry in entries {
      readers.push(self.open_file(entry).await?);
    }
    Ok(readers)
  }
}

// ============================================================================
// SearchService trait - Push 模式（远程搜索）
// ============================================================================

/// 搜索服务 trait
///
/// Agent 实现此接口
/// 在远程执行搜索，直接返回搜索结果
#[async_trait]
pub trait SearchService: Send + Sync {
  /// 获取服务类型
  fn service_type(&self) -> &'static str;

  /// 执行搜索
  ///
  /// query: 查询表达式
  /// context_lines: 上下文行数
  /// options: 搜索选项
  ///
  /// 返回搜索结果流（已经是最终结果，不需要 Server 再处理）
  async fn search(
    &self,
    query: &str,
    context_lines: usize,
    options: SearchOptions,
  ) -> Result<SearchResultStream, StorageError>;

  /// 获取搜索能力
  ///
  /// 返回该服务支持的特性
  fn capabilities(&self) -> ServiceCapabilities {
    ServiceCapabilities::default()
  }

  /// 可选：获取搜索进度
  async fn get_progress(&self, task_id: &str) -> Result<Option<SearchProgress>, StorageError> {
    let _ = task_id;
    Ok(None)
  }

  /// 可选：取消搜索
  async fn cancel(&self, task_id: &str) -> Result<(), StorageError> {
    let _ = task_id;
    Ok(())
  }
}

// ============================================================================
// 统一的存储源枚举
// ============================================================================

/// 存储源的类别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceCategory {
  /// 数据源：提供原始数据访问，搜索在 Server 执行
  DataSource,

  /// 搜索服务：在远程执行搜索，只返回结果
  SearchService,
}

/// 统一的存储源
///
/// 协调器使用此枚举来处理两种不同类型的源
pub enum StorageSource {
  /// 数据源（Server 端搜索）
  Data(std::sync::Arc<dyn DataSource>),

  /// 搜索服务（远程搜索）
  Service(std::sync::Arc<dyn SearchService>),
}

impl StorageSource {
  /// 获取类别
  pub fn category(&self) -> SourceCategory {
    match self {
      StorageSource::Data(_) => SourceCategory::DataSource,
      StorageSource::Service(_) => SourceCategory::SearchService,
    }
  }

  /// 获取类型名称
  pub fn type_name(&self) -> &'static str {
    match self {
      StorageSource::Data(source) => source.source_type(),
      StorageSource::Service(service) => service.service_type(),
    }
  }
}
