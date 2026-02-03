# Agent 搜索功能实现方案

**版本**: 1.0
**日期**: 2026-02-02
**状态**: 设计草案

---

## 目录

1. [Agent 搜索架构分析](#1-agent-搜索架构分析)
2. [设计原则](#2-设计原则)
3. [Agent 端实现](#3-agent-端实现)
4. [Server 端实现](#4-server-端实现)
5. [搜索协议设计](#5-搜索协议设计)
6. [完整代码示例](#6-完整代码示例)
7. [性能优化](#7-性能优化)

---

## 1. Agent 搜索架构分析

### 1.1 Agent 的本质

在新 DFS 设计中，Agent 的定位是：

```
Agent = Remote + FileSystem + Proxy
```

**关键洞察**：
- Agent 在其运行的服务器上就是 **Local**
- 对 Server 来说，Agent 是一个 **代理**
- Agent 搜索 = 将搜索请求代理到远程 Agent 执行

### 1.2 架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                        Agent 搜索架构                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Server 端 (opsbox-server)                                      │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  SearchService                                           │   │
│  │      │                                                   │   │
│  │      ├─── Resource: agent.web-01@agent.192.168.1.100/logs│   │
│  │      │                                                   │   │
│  │      ├─── ResourceResolver → AgentProxyFS              │   │
│  │      │                                                   │   │
│  │      └─── AgentProxyFS::create_entry_stream()           │   │
│  │              │                                          │   │
│  │              ▼                                          │   │
│  │      ┌─────────────────────────────────────────────┐   │   │
│  │      │ HTTP POST /api/v1/agent/{id}/search/start   │   │   │
│  │      │ Body: SearchRequest (serialized)            │   │   │
│  │      └─────────────────────────────────────────────┘   │   │
│  └──────────────────────────────────────────────────────────┘   │
│                           │ HTTP                                │
│                           ▼                                     │
│  Agent 端 (agent binary)                                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  /api/v1/agent/search/start                              │   │
│  │      │                                                   │   │
│  │      ▼                                                   │   │
│  │  LocalSearchService                                       │   │
│  │      │                                                   │   │
│  │      ├─── LocalFileSystem::create_entry_stream()        │   │
│  │      │                                                   │   │
│  │      ├─── Search in local files                         │   │
│  │      │                                                   │   │
│  │      └─── Stream results via HTTP                       │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 1.3 两种搜索模式

| 模式 | 描述 | 使用场景 |
|------|------|---------|
| **代理模式** | Server 发送搜索请求到 Agent，Agent 执行搜索并流式返回结果 | 大规模日志搜索，Agent 负载较低时 |
| **回拉模式** | Server 获取文件列表，然后逐个拉取文件内容进行搜索 | 小规模搜索，或 Agent 负载较高时 |

---

## 2. 设计原则

### 2.1 核心原则

1. **Agent 端执行搜索**：搜索逻辑在 Agent 端执行，减少网络传输
2. **流式结果返回**：使用 Server-Sent Events (SSE) 或分块传输返回结果
3. **可取消操作**：支持搜索取消，释放 Agent 资源
4. **资源隔离**：限制 Agent 搜索的并发度，避免影响 Agent 主机
5. **错误隔离**：单个 Agent 搜索失败不应影响其他 Agent

### 2.2 接口设计原则

```rust
// AgentProxyFS 实现统一的 Searchable trait
impl Searchable for AgentProxyFS {
    async fn create_entry_stream(
        &self,
        path: &ResourcePath,
        recursive: bool,
        config: &SearchConfig,
    ) -> Result<Box<dyn SearchEntryStream>, SearchError> {
        // 通过 HTTP 代理到 Agent
    }
}
```

**好处**：
- 对上层调用者透明，不需要知道是通过 Agent 搜索
- 统一的错误处理和日志记录
- 易于测试和模拟

---

## 3. Agent 端实现

### 3.1 Agent 搜索服务

```rust
use axum::{
    extract::{Path, State},
    response::{sse::Event, IntoResponse, Sse},
    Json,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;

/// Agent 搜索服务
pub struct AgentSearchService {
    local_search: Arc<LocalSearchService>,
    active_searches: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl AgentSearchService {
    pub fn new() -> Self {
        Self {
            local_search: Arc::new(LocalSearchService::new()),
            active_searches: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 开始搜索
    pub async fn start_search(
        &self,
        request: AgentSearchRequest,
    ) -> Result<impl Stream<Item = Event>, AgentError> {
        // 生成搜索 ID
        let search_id = Uuid::new_v4().to_string();
        let cancel_token = CancellationToken::new();

        // 注册搜索
        self.active_searches
            .lock()
            .await
            .insert(search_id.clone(), cancel_token.clone());

        // 执行搜索
        let search_service = self.local_search.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        tokio::spawn(async move {
            let resource = Resource {
                endpoint: Endpoint::local_fs(),
                primary_path: request.path,
                archive_context: request.archive_context,
            };

            let search_request = SearchRequest {
                query: request.query,
                resource,
                config: request.config.unwrap_or_default(),
                context_lines: request.context_lines,
                path_includes: request.path_includes,
                path_excludes: request.path_excludes,
                encoding: request.encoding,
            };

            let mut stream = search_service.search(search_request, cancel_token).await;

            while let Some(event) = stream.next().await {
                let event_json = match serde_json::to_string(&event) {
                    Ok(json) => json,
                    Err(e) => {
                        tracing::error!("Failed to serialize search event: {}", e);
                        continue;
                    }
                };

                if tx.send(Event::default().data(event_json)).await.is_err() {
                    break;
                }
            }
        });

        // 创建 SSE 流
        let stream = ReceiverStream::new(rx).map(|event| {
            Event::default().data(event)
        });

        Ok(stream)
    }

    /// 取消搜索
    pub async fn cancel_search(&self, search_id: &str) -> Result<(), AgentError> {
        if let Some(token) = self.active_searches.lock().await.remove(search_id) {
            token.cancel();
            Ok(())
        } else {
            Err(AgentError::SearchNotFound(search_id.to_string()))
        }
    }
}

/// Agent 搜索请求
#[derive(Debug, Clone, Deserialize)]
pub struct AgentSearchRequest {
    /// 搜索路径
    pub path: ResourcePath,

    /// 查询字符串（正则表达式）
    pub query: String,

    /// 上下文行数
    #[serde(default = "default_context_lines")]
    pub context_lines: usize,

    /// 搜索配置
    pub config: Option<SearchConfig>,

    /// 路径过滤器
    #[serde(default)]
    pub path_includes: Vec<String>,

    #[serde(default)]
    pub path_excludes: Vec<String>,

    /// 编码覆盖
    pub encoding: Option<String>,

    /// 归档上下文
    pub archive_context: Option<ArchiveContext>,
}

fn default_context_lines() -> usize {
    2
}

/// Agent 搜索响应事件
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum AgentSearchEvent {
    #[serde(rename = "progress")]
    Progress {
        entries_processed: usize,
        entries_found: usize,
        current_path: String,
    },

    #[serde(rename = "found")]
    Found {
        resource: Resource,
        line_number: usize,
        content: String,
        context_before: Vec<String>,
        context_after: Vec<String>,
    },

    #[serde(rename = "error")]
    Error {
        message: String,
    },

    #[serde(rename = "complete")]
    Complete {
        entries_found: usize,
        entries_processed: usize,
        duration_ms: u64,
    },
}
```

### 3.2 Agent HTTP 路由

```rust
use axum::{
    routing::{get, post},
    Router,
};

pub fn create_search_router() -> Router {
    Router::new()
        .route("/api/v1/search/start", post(start_search))
        .route("/api/v1/search/cancel/:id", post(cancel_search))
        .route("/api/v1/search/status/:id", get(search_status))
        .with_state(Arc::new(AgentSearchService::new()))
}

/// 开始搜索
async fn start_search(
    State(service): State<Arc<AgentSearchService>>,
    Json(request): Json<AgentSearchRequest>,
) -> impl IntoResponse {
    match service.start_search(request).await {
        Ok(stream) => Sse::new(stream).keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(std::time::Duration::from_secs(1)),
        ).into_response(),
        Err(e) => {
            let error = AgentSearchEvent::Error {
                message: e.to_string(),
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
        }
    }
}

/// 取消搜索
async fn cancel_search(
    State(service): State<Arc<AgentSearchService>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match service.cancel_search(&id).await {
        Ok(()) => (StatusCode::OK, "Search cancelled").into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    }
}

/// 搜索状态
async fn search_status(
    State(service): State<Arc<AgentSearchService>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // 返回搜索状态
    (StatusCode::OK, Json(json!({ "id": id, "status": "running" }))).into_response()
}
```

---

## 4. Server 端实现

### 4.1 AgentProxyFS Searchable 实现

```rust
use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;

/// Agent 代理文件系统
pub struct AgentProxyFS {
    agent_id: String,
    base_url: String,
    client: Client,
}

impl AgentProxyFS {
    pub fn new(agent_id: String, base_url: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            agent_id,
            base_url,
            client,
        }
    }

    /// 获取 Agent 搜索 URL
    fn search_url(&self) -> String {
        format!("{}/api/v1/search/start", self.base_url)
    }

    /// 发送搜索请求到 Agent
    async fn start_search(
        &self,
        request: AgentSearchRequest,
    ) -> Result<impl Stream<Item = Result<AgentSearchEvent, reqwest::Error>>, AgentError> {
        let url = self.search_url();
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::ConnectionError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::SearchFailed(format!(
                "Agent returned {}: {}",
                status, error_text
            )));
        }

        // 转换为 SSE 流
        let stream = response.bytes_stream().map(|chunk| {
            chunk
                .map_err(|e| reqwest::Error::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e,
                )))
                .and_then(|bytes| {
                    // 解析 SSE 事件
                    parse_sse_event(&bytes)
                })
        });

        Ok(stream)
    }
}

/// 解析 SSE 事件
fn parse_sse_event(bytes: &[u8]) -> Result<AgentSearchEvent, reqwest::Error> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| reqwest::Error::from(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e,
        )))?;

    // SSE 格式: "data: {json}\n\n"
    let json_str = text
        .strip_prefix("data: ")
        .and_then(|s| s.strip_suffix("\n\n"))
        .unwrap_or(text);

    serde_json::from_str(json_str)
        .map_err(|e| reqwest::Error::from(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e,
        )))
}

#[async_trait]
impl Searchable for AgentProxyFS {
    async fn create_entry_stream(
        &self,
        path: &ResourcePath,
        recursive: bool,
        config: &SearchConfig,
    ) -> Result<Box<dyn SearchEntryStream>, SearchError> {
        // 构造 Agent 搜索请求
        let request = AgentSearchRequest {
            path: path.clone(),
            query: ".*".to_string(), // 返回所有文件
            context_lines: 0,
            config: Some(config.clone()),
            path_includes: vec![],
            path_excludes: vec![],
            encoding: None,
            archive_context: None,
        };

        // 通过 HTTP 获取文件列表流
        let event_stream = self.start_search(request).await?;

        // 将 Agent 搜索事件转换为 SearchResultEntry 流
        let stream = async_stream::stream! {
            let mut event_stream = event_stream;

            while let Some(event_result) = event_stream.next().await {
                let event = event_result.map_err(|e| {
                    SearchError::StreamError(format!("Agent stream error: {}", e))
                })?;

                match event {
                    AgentSearchEvent::Found { resource, line_number, content, .. } => {
                        // 转换为 SearchResultEntry
                        let entry = SearchResultEntry {
                            path: resource.primary_path,
                            metadata: FileMetadata {
                                size: None,
                                modified: None,
                                file_type: FileType::File,
                            },
                            reader: None,
                        };

                        yield Ok(entry);

                        // 收到一个结果后就结束（我们只需要文件列表）
                        break;
                    }
                    AgentSearchEvent::Complete { .. } => {
                        break;
                    }
                    AgentSearchEvent::Error { message } => {
                        yield Err(SearchError::StreamError(message));
                        break;
                    }
                    _ => {}
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

### 4.2 AgentSearchStream - 专用的 Agent 搜索流

```rust
/// Agent 搜索的专用流实现
pub struct AgentSearchStream {
    agent_id: String,
    base_url: String,
    client: Client,
    search_id: String,
}

impl AgentSearchStream {
    pub async fn new(
        agent_id: String,
        base_url: String,
        request: AgentSearchRequest,
    ) -> Result<Self, SearchError> {
        let client = Client::new();
        let url = format!("{}/api/v1/search/start", base_url);

        let response = client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| SearchError::StreamError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(SearchError::StreamError(format!(
                "Agent returned {}",
                response.status()
            )));
        }

        // 获取搜索 ID
        let search_id = response
            .headers()
            .get("X-Search-ID")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        Ok(Self {
            agent_id,
            base_url,
            client,
            search_id,
        })
    }

    /// 取消搜索
    pub async fn cancel(&self) -> Result<(), SearchError> {
        let url = format!(
            "{}/api/v1/search/cancel/{}",
            self.base_url, self.search_id
        );

        self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| SearchError::Cancelled)?;

        Ok(())
    }
}

#[async_trait]
impl SearchEntryStream for AgentSearchStream {
    async fn next_entry(&mut self) -> Result<Option<SearchResultEntry>, SearchError> {
        // 从 Agent 获取下一个搜索结果
        let url = format!(
            "{}/api/v1/search/next/{}",
            self.base_url, self.search_id
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SearchError::StreamError(e.to_string()))?;

        if response.status() == StatusCode::NOT_FOUND {
            // 搜索结束
            return Ok(None);
        }

        if !response.status().is_success() {
            return Err(SearchError::StreamError(format!(
                "Agent returned {}",
                response.status()
            )));
        }

        let event: AgentSearchEvent = response
            .json()
            .await
            .map_err(|e| SearchError::StreamError(e.to_string()))?;

        match event {
            AgentSearchEvent::Found { resource, line_number, content, .. } => {
                Ok(Some(SearchResultEntry {
                    path: resource.primary_path,
                    metadata: FileMetadata {
                        size: None,
                        modified: None,
                        file_type: FileType::File,
                    },
                    reader: None,
                }))
            }
            AgentSearchEvent::Complete { .. } => Ok(None),
            AgentSearchEvent::Error { message } => Err(SearchError::StreamError(message)),
            _ => {
                // 其他事件类型，继续
                Box::pin(self.next_entry()).await
            }
        }
    }

    fn estimated_remaining(&self) -> Option<usize> {
        None // Agent 无法估计
    }
}
```

### 4.3 AgentProxyFS 优化版本

```rust
/// Agent 代理文件系统（优化版）
pub struct AgentProxyFS {
    agent_id: String,
    base_url: String,
    client: Client,
    /// 文件列表缓存
    file_list_cache: Arc<Mutex<LruCache<ResourcePath, Vec<FileMetadata>>>>,
}

impl AgentProxyFS {
    pub fn new(agent_id: String, base_url: String) -> Self {
        let client = Client::builder()
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(90))
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            agent_id,
            base_url,
            client,
            file_list_cache: Arc::new(Mutex::new(LruCache::new(100))),
        }
    }

    /// 优化的文件列表获取（带缓存）
    pub async fn list_files_cached(
        &self,
        path: &ResourcePath,
    ) -> Result<Vec<FileMetadata>, AgentError> {
        // 检查缓存
        {
            let cache = self.file_list_cache.lock().await;
            if let Some(cached) = cache.get(path) {
                return Ok(cached.clone());
            }
        }

        // 通过 HTTP 获取文件列表
        let url = format!("{}/api/v1/fs/list", self.base_url);
        let response = self.client
            .post(&url)
            .json(&serde_json::json!({
                "path": path.as_str(),
            }))
            .send()
            .await
            .map_err(|e| AgentError::ConnectionError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AgentError::ListFailed(format!(
                "Agent returned {}",
                response.status()
            )));
        }

        let list_response: AgentListResponse = response
            .json()
            .await
            .map_err(|e| AgentError::InvalidResponse(e.to_string()))?;

        let files = list_response.items;

        // 更新缓存
        {
            let mut cache = self.file_list_cache.lock().await;
            cache.put(path.clone(), files.clone());
        }

        Ok(files)
    }

    /// 执行搜索（专用接口）
    pub async fn search_files(
        &self,
        path: &ResourcePath,
        query: &str,
        config: &SearchConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<SearchResult, SearchError>> + Send>>, AgentError> {
        let request = AgentSearchRequest {
            path: path.clone(),
            query: query.to_string(),
            context_lines: config.context_lines.unwrap_or(2),
            config: Some(config.clone()),
            path_includes: vec![],
            path_excludes: vec![],
            encoding: None,
            archive_context: None,
        };

        let url = format!("{}/api/v1/search/start", self.base_url);

        // 发送搜索请求
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::ConnectionError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AgentError::SearchFailed(format!(
                "Agent returned {}",
                response.status()
            )));
        }

        // 转换响应为流
        let byte_stream = response.bytes_stream();
        let stream = byte_stream.map(|chunk| {
            chunk
                .map_err(|e| SearchError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))
                .and_then(|bytes| {
                    // 解析 SSE 事件中的搜索结果
                    parse_agent_search_event(&bytes)
                })
        });

        Ok(Box::pin(stream))
    }
}

