# DFS 搜索架构统一入口说明

**版本**: 1.0
**日期**: 2026-02-02
**状态**: 设计草案

---

## 核心要点

**logseek-search** 和 **agent-search** 的关系：

```
logseek-search = 通用搜索架构 (框架)
agent-search    = Agent 代理搜索 (具体实现)

┌─────────────────────────────────────────────────────────┐
│              logseek-search (通用搜索架构)              │
│  ┌─────────────────────────────────────────────────────┐ │
│  │  SearchService (统一入口)                          │ │
│  │      │                                             │ │
│  │      ├─── ResourceResolver                          │ │
│  │      │                                             │ │
│  │      └─── Searchable trait                          │ │
│  │              │                                     │ │
│  │              ├─── LocalFileSystem 实现             │ │
│  │              ├─── S3Storage 实现                    │ │
│  │              └─── AgentProxyFS 实现 ← agent-search │ │
│  └─────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

**关键设计**：`SearchService` 是统一入口，通过 `Searchable` trait 多态分发到不同的实现。

---

## 文档关系图

```
┌─────────────────────────────────────────────────────────────────┐
│                      DFS 搜索文档体系                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  dfs-logseek-search.md (1254 行)                               │
│  ├─ 通用搜索架构                                               │
│  ├─ Searchable trait 设计                                     │
│  ├─ SearchService 统一入口                                    │
│  └─ 各 FileSystem 搜索实现                                    │
│         │                                                    │
│         │ 包含                                               │
│         ▼                                                    │
│  dfs-agent-search.md (1114 行)                                │
│  ├─ AgentProxyFS 具体实现                                    │
│  ├─ Agent 端搜索服务                                         │
│  ├─ HTTP/SSE 协议设计                                       │
│  └─ 性能优化策略                                             │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 统一入口设计

### 核心代码

```rust
/// 统一搜索服务
pub struct SearchService {
    resolver: Arc<ResourceResolver>,
    encoding_detector: Arc<EncodingDetector>,
    query_parser: Arc<QueryParser>,
}

impl SearchService {
    /// 统一搜索入口 - 支持所有存储后端
    pub async fn search(
        &self,
        request: SearchRequest,
        cancel_token: CancellationToken,
    ) -> impl Stream<Item = SearchEvent> {
        async_stream::stream! {
            // 1. 通过 ResourceResolver 获取 FileSystem
            let fs = match self.resolver.resolve(&request.resource).await {
                Ok(fs) => fs,
                Err(e) => {
                    yield SearchEvent::Error(SearchError::NotFound(...));
                    return;
                }
            };

            // 2. 检查是否支持搜索（多态分发）
            let searchable = match fs.as_any().downcast_ref::<dyn Searchable>() {
                Some(s) => s,
                None => {
                    yield SearchEvent::Error(SearchError::StreamError(
                        "文件系统不支持搜索".to_string()
                    ));
                    return;
                }
            };

            // 3. 创建搜索流（具体实现由各 FileSystem 提供）
            let mut entry_stream = searchable.create_entry_stream(
                search_path,
                true,
                &request.config,
            ).await;

            // 4. 处理搜索流（统一逻辑）
            while let Some(entry) = entry_stream.next_entry().await {
                // ... 统一的搜索处理逻辑
            }
        }
    }
}
```

---

## 多态分发流程

```
SearchService::search()
        │
        ▼
ResourceResolver::resolve()
        │
        ├─── resource.endpoint == Local
        │         │
        │         └──► LocalFileSystem
        │                │
        │                └──► impl Searchable
        │                        │
        │                        └──► create_entry_stream() → walkdir
        │
        ├─── resource.endpoint == S3
        │         │
        │         └──► S3Storage
        │                │
        │                └──► impl Searchable
        │                        │
        │                        └──► create_entry_stream() → S3 ListObjects
        │
        └─── resource.endpoint == Agent (Remote + Proxy)
                  │
                  └──► AgentProxyFS
                          │
                          └──► impl Searchable
                                  │
                                  └──► create_entry_stream()
                                          │
                                          └──► HTTP POST to Agent
```

---

## 各存储后端的 Searchable 实现

### 1. LocalFileSystem (logseek-search)

