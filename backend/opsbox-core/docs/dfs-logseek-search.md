# LogSeek 搜索功能实现方案（基于新 DFS 设计）

**版本**: 1.0
**日期**: 2026-02-02
**状态**: 设计草案

---

## 目录

1. [当前实现分析](#1-当前实现分析)
2. [新设计概览](#2-新设计概览)
3. [Searchable Trait 设计](#3-searchable-trait-设计)
4. [EntryStream 重构](#4-entrystream-重构)
5. [搜索服务层](#5-搜索服务层)
6. [实现示例](#6-实现示例)
7. [迁移指南](#7-迁移指南)

---

## 1. 当前实现分析

### 1.1 当前架构

```
┌─────────────────────────────────────────────────────────┐
│                  LogSeek 搜索 (当前实现)                  │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  SearchExecutor                                         │
│      │                                                  │
│      ├─── create_search_provider(orl)                  │
│      │        │                                         │
│      │        ├── EndpointType::Local → LocalOpsFS      │
│      │        ├── EndpointType::S3 → S3OpsFS            │
│      │        └── EndpointType::Agent → AgentSearchProvider│
│      │                                                  │
│      └─── search_with_entry_stream()                    │
│             │                                           │
│             ├── EntryStreamProcessor                    │
│             ├── SearchProcessor                         │
│             └── Result caching                          │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

### 1.2 核心组件

| 组件 | 位置 | 职责 |
|-----------|----------|-----------------|
| `SearchableFileSystem` | `logseek/src/service/searchable.rs` | 统一搜索接口 |
| `EntryStreamProcessor` | `logseek/src/service/entry_stream.rs` | 流处理 |
| `SearchExecutor` | `logseek/src/service/search_executor.rs` | 搜索编排 |
| `SearchProcessor` | `logseek/src/service/search.rs` | 查询匹配 |

### 1.3 当前问题

1. **与 ORL 强耦合**: 搜索提供者直接依赖 `ORL` 类型
2. **基于字符串的注册**: Provider 选择使用字符串键
3. **重复逻辑**: 归档处理逻辑分散在多个文件中
4. **缺乏类型安全**: 端点类型在运行时确定

---

## 2. 新设计概览

### 2.1 架构目标

1. **搜索与 DFS 实现解耦**: 搜索只依赖 DFS 接口
2. **类型安全的提供者选择**: 使用 `Endpoint` 而非字符串匹配
3. **统一的归档处理**: ArchiveFileSystem 处理所有归档类型
4. **可组合的搜索管道**: 模块化、可测试的组件

### 2.2 新架构

```
┌─────────────────────────────────────────────────────────┐
│                LogSeek 搜索 (新设计)                      │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  SearchService                                          │
│      │                                                  │
│      ├─── Resource                                      │
│      │        │                                         │
│      │        ▼                                         │
│      ├─── ResourceResolver                              │
│      │        │                                         │
│      │        ├─── Endpoint → FileSystem                 │
│      │        │                                         │
│      │        └─── ArchiveContext → ArchiveFileSystem   │
│      │                                                  │
│      ▼                                                  │
│  SearchPipeline                                         │
│      ├─── SourceDiscovery                               │
│      ├─── StreamCreation (Searchable trait)             │
│      ├─── ContentDecoding                               │
│      ├─── QueryMatching                                 │
│      └─── ResultCaching                                 │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

---

## 3. Searchable Trait 设计

### 3.1 核心 Trait 定义

```rust
use async_trait::async_trait;
use std::io;
use tokio::io::AsyncRead;

use crate::domain::{Resource, ResourcePath};

/// 搜索结果条目元数据
#[derive(Debug, Clone)]
pub struct SearchResultEntry {
    /// 条目路径（相对于搜索根目录）
    pub path: ResourcePath,

    /// 文件元数据
    pub metadata: FileMetadata,

    /// 条目读取器（可选，用于延迟加载）
    pub reader: Option<Box<dyn AsyncRead + Send + Unpin>>,
}

/// 搜索配置
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// 最大并发文件读取数
    pub max_concurrency: usize,

    /// 内容读取超时
    pub content_timeout: Duration,

    /// 最大搜索文件大小（0 = 无限制）
    pub max_file_size: usize,

    /// 是否跟随符号链接
    pub follow_symlinks: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            max_concurrency: (num_cpus::get() * 2).clamp(8, 32),
            content_timeout: Duration::from_secs(60),
            max_file_size: 100 * 1024 * 1024, // 100MB
            follow_symlinks: false,
        }
    }
}

/// Searchable trait - 提供优化的搜索能力
///
/// 此 trait 扩展 FileSystem 的搜索特定操作。
/// 实现可以提供针对其存储后端优化的搜索算法。
#[async_trait]
pub trait Searchable: Send + Sync {
    /// 创建搜索的条目流
    ///
    /// 返回可搜索的条目流。
    /// 这是搜索操作的主要入口点。
    ///
    /// # 参数
    /// * `path` - 搜索的根路径
    /// * `recursive` - 是否递归搜索
    /// * `config` - 搜索配置
    ///
    /// # 返回
    /// 搜索结果条目流
    async fn create_entry_stream(
        &self,
        path: &ResourcePath,
        recursive: bool,
        config: &SearchConfig,
    ) -> Result<Box<dyn SearchEntryStream>, SearchError>;

    /// 检查此文件系统是否支持优化的搜索
    ///
    /// 某些后端（如 S3）可能不支持高效的流式搜索。
    /// 在这种情况下，调用者应该回退到目录遍历。
    fn supports_streaming_search(&self) -> bool {
        true
    }

    /// 获取搜索统计信息（可选）
    ///
    /// 提供有关搜索操作的信息。
    fn search_stats(&self) -> Option<SearchStats> {
        None
    }
}

/// 搜索操作的条目流
#[async_trait]
pub trait SearchEntryStream: Send + Sync {
    /// 获取流中的下一个条目
    ///
    /// 流耗尽时返回 `None`。
    async fn next_entry(
        &mut self,
    ) -> Result<Option<SearchResultEntry>, SearchError>;

    /// 估计剩余条目数（可选）
    fn estimated_remaining(&self) -> Option<usize> {
        None
    }
}

/// 搜索统计信息
#[derive(Debug, Clone)]
pub struct SearchStats {
    pub entries_found: usize,
    pub entries_processed: usize,
    pub bytes_read: u64,
    pub duration: Duration,
}
```

### 3.2 错误类型

```rust
/// 搜索专用错误
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("I/O 错误: {0}")]
    Io(#[from] io::Error),

    #[error("路径未找到: {0}")]
    NotFound(ResourcePath),

    #[error("无效的搜索配置: {0}")]
    InvalidConfig(String),

    #[error("搜索超时: 超过 {0:?}")]
    Timeout(Duration),

    #[error("搜索已取消")]
    Cancelled,

    #[error("条目流错误: {0}")]
    StreamError(String),

    #[error("解码错误: {0}")]
    DecodingError(String),
}
```

---

## 4. EntryStream 重构

### 4.1 统一条目流

```rust
use futures::Stream;
use std::pin::Pin;

/// 统一条目流类型
pub type EntryStreamItem = Result<SearchResultEntry, SearchError>;

/// 动态条目流
pub type DynEntryStream = Pin<Box<dyn Stream<Item = EntryStreamItem> + Send>>;

/// Stream 转 SearchEntryStream trait 的适配器
pub struct StreamAdapter {
    stream: DynEntryStream,
}

impl StreamAdapter {
    pub fn new(stream: DynEntryStream) -> Self {
        Self { stream }
    }
}

#[async_trait]
impl SearchEntryStream for StreamAdapter {
    async fn next_entry(&mut self) -> Result<Option<SearchResultEntry>, SearchError> {
        use futures::StreamExt;
        self.stream.next().await.transpose()
    }
}
```

### 4.2 各 FileSystem 的实现

#### 本地文件系统

```rust
use crate::implementations::LocalFileSystem;
use walkdir::WalkDir;

#[async_trait]
impl Searchable for LocalFileSystem {
    async fn create_entry_stream(
        &self,
        path: &ResourcePath,
        recursive: bool,
        config: &SearchConfig,
    ) -> Result<Box<dyn SearchEntryStream>, SearchError> {
        let root_path = self.resolve_path(path)?;
        let max_depth = if recursive { usize::MAX } else { 1 };

        // 使用 walkdir 高效遍历目录
        let walker = WalkDir::new(root_path)
            .max_depth(max_depth)
            .follow_links(config.follow_symlinks)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                // 过滤超过 max_file_size 的文件
                if let Ok(metadata) = entry.metadata() {
                    metadata.len() <= config.max_file_size as u64
                } else {
                    true
                }
            });

        // 转换为异步流
        let stream = async_stream::stream! {
            for entry in walker {
                let metadata = entry.metadata().ok();
                let path = ResourcePath::from_path(entry.path());

                let search_entry = SearchResultEntry {
                    path: path.clone(),
                    metadata: FileMetadata::from_std(metadata),
                    reader: None, // 延迟加载
                };

                yield Ok(search_entry);
            }
        };

        Ok(Box::new(StreamAdapter::new(Box::pin(stream))))
    }

    fn supports_streaming_search(&self) -> bool {
        true
    }
}
```

#### S3 存储

```rust
use crate::implementations::S3Storage;
use aws_sdk_s3::types::Object;

#[async_trait]
impl Searchable for S3Storage {
    async fn create_entry_stream(
        &self,
        path: &ResourcePath,
        recursive: bool,
        config: &SearchConfig,
    ) -> Result<Box<dyn SearchEntryStream>, SearchError> {
        let prefix = self.s3_prefix(path)?;

        // 使用分页列出 S3 对象
        let stream = async_stream::stream! {
            let mut continuation_token = None;

            loop {
                let mut request = self.client
                    .list_objects_v2()
                    .bucket(&self.bucket)
                    .prefix(&prefix)
                    .max_keys(config.max_concurrency as i32);

                if let Some(token) = &continuation_token {
                    request = request.continuation_token(token);
                }

                match request.send().await {
                    Ok(response) => {
                        continuation_token = response.next_continuation_token();

                        for object in response.contents().unwrap_or(&[]) {
                            let metadata = FileMetadata::from_s3_object(object);

                            // 跳过目录（以 / 结尾的键）
                            if object.key().ends_with('/') {
                                continue;
                            }

                            let resource_path = ResourcePath::from_str(object.key());

                            yield Ok(SearchResultEntry {
                                path: resource_path,
                                metadata,
                                reader: None,
                            });
                        }

                        if continuation_token.is_none() {
                            break;
                        }
                    }
                    Err(e) => {
                        yield Err(SearchError::StreamError(e.to_string()));
                        break;
                    }
                }
            }
        };

        Ok(Box::new(StreamAdapter::new(Box::pin(stream))))
    }

    fn supports_streaming_search(&self) -> bool {
        true
    }
}
```

#### 归档文件系统

```rust
use crate::implementations::ArchiveFileSystem;

#[async_trait]
impl<F: FileSystem + Searchable> Searchable for ArchiveFileSystem<F> {
    async fn create_entry_stream(
        &self,
        path: &ResourcePath,
        recursive: bool,
        config: &SearchConfig,
    ) -> Result<Box<dyn SearchEntryStream>, SearchError> {
        // 获取归档
        let archive = self.get_archive().await?;

        // 列出归档中的条目
        let entries = archive.list_entries(path)?;

        // 转换为流
        let stream = async_stream::stream! {
            for entry in entries {
                let metadata = entry.metadata.clone();
                let resource_path = entry.path.clone();

                yield Ok(SearchResultEntry {
                    path: resource_path,
                    metadata,
                    reader: None,
                });
            }
        };

        Ok(Box::new(StreamAdapter::new(Box::pin(stream))))
    }

    fn supports_streaming_search(&self) -> bool {
        // 归档完全加载在内存中
        true
    }
}
```

---

## 5. 搜索服务层

### 5.1 搜索服务

```rust
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use std::sync::Arc;

/// 搜索请求
#[derive(Debug, Clone)]
pub struct SearchRequest {
    /// 查询字符串（正则表达式模式）
    pub query: String,

    /// 要搜索的资源
    pub resource: Resource,

    /// 搜索配置
    pub config: SearchConfig,

    /// 上下文行数（匹配行前后）
    pub context_lines: usize,

    /// 路径过滤器（glob 模式）
    pub path_includes: Vec<String>,
    pub path_excludes: Vec<String>,

    /// 编码覆盖
    pub encoding: Option<String>,
}

/// 搜索结果
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// 找到匹配的资源
    pub resource: Resource,

    /// 行号
    pub line_number: usize,

    /// 行内容
    pub content: String,

    /// 上下文行
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

/// 搜索事件
pub enum SearchEvent {
    Progress(SearchProgress),
    Found(SearchResult),
    Error(SearchError),
    Complete(SearchStats),
}

/// 搜索进度
#[derive(Debug, Clone)]
pub struct SearchProgress {
    pub entries_processed: usize,
    pub entries_found: usize,
    pub current_path: ResourcePath,
}

/// 搜索服务
pub struct SearchService {
    resolver: Arc<ResourceResolver>,
    encoding_detector: Arc<EncodingDetector>,
    query_parser: Arc<QueryParser>,
}

impl SearchService {
    pub fn new(
        resolver: Arc<ResourceResolver>,
    ) -> Self {
        Self {
            resolver,
            encoding_detector: Arc::new(EncodingDetector::new()),
            query_parser: Arc::new(QueryParser::new()),
        }
    }

    /// 执行搜索
    pub async fn search(
        &self,
        request: SearchRequest,
        cancel_token: CancellationToken,
    ) -> impl Stream<Item = SearchEvent> {
        let resolver = self.resolver.clone();
        let encoding_detector = self.encoding_detector.clone();
        let query_parser = self.query_parser.clone();

        async_stream::stream! {
            // 解析资源到文件系统
            let fs = match resolver.resolve(&request.resource).await {
                Ok(fs) => fs,
                Err(e) => {
                    yield SearchEvent::Error(SearchError::NotFound(request.resource.primary_path));
                    return;
                }
            };

            // 检查是否可搜索
            let searchable = match fs.as_any().downcast_ref::<dyn Searchable>() {
                Some(s) => s,
                None => {
                    // 回退到目录遍历
                    yield SearchEvent::Error(SearchError::StreamError(
                        "文件系统不支持搜索".to_string()
                    ));
                    return;
                }
            };

            // 确定搜索路径
            let search_path = match &request.resource.archive_context {
                Some(ctx) => &ctx.inner_path,
                None => &request.resource.primary_path,
            };

            // 创建条目流
            let mut entry_stream = match searchable.create_entry_stream(
                search_path,
                true,
                &request.config,
            ).await {
                Ok(stream) => stream,
                Err(e) => {
                    yield SearchEvent::Error(e);
                    return;
                }
            };

            // 使用并发控制处理条目
            let semaphore = Arc::new(Semaphore::new(request.config.max_concurrency));
            let mut tasks = FuturesUnordered::new();
            let mut stats = SearchStats::default();

            loop {
                // 检查取消
                if cancel_token.is_cancelled() {
                    yield SearchEvent::Error(SearchError::Cancelled);
                    break;
                }

                // 获取下一个条目
                let entry = match entry_stream.next_entry().await {
                    Ok(Some(e)) => e,
                    Ok(None) => break,
                    Err(e) => {
                        yield SearchEvent::Error(e);
                        continue;
                    }
                };

                // 更新进度
                stats.entries_processed += 1;
                yield SearchEvent::Progress(SearchProgress {
                    entries_processed: stats.entries_processed,
                    entries_found: stats.entries_found,
                    current_path: entry.path.clone(),
                });

                // 应用路径过滤器
                if !Self::matches_path_filters(&entry.path, &request.path_includes, &request.path_excludes) {
                    continue;
                }

                // 获取信号量许可
                let permit = match semaphore.clone().acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => break, // 信号量已关闭
                };

                // 克隆任务所需数据
                let fs_clone = fs.clone();
                let entry_path = entry.path.clone();
                let query = request.query.clone();
                let context_lines = request.context_lines;
                let encoding_override = request.encoding.clone();
                let encoding_detector = encoding_detector.clone();
                let query_parser = query_parser.clone();
                let cancel_token_clone = cancel_token.clone();

                // 生成搜索任务
                let task = tokio::spawn(async move {
                    let _permit = permit; // 持有许可直到任务完成

                    // 打开文件
                    let reader = fs_clone.open_read(&entry_path).await?;

                    // 检测编码
                    let encoding = match encoding_override {
                        Some(enc) => enc,
                        None => encoding_detector.detect(&reader).await?,
                    };

                    // 解码内容
                    let content = decode_content(reader, &encoding).await?;

                    // 执行查询
                    let matches = query_parser.execute(&content, &query, context_lines)?;

                    Ok::<Vec<SearchResult>, SearchError>(matches)
                });

                tasks.push(task);

                // 限制并发任务数
                if tasks.len() >= request.config.max_concurrency {
                    match tasks.next().await {
                        Some(Ok(Ok(results))) => {
                            stats.entries_found += results.len();
                            for result in results {
                                yield SearchEvent::Found(result);
                            }
                        }
                        Some(Ok(Err(e))) => {
                            yield SearchEvent::Error(e);
                        }
                        Some(Err(e)) => {
                            if e.is_cancelled() {
                                yield SearchEvent::Error(SearchError::Cancelled);
                                return;
                            }
                            // 任务 panic 或其他错误
                        }
                        None => break,
                    }
                }
            }

            // 收集剩余任务
            while let Some(task) = tasks.next().await {
                match task {
                    Ok(Ok(results)) => {
                        stats.entries_found += results.len();
                        for result in results {
                            yield SearchEvent::Found(result);
                        }
                    }
                    Ok(Err(e)) => {
                        yield SearchEvent::Error(e);
                    }
                    Err(_) => {}
                }
            }

            yield SearchEvent::Complete(stats);
        }
    }

    fn matches_path_filters(
        path: &ResourcePath,
        includes: &[String],
        excludes: &[String],
    ) -> bool {
        // 应用 includes
        if !includes.is_empty() {
            let matches = includes.iter().any(|pattern| {
                match glob::Pattern::new(pattern) {
                    Ok(glob) => glob.matches_path(&std::path::Path::new(path.as_str())),
                    Err(_) => false,
                }
            });
            if !matches {
                return false;
            }
        }

        // 应用 excludes
        if !excludes.is_empty() {
            let matches = excludes.iter().any(|pattern| {
                match glob::Pattern::new(pattern) {
                    Ok(glob) => glob.matches_path(&std::path::Path::new(path.as_str())),
                    Err(_) => false,
                }
            });
            if matches {
                return false;
            }
        }

        true
    }
}
```

### 5.2 搜索管道组件

```rust
/// 查询解析器
pub struct QueryParser {
    regex_cache: LruCache<String, Regex>,
}

impl QueryParser {
    pub fn new() -> Self {
        Self {
            regex_cache: LruCache::new(100),
        }
    }

    pub fn execute(
        &self,
        content: &str,
        query: &str,
        context_lines: usize,
    ) -> Result<Vec<SearchResult>, SearchError> {
        // 获取或编译正则
        let regex = self.get_or_compile_regex(query)?;

        let mut results = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            if regex.is_match(line) {
                let start = idx.saturating_sub(context_lines);
                let end = (idx + context_lines + 1).min(lines.len());

                results.push(SearchResult {
                    resource: Resource::default(), // 由调用者设置
                    line_number: idx + 1,
                    content: line.to_string(),
                    context_before: lines[start..idx].iter().map(|s| s.to_string()).collect(),
                    context_after: lines[idx+1..end].iter().map(|s| s.to_string()).collect(),
                });
            }
        }

        Ok(results)
    }

    fn get_or_compile_regex(&self, pattern: &str) -> Result<&Regex, SearchError> {
        if !self.regex_cache.contains(pattern) {
            let regex = Regex::new(pattern)
                .map_err(|e| SearchError::InvalidConfig(format!("无效的正则: {}", e)))?;
            self.regex_cache.put(pattern.to_string(), regex);
        }
        Ok(self.regex_cache.get(pattern).unwrap())
    }
}

/// 编码检测器
pub struct EncodingDetector {
    // 实现细节...
}

impl EncodingDetector {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn detect(
        &self,
        reader: &(impl AsyncRead + Unpin),
    ) -> Result<String, SearchError> {
        // 使用 chardetng 或类似工具
        Ok("UTF-8".to_string())
    }
}
```

---

## 6. 实现示例

### 6.1 基本搜索

```rust
use logseek::search::{SearchService, SearchRequest, SearchConfig};

async fn search_local_files() -> Result<()> {
    // 创建解析器
    let mut resolver = ResourceResolver::new();
    resolver.register(
        Endpoint::local_fs(),
        Arc::new(LocalFileSystem::new(None))
    );

    // 创建搜索服务
    let search_service = SearchService::new(Arc::new(resolver));

    // 创建搜索请求
    let resource = Resource {
        endpoint: Endpoint::local_fs(),
        primary_path: ResourcePath::from_str("/var/log"),
        archive_context: None,
    };

    let request = SearchRequest {
        query: r"ERROR.*timeout".to_string(),
        resource,
        config: SearchConfig::default(),
        context_lines: 2,
        path_includes: vec!["*.log".to_string()],
        path_excludes: vec!["*.gz".to_string()],
        encoding: None,
    };

    // 执行搜索
    let cancel_token = CancellationToken::new();
    let mut stream = search_service.search(request, cancel_token).await;

    // 处理结果
    while let Some(event) = stream.next().await {
        match event {
            SearchEvent::Found(result) => {
                println!("在 {} 行找到匹配: {:?}", result.line_number, result.resource);
            }
            SearchEvent::Progress(progress) => {
                println!("已处理: {}", progress.entries_processed);
            }
            SearchEvent::Complete(stats) => {
                println!("搜索完成: {} 个匹配", stats.entries_found);
            }
            SearchEvent::Error(e) => {
                eprintln!("搜索错误: {}", e);
            }
        }
    }

    Ok(())
}
```

### 6.2 归档内搜索

```rust
async fn search_archive() -> Result<()> {
    let resource = Resource {
        endpoint: Endpoint::local_fs(),
        primary_path: ResourcePath::from_str("/data/logs.tar.gz"),
        archive_context: Some(ArchiveContext {
            inner_path: ResourcePath::from_str(""),
            archive_type: Some(ArchiveType::TarGz),
        }),
    };

    let request = SearchRequest {
        query: r"FATAL".to_string(),
        resource,
        config: SearchConfig::default(),
        context_lines: 3,
        path_includes: vec!["*.log".to_string()],
        path_excludes: vec![],
        encoding: Some("UTF-8".to_string()),
    };

    // 执行搜索...
    Ok(())
}
```

### 6.3 搜索 S3

```rust
async fn search_s3() -> Result<()> {
    let resource = Resource {
        endpoint: Endpoint::s3("production".to_string()),
        primary_path: ResourcePath::from_str("logs/2024/"),
        archive_context: None,
    };

    let request = SearchRequest {
        query: r"Exception".to_string(),
        resource,
        config: SearchConfig {
            max_concurrency: 16, // S3 可以处理更高并发
            ..Default::default()
        },
        context_lines: 2,
        path_includes: vec!["*.log".to_string()],
        path_excludes: vec!["debug/*".to_string()],
        encoding: None,
    };

    // 执行搜索...
    Ok(())
}
```

---

## 7. 迁移指南

### 7.1 迁移步骤

#### 阶段 1: 添加 DFS 依赖

```toml
# logseek/Cargo.toml
[dependencies]
opsbox-core = { path = "../opsbox-core", features = ["dfs-v2"] }
```

#### 阶段 2: 更新搜索服务

```rust
// 旧代码
use opsbox_core::odfs::OrlManager;

// 新代码
use opsbox_core::dfs::{ResourceResolver, Resource, Endpoint};
```

#### 阶段 3: 重构 Searchable 实现

```rust
// 旧代码
impl SearchableFileSystem for LocalOpsFS {
    async fn search(&self, ctx: &SearchContext, req: &SearchRequest) -> Result<()> {
        // ...
    }
}

// 新代码
#[async_trait]
impl Searchable for LocalFileSystem {
    async fn create_entry_stream(
        &self,
        path: &ResourcePath,
        recursive: bool,
        config: &SearchConfig,
    ) -> Result<Box<dyn SearchEntryStream>, SearchError> {
        // ...
    }
}
```

#### 阶段 4: 更新 API 层

```rust
// 旧代码
pub async fn search(orl: &ORL, query: &str) -> Result<SearchResults>

// 新代码
pub async fn search(resource: &Resource, query: &str) -> Result<SearchResults>

// 兼容包装器
pub async fn search_orl(orl: &ORL, query: &str) -> Result<SearchResults> {
    let resource = orl_to_resource(orl)?;
    search(&resource, query).await
}
```

### 7.2 兼容层

```rust
/// 遗留 ORL API 的适配器
pub struct SearchAdapter {
    new_service: SearchService,
}

impl SearchAdapter {
    pub async fn search_orl(
        &self,
        orl: &ORL,
        query: &str,
        context: &SearchContext,
    ) -> Result<SearchResults> {
        let resource = self.orl_to_resource(orl)?;
        let request = SearchRequest {
            query: query.to_string(),
            resource,
            config: SearchConfig::default(),
            context_lines: context.context_lines,
            path_includes: context.path_includes.clone(),
            path_excludes: context.path_excludes.clone(),
            encoding: context.encoding.clone(),
        };

        self.new_service.search(request, context.cancel_token.clone()).await
    }

    fn orl_to_resource(&self, orl: &ORL) -> Result<Resource> {
        // 转换 ORL 为 Resource
        let endpoint = match orl.endpoint_type()? {
            EndpointType::Local => Endpoint::local_fs(),
            EndpointType::S3 => Endpoint::s3(orl.effective_id().to_string()),
            EndpointType::Agent => {
                // 从 ORL 解析 agent 信息
                let host = // 从 ORL 提取
                Endpoint::agent(host, port, orl.effective_id().to_string())
            }
        };

        let archive_context = if orl.target_type() == TargetType::Archive {
            Some(ArchiveContext {
                inner_path: ResourcePath::from_str(orl.entry_path().unwrap_or("")),
                archive_type: ArchiveType::from_extension(orl.path()),
            })
        } else {
            None
        };

        Ok(Resource {
            endpoint,
            primary_path: ResourcePath::from_str(orl.path()),
            archive_context,
        })
    }
}
```

---

## 附录

### A. 性能考虑

| 方面 | 建议 |
|------|------|
| **并发度** | CPU 密集型操作使用 CPU 核心数的 2-4 倍 |
| **S3** | 更高并发（16-32）以应对网络延迟 |
| **归档** | 较低并发（4-8），因为受内存限制 |
| **缓存** | 缓存归档内容和已编译的正则表达式 |

### B. 测试策略

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_search_local_files() {
        let service = setup_test_service();
        let resource = test_resource("tests/fixtures/logs");
        let results = service.search(search_request(resource)).await;

        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_search_in_archive() {
        let service = setup_test_service();
        let resource = test_archive_resource("tests/fixtures/data.tar");
        let results = service.search(search_request(resource)).await;

        assert!(!results.is_empty());
    }
}
```

---

**文档版本**: 1.0
**最后更新**: 2026-02-02