/// 解析 Agent 搜索事件
fn parse_agent_search_event(bytes: &[u8]) -> Result<SearchResult, SearchError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| SearchError::DecodingError(e.to_string()))?;

    let event: AgentSearchEvent = serde_json::from_str(text)
        .map_err(|e| SearchError::DecodingError(e.to_string()))?;

    match event {
        AgentSearchEvent::Found {
            resource,
            line_number,
            content,
            context_before,
            context_after,
        } => Ok(SearchResult {
            resource,
            line_number,
            content,
            context_before,
            context_after,
        }),
        AgentSearchEvent::Complete { .. } => {
            // 搜索完成，返回特殊标记
            Err(SearchError::StreamError("Search complete".to_string()))
        }
        AgentSearchEvent::Error { message } => {
            Err(SearchError::StreamError(message))
        }
        AgentSearchEvent::Progress { .. } => {
            // 进度事件，跳过
            Err(SearchError::StreamError("Progress event".to_string()))
        }
    }
}

/// Agent 文件列表响应
#[derive(Debug, Deserialize)]
struct AgentListResponse {
    items: Vec<FileMetadata>,
}
```

---

## 5. 搜索协议设计

### 5.1 搜索请求协议

```json
// POST /api/v1/agent/{agent_id}/search/start
{
  "path": "/var/log",
  "query": "ERROR.*timeout",
  "context_lines": 2,
  "config": {
    "max_concurrency": 16,
    "max_file_size": 104857600,
    "follow_symlinks": false
  },
  "path_includes": ["*.log"],
  "path_excludes": ["*.gz"],
  "encoding": null,
  "archive_context": null
}