```rust
#[async_trait]
impl Searchable for LocalFileSystem {
    async fn create_entry_stream(
        &self,
        path: &ResourcePath,
        recursive: bool,
        config: &SearchConfig,
    ) -> Result<Box<dyn SearchEntryStream>, SearchError> {
        // 使用 walkdir 遍历本地文件
        let walker = WalkDir::new(root_path)
            .max_depth(max_depth)
            .into_iter();

        let stream = async_stream::stream! {
            for entry in walker {
                yield Ok(SearchResultEntry { ... });
            }
        };

        Ok(Box::new(StreamAdapter::new(Box::pin(stream))))
    }
}
```

### 2. S3Storage (logseek-search)

```rust
#[async_trait]
impl Searchable for S3Storage {
    async fn create_entry_stream(
        &self,
        path: &ResourcePath,
        recursive: bool,
        config: &SearchConfig,
    ) -> Result<Box<dyn SearchEntryStream>, SearchError> {
        // 分页列出 S3 对象
        let stream = async_stream::stream! {
            let mut continuation_token = None;
            loop {
                let response = self.client.list_objects_v2()
                    .bucket(&self.bucket)
                    .prefix(&prefix)
                    .continuation_token(continuation_token)
                    .send().await;

                // 处理响应...
            }
        };

        Ok(Box::new(StreamAdapter::new(Box::pin(stream))))
    }
}
```

### 3. AgentProxyFS (agent-search)

```rust
#[async_trait]
impl Searchable for AgentProxyFS {
    async fn create_entry_stream(
        &self,
        path: &ResourcePath,
        recursive: bool,
        config: &SearchConfig,
    ) -> Result<Box<dyn SearchEntryStream>, SearchError> {
        // 通过 HTTP 代理到 Agent
        let url = format!("{}/api/v1/search/start", self.base_url);

        let response = self.client
            .post(&url)
            .json(&AgentSearchRequest { ... })
            .send()
            .await?;

        // 将 Agent SSE 流转换为 SearchEntryStream
        let stream = response.bytes_stream().map(|chunk| {
            // 解析 SSE 事件
            parse_sse_event(&chunk)
        });

        Ok(Box::new(StreamAdapter::new(Box::pin(stream))))
    }
}
```

### 4. ArchiveFileSystem (logseek-search)

```rust
#[async_trait]
impl<F: FileSystem + Searchable> Searchable for ArchiveFileSystem<F> {
    async fn create_entry_stream(
        &self,
        path: &ResourcePath,
        recursive: bool,
        config: &SearchConfig,
    ) -> Result<Box<dyn SearchEntryStream>, SearchError> {
        // 先获取归档
        let archive = self.get_archive().await?;

        // 列出归档内条目
        let entries = archive.list_entries(path)?;

        let stream = async_stream::stream! {
            for entry in entries {
                yield Ok(SearchResultEntry { ... });
            }
        };

        Ok(Box::new(StreamAdapter::new(Box::pin(stream))))
    }
}
```

---

## 使用方式对比

### 本地搜索 (直接使用 Searchable)

```rust
// 创建资源
let resource = Resource {
    endpoint: Endpoint::local_fs(),
    primary_path: ResourcePath::from_str("/var/log"),
    archive_context: None,
};

// 统一搜索入口
let search_service = SearchService::new(resolver);
let mut stream = search_service.search(request, cancel_token).await;

// 内部流程：
// SearchService → ResourceResolver → LocalFileSystem
//             → impl Searchable → create_entry_stream()
//             → walkdir → 文件流
```

### Agent 搜索 (通过 AgentProxyFS)

```rust
// 创建资源 (Agent 端点)
let resource = Resource {
    endpoint: Endpoint::agent("192.168.1.100".to_string(), 4001, "web-01".to_string()),
    primary_path: ResourcePath::from_str("/var/log"),
    archive_context: None,
};

// 统一搜索入口（完全相同的代码！）
let search_service = SearchService::new(resolver);
let mut stream = search_service.search(request, cancel_token).await;

// 内部流程：
// SearchService → ResourceResolver → AgentProxyFS
//             → impl Searchable → create_entry_stream()
//             → HTTP POST → Agent → LocalFileSystem
```