// Response
// SSE 流，每个事件格式：
data: {"type":"progress","entries_processed":100,"entries_found":5,"current_path":"/var/log/app.log"}

data: {"type":"found","resource":{"endpoint":{...},"primary_path":"/var/log/app.log","archive_context":null},"line_number":42,"content":"ERROR: connection timeout","context_before":["[INFO] Starting request","[DEBUG] Connecting to server"],"context_after":["[INFO] Retrying..."]}

data: {"type":"complete","entries_found":10,"entries_processed":500,"duration_ms":1523}
```

### 5.2 搜索控制协议

```json
// POST /api/v1/agent/{agent_id}/search/cancel/{search_id}
// Response: 200 OK

// GET /api/v1/agent/{agent_id}/search/status/{search_id}
{
  "id": "uuid",
  "status": "running",  // running, completed, cancelled, failed
  "entries_processed": 100,
  "entries_found": 5,
  "started_at": "2026-02-02T10:30:00Z"
}
```

---

## 6. 完整代码示例

### 6.1 Server 端：通过 Agent 搜索

```rust
use opsbox_core::dfs::{
    ResourceResolver, Resource, Endpoint, ResourcePath,
    services::{SearchService, SearchRequest, SearchConfig},
};
use tokio_util::sync::CancellationToken;

async fn search_via_agent() -> Result<()> {
    // 创建解析器并注册 Agent
    let mut resolver = ResourceResolver::new();
    resolver.register(
        Endpoint::agent("192.168.1.100".to_string(), 4001, "web-01".to_string()),
        Arc::new(AgentProxyFS::new("web-01".to_string(), "http://192.168.1.100:4001".to_string()))
    );

    // 创建搜索服务
    let search_service = SearchService::new(Arc::new(resolver));

    // 创建搜索请求（指向 Agent）
    let resource = Resource {
        endpoint: Endpoint::agent("192.168.1.100".to_string(), 4001, "web-01".to_string()),
        primary_path: ResourcePath::from_str("/var/log"),
        archive_context: None,
    };

    let request = SearchRequest {
        query: r"ERROR.*timeout".to_string(),
        resource,
        config: SearchConfig {
            max_concurrency: 16, // Agent 可以处理更高并发
            ..Default::default()
        },
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
                println!("在 Agent {} 的 {} 行找到匹配: {}",
                    result.resource.endpoint.identity,
                    result.line_number,
                    result.content
                );
            }
            SearchEvent::Progress(progress) => {
                println!("已处理: {}", progress.entries_processed);
            }
            SearchEvent::Complete(stats) => {
                println!("搜索完成: {} 个匹配，耗时 {}ms",
                    stats.entries_found,
                    stats.duration.as_millis()
                );
            }
            SearchEvent::Error(e) => {
                eprintln!("搜索错误: {}", e);
            }
        }
    }

    Ok(())
}
```

### 6.2 搜索 Agent 上的归档

```rust
async fn search_agent_archive() -> Result<()> {
    // Agent 上的归档文件
    let resource = Resource {
        endpoint: Endpoint::agent("192.168.1.100".to_string(), 4001, "web-01".to_string()),
        primary_path: ResourcePath::from_str("/data/logs.tar.gz"),
        archive_context: Some(ArchiveContext {
            inner_path: ResourcePath::from_str(""),
            archive_type: Some(ArchiveType::TarGz),
        }),
    };

    let request = SearchRequest {
        query: r"FATAL".to_string(),
        resource,
        config: SearchConfig {
            max_concurrency: 8, // 归档搜索降低并发
            ..Default::default()
        },
        context_lines: 3,
        path_includes: vec!["*.log".to_string()],
        path_excludes: vec![],
        encoding: Some("UTF-8".to_string()),
    };

    // 执行搜索...
    Ok(())
}
```

### 6.3 多 Agent 并行搜索

```rust
use futures::stream::{StreamExt, FuturesUnordered};