### 归档搜索 (通过 ArchiveFileSystem)

```rust
// 创建资源 (包含归档上下文)
let resource = Resource {
    endpoint: Endpoint::local_fs(),
    primary_path: ResourcePath::from_str("/data/archive.tar"),
    archive_context: Some(ArchiveContext {
        inner_path: ResourcePath::from_str("app.log"),
        archive_type: Some(ArchiveType::Tar),
    }),
};

// 统一搜索入口（完全相同的代码！）
let search_service = SearchService::new(resolver);
let mut stream = search_service.search(request, cancel_token).await;

// 内部流程：
// SearchService → ResourceResolver → ArchiveFileSystem<LocalFS>
//             → impl Searchable → create_entry_stream()
//             → 解析归档 → entries
```

---

## 架构层次

```
┌─────────────────────────────────────────────────────────────────┐
│                      应用层 (LogSeek)                          │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  routes/search.rs                                         │ │
│  │      │                                                     │ │
│  │      └──► POST /api/v1/logseek/search                   │ │
│  │                                                         │ │
│  │  SearchService (统一入口)                                │ │
│  └─────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│                      DFS 领域层 (opsbox-core)                   │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  trait Searchable (统一接口)                              │ │
│  │      ├─── create_entry_stream()                            │ │
│  │      └─── supports_streaming_search()                     │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                          │                                      │
│         ┌────────────────┼────────────────┐                    │
│         ▼                ▼                ▼                    │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐        │
│  │LocalFileSystem│  │  S3Storage   │  │AgentProxyFS  │        │
│  │              │  │              │  │              │        │
│  │ impl Searchable│  │impl Searchable│  │impl Searchable│        │
│  └──────────────┘  └──────────────┘  └──────────────┘        │
│         │                │                │                    │
│         └────────────────┼────────────────┘                    │
│                          ▼                                     │
│                  ┌──────────────┐                              │
│                  │ArchiveFileSystem│ (装饰器)                  │
│                  │ impl Searchable│                              │
│                  └──────────────┘                              │
└─────────────────────────────────────────────────────────────────┘
```

---

## 文档内容对比

| 方面 | dfs-logseek-search.md | dfs-agent-search.md |
|------|------------------------|---------------------|
| **定位** | 通用搜索架构设计 | Agent 特定实现 |
| **SearchService** | ✅ 统一入口设计 | ⚠️ 作为使用方 |
| **Searchable trait** | ✅ 完整定义 | ✅ 实现该 trait |
| **LocalFileSystem** | ✅ 实现细节 | ❌ 不涉及 |
| **S3Storage** | ✅ 实现细节 | ❌ 不涉及 |
| **AgentProxyFS** | ⚠️ 简要说明 | ✅ 详细实现 |
| **HTTP 协议** | ❌ 不涉及 | ✅ 完整设计 |
| **SSE 流式** | ❌ 不涉及 | ✅ 完整设计 |
| **Agent 端** | ❌ 不涉及 | ✅ 完整实现 |

---

## 关键设计原则

### 1. 单一入口原则

```rust
// 所有搜索都通过 SearchService::search() 进入
let mut stream = search_service.search(request, cancel_token).await;

// 内部根据 Resource.endpoint 自动分发到：
// - LocalFileSystem::create_entry_stream()
// - S3Storage::create_entry_stream()
// - AgentProxyFS::create_entry_stream()
// - ArchiveFileSystem::create_entry_stream()
```

### 2. 开闭原则

添加新的搜索后端（如 NFS、FTP）时：
- ✅ 只需实现 `Searchable` trait
- ✅ 注册到 `ResourceResolver`
- ❌ 不需要修改 `SearchService`
- ❌ 不需要修改调用方代码

### 3. 依赖倒置原则

```
SearchService 依赖  →  Searchable trait (抽象)
                     ↑
                     ↑
    ┌────────────────┼────────────────┐
    │                │                │
LocalFileSystem  S3Storage    AgentProxyFS
```

---

## 完整调用链示例

### 场景：搜索 Agent 上的归档文件