async fn search_multiple_agents() -> Result<()> {
    let agent_endpoints = vec![
        Endpoint::agent("192.168.1.100".to_string(), 4001, "web-01".to_string()),
        Endpoint::agent("192.168.1.101".to_string(), 4001, "web-02".to_string()),
        Endpoint::agent("192.168.1.102".to_string(), 4001, "web-03".to_string()),
    ];

    // 创建解析器
    let mut resolver = ResourceResolver::new();
    for endpoint in &agent_endpoints {
        resolver.register(
            endpoint.clone(),
            Arc::new(AgentProxyFS::new(
                endpoint.identity.clone(),
                format!("http://{}:4001", endpoint.get_host())
            ))
        );
    }

    let search_service = SearchService::new(Arc::new(resolver));
    let cancel_token = CancellationToken::new();

    // 并行搜索多个 Agent
    let mut tasks = FuturesUnordered::new();

    for endpoint in agent_endpoints {
        let service = search_service.clone();
        let token = cancel_token.clone();

        let task = tokio::spawn(async move {
            let resource = Resource {
                endpoint: endpoint.clone(),
                primary_path: ResourcePath::from_str("/var/log"),
                archive_context: None,
            };

            let request = SearchRequest {
                query: r"ERROR".to_string(),
                resource,
                config: SearchConfig::default(),
                context_lines: 2,
                path_includes: vec![],
                path_excludes: vec![],
                encoding: None,
            };

            let mut stream = service.search(request, token).await;
            let mut count = 0;

            while let Some(event) = stream.next().await {
                if let SearchEvent::Found(_) = event {
                    count += 1;
                }
            }

            (endpoint.identity, count)
        });

        tasks.push(task);
    }

    // 收集结果
    while let Some(result) = tasks.next().await {
        match result {
            Ok((agent, count)) => {
                println!("Agent {} 找到 {} 个匹配", agent, count);
            }
            Err(e) => {
                eprintln!("Agent 搜索失败: {}", e);
            }
        }
    }

    Ok(())
}
```

---

## 7. 性能优化

### 7.1 优化策略

| 优化点 | 策略 | 效果 |
|--------|------|------|
| **连接池** | 复用 HTTP 连接 | 减少连接开销 |
| **文件列表缓存** | 缓存 Agent 返回的文件列表 | 减少重复请求 |
| **批量请求** | 合并多个搜索请求 | 减少 RPC 次数 |
| **结果分页** | 分批返回搜索结果 | 降低内存占用 |
| **智能路由** | 根据负载选择 Agent | 负载均衡 |

### 7.2 连接池配置

```rust
use reqwest::Client;

fn create_agent_client() -> Client {
    Client::builder()
        .pool_max_idle_per_host(10)      // 每个Agent保持10个空闲连接
        .pool_idle_timeout(Duration::from_secs(90))
        .connect_timeout(Duration::from_secs(5))
        .http2_keep_alive_interval(Duration::from_secs(30))
        .http2_keep_alive_timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to create Agent client")
}
```

### 7.3 缓存策略

```rust
use lru::LruCache;
use tokio::sync::Mutex;

pub struct AgentProxyFS {
    // ...
    /// 文件列表缓存
    file_list_cache: Arc<Mutex<LruCache<ResourcePath, CacheEntry>>>,
}

struct CacheEntry {
    files: Vec<FileMetadata>,
    cached_at: Instant,
    ttl: Duration,
}

impl AgentProxyFS {
    async fn get_file_list(&self, path: &ResourcePath) -> Result<Vec<FileMetadata>> {
        // 检查缓存
        {
            let cache = self.file_list_cache.lock().await;
            if let Some(entry) = cache.get(path) {
                if entry.cached_at.elapsed() < entry.ttl {
                    return Ok(entry.files.clone());
                }
            }
        }

        // 从 Agent 获取
        let files = self.list_files_from_agent(path).await?;

        // 更新缓存
        {
            let mut cache = self.file_list_cache.lock().await;
            cache.put(path.clone(), CacheEntry {
                files: files.clone(),
                cached_at: Instant::now(),
                ttl: Duration::from_secs(60), // 60秒缓存
            });
        }

        Ok(files)
    }
}
```

### 7.4 批量搜索优化

```rust
/// 批量搜索请求
#[derive(Debug, Serialize)]
pub struct BatchSearchRequest {
    /// 多个搜索查询
    pub queries: Vec<SearchQuery>,
}