```
用户请求
  │
  │ POST /api/v1/logseek/search
  │ Body: { "orl": "orl://web-01@agent.192.168.1.100:4001/data.tar?entry=app.log", "query": "ERROR" }
  ▼
LogSeek routes/search.rs
  │
  ├─── ORL::parse() → Resource
  │     {
  │       endpoint: AgentProxyFS,
  │       primary_path: /data.tar,
  │       archive_context: Some(ArchiveContext { inner_path: "app.log" })
  │     }
  │
  ▼
SearchService::search()
  │
  ├─── ResourceResolver::resolve(resource)
  │     │
  │     └──► AgentProxyFS (因为 endpoint 是 Agent 类型)
  │            │
  │            └──► AgentProxyFS::as_any().downcast_ref::<dyn Searchable>()
  │
  ├─── searchable.create_entry_stream()
  │     │
  │     └──► AgentProxyFS::create_entry_stream()
  │            │
  │            ├─── 检测到 archive_context
  │            │
  │            └──► HTTP POST to Agent:192.168.1.100:4001/api/v1/search/start
  │                   │
  │                   ▼
  ┌──────────────────────────────────────────────────────┐
  │  Agent 端                                         │
  │  ┌────────────────────────────────────────────────┐ │
  │  │  AgentSearchService::start_search()            │ │
  │  │      │                                         │ │
  │  │      ▼                                         │ │
  │  │  LocalSearchService::search()                  │ │
  │  │      │                                         │ │
  │  │      ├─── ArchiveFileSystem::create_entry_stream()│ │
  │  │      │     │                                  │ │
  │  │      │     └──► 解析 data.tar                  │ │
  │  │      │     │                                  │ │
  │  │      │     └──► 搜索 app.log                   │ │
  │  │      │                                        │ │
  │  │      └──► SSE 流返回结果                       │ │
  │  └────────────────────────────────────────────────┘ │
  └──────────────────────────────────────────────────────┘
  │
  ▼
Server 接收 SSE 流
  │
  └──► 解析事件
        │
        ├─── Progress → SearchEvent::Progress
        ├─── Found → SearchEvent::Found
        ├─── Complete → SearchEvent::Complete
        └─── Error → SearchEvent::Error
```

---

## 文档阅读顺序建议

### 新手入门

1. **先读**: `dfs-logseek-search.md` - 理解通用搜索架构
2. **再读**: `dfs-agent-search.md` - 了解 Agent 特定实现

### 架构师/高级开发者

1. **先读**: `dfs-complete-design.md` - 理解整体 DFS 设计
2. **再读**: `dfs-logseek-search.md` - 理解搜索架构
3. **最后**: `dfs-agent-search.md` - 了解 Agent 代理细节

### 实现者

| 角色 | 需要读的文档 |
|------|-------------|
| **LocalFileSystem 实现** | `dfs-logseek-search.md` §4.2 |
| **S3Storage 实现** | `dfs-logseek-search.md` §4.2 |
| **AgentProxyFS 实现** | `dfs-agent-search.md` §4 |
| **Agent 端实现** | `dfs-agent-search.md` §3 |
| **ArchiveFileSystem 实现** | `dfs-logseek-search.md` §4.2 |

---

## 总结

### 关键关系

```
logseek-search (通用)           agent-search (特定)
        │                              │
        │ 实现                         │ 继承
        ├──────────────────────────────┤
        │                              │
    Searchable ←────────────────── AgentProxyFS
        │                              │
        │ 统一入口                      │
    SearchService ◄──────────────────┤
        │                              │
        │                              ▼
    统一的 search() 方法             HTTP 代理到 Agent
```

### 统一入口

```rust
// 无论是本地、S3、Agent 还是归档搜索
// 都使用完全相同的代码：

let search_service = SearchService::new(resolver);
let mut stream = search_service.search(request, cancel_token).await;

// 内部自动分发到：
// - LocalFileSystem::create_entry_stream()   [logseek-search]
// - S3Storage::create_entry_stream()        [logseek-search]
// - AgentProxyFS::create_entry_stream()    [agent-search]
// - ArchiveFileSystem::create_entry_stream() [logseek-search]
```

---

**文档版本**: 1.0
**最后更新**: 2026-02-02