#[derive(Debug, Serialize)]
pub struct SearchQuery {
    pub path: ResourcePath,
    pub query: String,
    pub context_lines: usize,
}

impl AgentProxyFS {
    /// 批量搜索
    pub async fn batch_search(
        &self,
        queries: Vec<SearchQuery>,
    ) -> Result<Vec<SearchResult>, AgentError> {
        let url = format!("{}/api/v1/search/batch", self.base_url);

        let response = self.client
            .post(&url)
            .json(&BatchSearchRequest { queries })
            .send()
            .await
            .map_err(|e| AgentError::ConnectionError(e.to_string()))?;

        let results: Vec<SearchResult> = response
            .json()
            .await
            .map_err(|e| AgentError::InvalidResponse(e.to_string()))?;

        Ok(results)
    }
}
```

---

## 附录

### A. 错误处理

```rust
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("连接错误: {0}")]
    ConnectionError(String),

    #[error("搜索失败: {0}")]
    SearchFailed(String),

    #[error("列表失败: {0}")]
    ListFailed(String),

    #[error("无效响应: {0}")]
    InvalidResponse(String),

    #[error("搜索超时")]
    Timeout,

    #[error("搜索已取消")]
    Cancelled,

    #[error("搜索未找到: {0}")]
    SearchNotFound(String),
}
```

### B. 监控指标

```rust
pub struct AgentSearchMetrics {
    /// 搜索请求数
    pub search_requests: AtomicU64,

    /// 活跃搜索数
    pub active_searches: AtomicU64,

    /// 平均搜索延迟
    pub avg_latency: AtomicU64,

    /// 搜索失败率
    pub failure_rate: AtomicU64,
}
```

### C. 配置建议

```rust
/// Agent 搜索配置
#[derive(Debug, Clone)]
pub struct AgentSearchConfig {
    /// 最大并发搜索数
    pub max_concurrent_searches: usize,

    /// 单个搜索超时
    pub search_timeout: Duration,

    /// 结果缓存 TTL
    pub cache_ttl: Duration,

    /// 连接池大小
    pub connection_pool_size: usize,
}

impl Default for AgentSearchConfig {
    fn default() -> Self {
        Self {
            max_concurrent_searches: 10,
            search_timeout: Duration::from_secs(300),
            cache_ttl: Duration::from_secs(60),
            connection_pool_size: 10,
        }
    }
}
```

---

**文档版本**: 1.0
**最后更新**: 2026-02-02
