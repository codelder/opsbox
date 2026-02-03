# LogSeek 搜索完整调用链：从 Router 到 DFS

**版本**: 1.0
**日期**: 2026-02-02
**状态**: 设计草案

---

## 目录

1. [当前实现完整调用链](#1-当前实现完整调用链)
2. [新设计下的调用链](#2-新设计下的调用链)
3. [迁移对比](#3-迁移对比)
4. [实现示例](#4-实现示例)

---

## 1. 当前实现完整调用链

### 1.1 架构总览

```
┌─────────────────────────────────────────────────────────────────┐
│                      LogSeek 搜索调用链（当前实现）                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  Router: POST /api/v1/logseek/search.ndjson              │ │
│  │      │                                                     │ │
│  │      ├─── 1. 解析 SearchBody { q, context }              │ │
│  │      │                                                     │ │
│  │      └─── 2. 创建 SearchExecutor                        │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                          │                                      │
│                          ▼                                      │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  SearchExecutor::search(query, sid, context_lines)        │ │
│  │      │                                                     │ │
│  │      ├─── 3. 解析 Query → 获取 sources (ORL 列表)          │ │
│  │      │      └─── SourcePlanner (Starlark)                │ │
│  │      │                                                     │ │
│  │      └─── 4. 对每个 ORL 并行执行搜索                         │ │
│  │              │                                             │ │
│  │              ▼                                             │ │
│  │      ┌────────────────────────────────────────────────────┐ │ │
│  │      │  create_search_provider(orl)                      │ │ │
│  │      │      │                                           │ │ │
│  │      │      ├─── EndpointType::Local → LocalOpsFS         │ │ │
│  │      │      ├─── EndpointType::S3 → S3OpsFS               │ │ │
│  │      │      └─── EndpointType::Agent → AgentSearchProvider│ │ │
│  │      └────────────────────────────────────────────────────┘ │ │
│  │              │                                             │ │
│  │              ▼                                             │ │
│  │      ┌────────────────────────────────────────────────────┐ │ │
│  │      │  provider.search(ctx, request)                      │ │ │
│  │      │      │                                           │ │ │
│  │      │      └─── search_with_entry_stream()             │ │ │
│  │      │              │                                   │ │ │
│  │      │              ▼                                   │ │ │
│  │      │      ┌─────────────────────────────────────────┐ │ │ │
│  │      │      │ EntryStream (stream of entries)          │ │ │ │
│  │      │      │  │                                        │ │ │ │
│  │      │      │  └──► For each entry:                       │ │ │ │
│      │      │         - open_read()                          │ │ │ │
│      │      │         - decode()                             │ │ │ │
│      │      │         - regex_match()                        │ │ │ │
│      │      │         - send SearchEvent                   │ │ │ │
│  │      │      └─────────────────────────────────────────┘ │ │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                          │                                      │
│                          ▼                                      │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  convert_to_ndjson_stream(SearchEvent)                    │ │
│  │      │                                                     │ │
│  │      └─── 5. 将 SearchEvent 转换为 NDJSON               │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                          │                                      │
│                          ▼                                      │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  HTTP Response: application/x-ndjson                       │ │
│  │      - X-Logseek-SID header                                │ │
│  │      - Stream of JSON lines                               │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 1.2 详细代码流程

#### 步骤 1: Router 层 - 接收 HTTP 请求

**文件**: `logseek/src/routes/search.rs`

```rust
/// POST /api/v1/logseek/search.ndjson
pub async fn stream_search(
    State(pool): State<SqlitePool>,
    Json(body): Json<SearchBody>,
) -> Result<HttpResponse<Body>, LogSeekApiError> {
    // 1. 解析请求参数
    let q = body.q;                    // 查询字符串
    let ctx = body.context.unwrap_or(3);  // 上下文行数

    // 2. 创建 SearchExecutor
    let config = SearchExecutorConfig {
        io_max_concurrency: s3_max_concurrency(),
        stream_channel_capacity: stream_channel_capacity(),
    };
    let executor = SearchExecutor::new(pool, config);
    let cancel_token = CancellationToken::new();
    let sid = new_sid();  // 生成会话 ID

    // 3. 解析查询获取高亮关键字
    let highlights = Query::parse_github_like(&q)
        .map(|spec| spec.highlights.clone())
        .unwrap_or_default();

    // 4. 执行搜索
    let result_rx = executor.search(&q, sid.clone(), ctx, Some(cancel_token)).await?;

    // 5. 缓存关键字
    simple_cache().put_keywords(&sid, highlights).await;

    // 6. 转换为 NDJSON 流
    let stream = convert_to_ndjson_stream(result_rx, highlights);

    // 7. 构建 HTTP 响应
    build_ndjson_response(stream, sid)
}
```

**请求体格式**:
```json
{
  "q": "ERROR app=myapp -path:/var/log -exclude:*.gz",
  "context": 2
}
```

#### 步骤 2: Query 解析与源规划

**文件**: `logseek/src/service/search_executor.rs`

```rust
impl SearchExecutor {
    /// 规划搜索：解析查询并确定搜索源
    async fn plan(
        &self,
        query: &str,
        context_lines: usize,
    ) -> Result<(Vec<ORL>, SearchRequest), ServiceError> {
        // 1. 解析查询语法
        let (app, path_includes, path_excludes) = parse_query_syntax(query)?;

        // 2. 提取 app 限定符（如果有）
        let app = app.and_then(|a| {
            // 查找指定应用的 planner
            get_planner(&self.pool, &a).await.ok()
        });

        // 3. 执行 Starlark 脚本获取源列表
        let plan = source_planner::plan_with_starlark(
            &self.pool,
            app.as_deref(),
            &cleaned_query,
        ).await?;

        // 4. 构造搜索请求
        let request = SearchRequest {
            query: plan.cleaned_query,
            encoding: None,
            path_includes,
            path_excludes,
            context_lines,
        };

        Ok((plan.sources, request))
    }
}
```

**Starlark 脚本示例**:
```python
# app: myapp
SOURCES = [
    "orl://local/var/log/myapp",
    "orl://prod@s3/logs/myapp",
    "orl://web-01@agent.192.168.1.100:4001/var/log/myapp",
]
```

#### 步骤 3: 并行搜索执行

```rust
impl SearchExecutor {
    pub async fn search(
        &self,
        query: &str,
        sid: String,
        context_lines: usize,
        cancel_token: Option<CancellationToken>,
    ) -> Result<mpsc::Receiver<SearchEvent>, ServiceError> {
        // 1. 规划搜索，获取源列表
        let (sources, request) = self.plan(query, context_lines).await?;

        // 2. 创建结果通道
        let (tx, rx) = mpsc::channel(self.config.stream_channel_capacity);

        // 3. 对每个源并行执行搜索
        for source in sources {
            let orl = source;  // ORL: orl://local/var/log, orl://prod@s3/bucket/...

            // 启动搜索任务
            tokio::spawn(async move {
                // 创建 Provider
                let provider = create_search_provider(&pool, &orl).await?;

                // 创建搜索上下文
                let ctx = SearchContext {
                    orl: orl.clone(),
                    sid: sid.clone(),
                    tx: tx_internal,
                    cancel_token: cancel_token.clone(),
                };

                // 执行搜索
                provider.search(&ctx, &request, &pool).await?;
            });
        }

        // 4. 返回结果接收器
        Ok(rx)
    }
}
```

#### 步骤 4: Provider 搜索执行

```rust
// LocalFS 实现
impl SearchableFileSystem for LocalOpsFS {
    async fn search(&self, ctx: &SearchContext, req: &SearchRequest) -> Result<()> {
        // 创建 EntryStream
        let stream = self.as_entry_stream(&ctx.orl.to_ops_path()?, true).await?;

        // 处理每个条目
        while let Some(entry) = stream.next_entry().await {
            let (meta, reader) = entry?;

            // 读取、解码、搜索
            let content = decode_content(reader, &req.encoding).await?;
            let matches = regex_search(&content, &req.query, req.context_lines)?;

            // 发送结果
            for match in matches {
                ctx.tx.send(SearchEvent::Success { ... }).await?;
            }
        }

        Ok(())
    }
}
```

#### 步骤 5: 结果转换为 NDJSON

```rust
fn convert_to_ndjson_stream(
    mut rx: mpsc::Receiver<SearchEvent>,
    highlights: Vec<KeywordHighlight>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> {
    async_stream::stream! {
        while let Some(event) = rx.recv().await {
            let json_bytes = match event {
                SearchEvent::Success(res) => {
                    let json_obj = render_json_chunks(
                        &res.path,
                        res.merged,
                        &res.lines,
                        &highlights,
                        &res.encoding,
                    );
                    serde_json::to_vec(&SearchResponse::Result { data: json_obj })
                }
                SearchEvent::Error { source, message, .. } => {
                    serde_json::to_vec(&SearchResponse::Error { source, message, recoverable })
                }
                SearchEvent::Complete { source, elapsed_ms } => {
                    serde_json::to_vec(&SearchResponse::Complete { source, elapsed_ms })
                }
            };

            if let Some(bytes) = json_bytes {
                bytes.push(b'\n');
                yield Ok(Bytes::from(bytes));
            }
        }
    }
}
```

**NDJSON 响应格式**:
```
HTTP/1.1 200 OK
Content-Type: application/x-ndjson; charset=utf-8
X-Logseek-SID: session-uuid

{"type":"result","data":{"path":"orl://...","lines":[...],"encoding":"UTF-8"}}
{"type":"result","data":{"path":"orl://...","lines":[...],"encoding":"UTF-8"}}
{"type":"complete","source":"orl://local/var/log","elapsed_ms":1234}
```

---

## 2. 新设计下的调用链

### 2.1 架构变化

```
┌─────────────────────────────────────────────────────────────────┐
│                    新设计下的搜索调用链                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  Router: POST /api/v1/logseek/search.ndjson              │ │
│  │      │                                                     │ │
│  │      ├─── 1. 解析 SearchBody → Resource                     │ │  ← 变化
│  │      │                                                     │ │
│  │      └─── 2. 调用 SearchService (统一入口)                  │ │  ← 变化
│  └─────────────────────────────────────────────────────────────┘ │
│                          │                                      │
│                          ▼                                      │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  SearchService::search(SearchRequest, cancel_token)       │ │  ← 新增
│  │      │                                                     │ │
│  │      ├─── 3. ResourceResolver.resolve(resource)              │ │  ← 新增
│  │      │      │                                           │ │
│  │      │      └──► 根据 resource.endpoint 选择:           │ │
│  │      │              - Local → LocalFileSystem           │ │
│  │      │              - S3 → S3Storage                  │ │
│      │      │              - Agent → AgentProxyFS            │ │
│  │      │                                                     │ │ │
│  │      └─── 4. as_any().downcast_ref::<dyn Searchable>()       │ │  ← 新增
│  │      │              │                                   │ │
│  │      │              └──► create_entry_stream()           │ │
│  │      │                  │                             │ │ │
│  │      │                  ├─── LocalFS: walkdir          │ │ │
│  │      │                  ├─── S3: ListObjectsV2        │ │ │
│      │      │                  └─── Agent: HTTP to Agent    │ │ │ ← agent-search
│  │      │                                                     │ │
│  │      └─── 5. 统一的搜索管道处理                             │ │  ← 新增
│  │             │                                             │ │
│  │             ├─── 并发控制 (Semaphore)                    │ │
│  │             ├─── 编码检测 (EncodingDetector)               │ │
│  │             ├─── 查询匹配 (QueryParser)                    │ │
│  │             └─── 结果缓存 (simple_cache)                  │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                          │                                      │
│                          ▼                                      │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  SearchEvent 流转换为 NDJSON (保持不变)                    │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                          │                                      │
│                          ▼                                      │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  HTTP Response: application/x-ndjson                       │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 详细代码流程

#### 步骤 1: Router 层 - 接收 HTTP 请求

**文件**: `logseek/src/routes/search.rs` (新设计)

```rust
use opsbox_core::dfs::{ResourceResolver, Resource, SearchService, SearchRequest, SearchConfig};
use opsbox_core::odfs::ORL;

/// POST /api/v1/logseek/search.ndjson
pub async fn stream_search(
    State(resolver): State<Arc<ResourceResolver>>,
    Json(body): Json<SearchBody>,
) -> Result<HttpResponse<Body>, LogSeekApiError> {
    tracing::info!("[Search] 开始搜索: q={}", body.q);

    let ctx = body.context.unwrap_or(3);
    let sid = new_sid();

    // 1. 解析查询获取源列表 (新设计：这里需要重构)
    let (resources, query_config) = parse_query_to_resources(
        &body.q,
        ctx,
    ).await?;

    // 2. 为每个 Resource 创建 SearchRequest
    let search_requests: Vec<SearchRequest> = resources
        .into_iter()
        .map(|resource| SearchRequest {
            query: query_config.query.clone(),
            resource,
            config: SearchConfig::default(),
            context_lines: ctx,
            path_includes: query_config.path_includes.clone(),
            path_excludes: query_config.path_excludes.clone(),
            encoding: query_config.encoding.clone(),
        })
        .collect();

    // 3. 创建 SearchService
    let search_service = SearchService::new(resolver);

    // 4. 执行搜索（支持多个源）
    let cancel_token = CancellationToken::new();
    let result_rx = search_service.search_multiple(
        search_requests,
        sid.clone(),
        cancel_token,
    ).await?;

    // 5. 解析查询获取高亮关键字
    let highlights = Query::parse_github_like(&body.q)
        .map(|spec| spec.highlights.clone())
        .unwrap_or_default();

    // 6. 缓存关键字
    simple_cache().put_keywords(&sid, highlights).await;

    // 7. 转换为 NDJSON 流
    let stream = convert_to_ndjson_stream(result_rx, highlights);

    // 8. 构建 HTTP 响应
    build_ndjson_response(stream, sid)
}
```

**请求体格式** (增强版):
```json
{
  "q": "ERROR app=myapp -path:/var/log -exclude:*.gz",
  "context": 2,
  "sources": [  // 可选：显式指定搜索源
    "orl://local/var/log/myapp",
    "orl://prod@s3/logs/myapp"
  ]
}
```

#### 步骤 2: Query 解析与 Resource 构造

**文件**: `logseek/src/service/query_parser.rs` (新增)

```rust
use opsbox_core::dfs::{Resource, Endpoint, ResourcePath};
use opsbox_core::odfs::ORL;

/// 解析查询字符串，提取资源和搜索配置
pub async fn parse_query_to_resources(
    query: &str,
    context_lines: usize,
) -> Result<(Vec<Resource>, QueryConfig), ServiceError> {
    // 1. 解析查询语法
    let (app_name, path_includes, path_excludes) = parse_query_syntax(query)?;

    // 2. 获取源列表（新设计：从数据库配置或 Starlark）
    let sources = if let Some(app) = app_name {
        // 使用 Starlark planner 获取源
        let orl_list = source_planner::plan_with_starlark(&pool, &app, query).await?;

        // 将 ORL 转换为 Resource
        orl_list.into_iter().map(|orl| orl_to_resource(orl)).collect()
    } else {
        // 从查询中提取 ORL
        let orl_strings = extract_orls_from_query(query)?;
        orl_strings.into_iter().map(|s| ORL::parse(&s)).map_ok(|orl| orl_to_resource(orl)).collect()
    }?;

    // 3. 构造查询配置
    let query_config = QueryConfig {
        query: query.to_string(),
        path_includes,
        path_excludes,
        encoding: None,
    };

    Ok((sources, query_config))
}

/// ORL 转 Resource
fn orl_to_resource(orl: &ORL) -> Resource {
    let endpoint = match orl.endpoint_type().unwrap() {
        EndpointType::Local => Endpoint::local_fs(),
        EndpointType::S3 => Endpoint::s3(orl.effective_id().to_string()),
        EndpointType::Agent => {
            // 解析 Agent 信息
            let (name, host, port) = parse_agent_orl(orl);
            Endpoint::agent(host, port, name)
        }
    };

    let primary_path = ResourcePath::from_str(orl.path());

    let archive_context = if orl.target_type() == TargetType::Archive {
        Some(ArchiveContext {
            inner_path: ResourcePath::from_str(orl.entry_path().unwrap_or("")),
            archive_type: ArchiveType::from_extension(orl.path()),
        })
    } else {
        None
    };

    Resource {
        endpoint,
        primary_path,
        archive_context,
    }
}
```

#### 步骤 3: SearchService 统一搜索

**文件**: `opsbox-core/src/dfs/services/search.rs` (新增)

```rust
impl SearchService {
    /// 单个资源搜索
    pub async fn search(
        &self,
        request: SearchRequest,
        cancel_token: CancellationToken,
    ) -> impl Stream<Item = SearchEvent> {
        async_stream::stream! {
            // 1. 解析 Resource 到 FileSystem
            let fs = match self.resolver.resolve(&request.resource).await {
                Ok(fs) => fs,
                Err(e) => {
                    yield SearchEvent::Error(SearchError::NotFound(...));
                    return;
                }
            };

            // 2. 检查是否支持搜索
            let searchable = match fs.as_any().downcast_ref::<dyn Searchable>() {
                Some(s) => s,
                None => {
                    yield SearchEvent::Error(SearchError::StreamError(
                        "文件系统不支持搜索".to_string()
                    ));
                    return;
                }
            };

            // 3. 创建搜索流
            let search_path = get_search_path(&request.resource);
            let mut entry_stream = searchable.create_entry_stream(
                search_path,
                true,
                &request.config,
            ).await;

            // 4. 并发处理条目
            let semaphore = Arc::new(Semaphore::new(request.config.max_concurrency));
            let mut tasks = FuturesUnordered::new();

            loop {
                if cancel_token.is_cancelled() {
                    break;
                }

                let entry = match entry_stream.next_entry().await {
                    Ok(Some(e)) => e,
                    Ok(None) => break,
                    Err(e) => {
                        yield SearchEvent::Error(e.into());
                        continue;
                    }
                };

                // 应用路径过滤
                if !matches_path_filters(&entry.path, &request.path_includes, &request.path_excludes) {
                    continue;
                }

                // 启动搜索任务
                let fs_clone = fs.clone();
                let entry_path = entry.path.clone();
                let query = request.query.clone();
                let context_lines = request.context_lines;

                let task = tokio::spawn(async move {
                    // 打开文件、检测编码、搜索
                    let reader = fs_clone.open_read(&entry_path).await?;
                    let content = decode_content(reader, "UTF-8").await?;
                    let matches = query_parser.execute(&content, &query, context_lines)?;
                    Ok(matches)
                });

                tasks.push(task);

                // 限制并发数
                if tasks.len() >= request.config.max_concurrency {
                    match tasks.next().await {
                        Some(Ok(results)) => {
                            for result in results {
                                yield SearchEvent::Found(result);
                            }
                        }
                        Some(Err(e)) => yield SearchEvent::Error(e.into()),
                        None => break,
                    }
                }
            }

            // 收集剩余任务
            while let Some(task) = tasks.next().await {
                match task {
                    Ok(Ok(results)) => {
                        for result in results {
                            yield SearchEvent::Found(result);
                        }
                    }
                    Ok(Err(e)) => yield SearchEvent::Error(e.into()),
                    Err(_) => {}
                }
            }

            yield SearchEvent::Complete(SearchStats::default());
        }
    }

    /// 多资源并行搜索
    pub async fn search_multiple(
        &self,
        requests: Vec<SearchRequest>,
        sid: String,
        cancel_token: CancellationToken,
    ) -> mpsc::Receiver<SearchEvent> {
        let (tx, rx) = mpsc::channel(1000);

        // 为每个资源启动搜索任务
        for request in requests {
            let service = self.clone();
            let tx_clone = tx.clone();

            tokio::spawn(async move {
                let mut stream = service.search(request, cancel_token.clone()).await;

                while let Some(event) = stream.next().await {
                    // 添加资源标识
                    let event = add_resource_identifier(event, &request.resource);
                    let _ = tx_clone.send(event).await;
                }
            });
        }

        rx
    }
}
```

#### 步骤 4: 各 FileSystem 的 Searchable 实现

**LocalFileSystem**: `logseek/src/infra/local_fs.rs`

```rust
#[async_trait]
impl Searchable for LocalFileSystem {
    async fn create_entry_stream(
        &self,
        path: &ResourcePath,
        recursive: bool,
        config: &SearchConfig,
    ) -> Result<Box<dyn SearchEntryStream>, SearchError> {
        // 使用 walkdir 遍历文件
        let walker = WalkDir::new(self.resolve_path(path)?)
            .max_depth(if recursive { usize::MAX } else { 1 })
            .into_iter();

        let stream = async_stream::stream! {
            for entry in walker {
                yield Ok(SearchResultEntry {
                    path: ResourcePath::from_path(entry.path()),
                    metadata: FileMetadata::from_std(entry.metadata().ok()),
                    reader: None,
                });
            }
        };

        Ok(Box::new(StreamAdapter::new(Box::pin(stream))))
    }
}
```

**AgentProxyFS**: `logseek/src/infra/agent_proxy_fs.rs`

```rust
#[async_trait]
impl Searchable for AgentProxyFS {
    async fn create_entry_stream(
        &self,
        path: &ResourcePath,
        recursive: bool,
        config: &SearchConfig,
    ) -> Result<Box<dyn SearchEntryStream>, SearchError> {
        // 1. 通过 HTTP 代理到 Agent
        let url = format!("{}/api/v1/search/start", self.base_url);

        // 2. 发送搜索请求
        let response = self.client.post(&url)
            .json(&AgentSearchRequest {
                path: path.clone(),
                query: ".*".to_string(),  // 传文件列表逻辑
                config: Some(config.clone()),
                recursive,
            })
            .send()
            .await?;

        // 3. 处理 SSE 响应
        let sse_stream = response.bytes_stream().map(|chunk| {
            // 解析 SSE 事件
            parse_sse_event(&chunk)
                .map(|event| event_to_search_entry(event))
        });

        Ok(Box::new(StreamAdapter::new(Box::pin(sse_stream))))
    }
}
```

---

## 3. 迁移对比

### 3.1 变化点总结

| 层级 | 当前实现 | 新设计 | 变化点 |
|------|---------|--------|--------|
| **请求参数** | `SearchBody { q, context }` | 需要支持显式 sources | 解析逻辑增强 |
| **源获取** | SourcePlanner (Starlark) | SourcePlanner + 显式 ORL | 无本质变化，增强灵活性 |
| **搜索入口** | `SearchExecutor::search()` | `SearchService::search()` | 统一入口 |
| **Provider创建** | `create_search_provider(orl)` | `ResourceResolver::resolve()` | 自动分发 |
| **搜索执行** | `provider.search(ctx, req)` | `Searchable::create_entry_stream()` | 统一接口 |
| **结果返回** | `SearchEvent` (mpsc) | `SearchEvent` (Stream) | 流式优化 |

### 3.2 代码对比

#### 当前实现

```rust
// Router 层
pub async fn stream_search(
    State(pool): State<SqlitePool>,
    Json(body): Json<SearchBody>,
) -> Result<HttpResponse<Body>, LogSeekApiError> {
    let executor = SearchExecutor::new(pool, config);
    let cancel_token = CancellationToken::new();
    let sid = new_sid();

    // 执行搜索
    let result_rx = executor.search(&body.q, sid.clone(), ctx, Some(cancel_token)).await?;

    // 转换为 NDJSON
    let stream = convert_to_ndjson_stream(result_rx, highlights);
    build_ndjson_response(stream, sid)
}

// SearchExecutor
pub async fn search(
    &self,
    query: &str,
    sid: String,
    context_lines: usize,
    cancel_token: Option<CancellationToken>,
) -> Result<mpsc::Receiver<SearchEvent>, ServiceError> {
    // 规划搜索
    let (sources, request) = self.plan(query, context_lines).await?;

    // 并行执行
    for source in sources {
        let provider = create_search_provider(&pool, &source).await?;
        provider.search(&ctx, &request, &pool).await?;
    }
}
```

#### 新设计

```rust
// Router 层
pub async fn stream_search(
    State(resolver): State<Arc<ResourceResolver>>,
    Json(body): Json<SearchBody>,
) -> Result<HttpResponse<Body>, LogSeekApiError> {
    // 解析资源
    let (resources, query_config) = parse_query_to_resources(&body.q, body.context).await?;

    // 构造请求
    let requests: Vec<SearchRequest> = resources
        .into_iter()
        .map(|r| SearchRequest { ... })
        .collect();

    // 创建搜索服务
    let search_service = SearchService::new(resolver.clone());

    // 执行搜索
    let result_rx = search_service.search_multiple(
        requests,
        sid.clone(),
        cancel_token,
    ).await?;

    // NDJSON 转换 (保持不变)
    let stream = convert_to_ndjson_stream(result_rx, highlights);
    build_ndjson_response(stream, sid)
}

// SearchService
pub async fn search_multiple(
    &self,
    requests: Vec<SearchRequest>,
    sid: String,
    cancel_token: CancellationToken,
) -> mpsc::Receiver<SearchEvent> {
    // 为每个请求启动搜索
    for request in requests {
        let stream = self.search(request, cancel_token.clone()).await;
        // 转发事件到主通道
    }
}
```

---

## 4. 实现示例

### 4.1 完整的搜索流程示例

```rust
// 1. 用户发起请求
POST /api/v1/logseek/search.ndjson
{
  "q": "ERROR app:myapp -path:/var/log -exclude:*.gz",
  "context": 2
}

// 2. Router 解析
async fn stream_search(...) {
    // 解析查询
    let (resources, config) = parse_query_to_resources(&body.q, body.context).await?;
    // resources = [
    //   Resource { endpoint: Local, primary_path: "/var/log/myapp" },
    //   Resource { endpoint: S3("prod"), primary_path: "logs/myapp" },
    // ]

    // 构造搜索请求
    let requests = resources.into_iter().map(|r| SearchRequest {
        query: "ERROR".to_string(),  // 从查询中提取
        resource: r,
        config: SearchConfig::default(),
        context_lines: 2,
        path_includes: vec!["*.log".to_string()],
        path_excludes: vec!["*.gz".to_string()],
        encoding: None,
    }).collect();

    // 调用搜索服务
    let search_service = SearchService::new(resolver);
    let stream = search_service.search_multiple(requests, sid, token).await;
}

// 3. SearchService 处理
impl SearchService {
    pub async fn search_multiple(...) -> Receiver<SearchEvent> {
        for request in requests {
            // 根据 resource.endpoint 解析 FileSystem
            let fs = resolver.resolve(&request.resource).await?;

            // 检查 Searchable 支持
            let searchable = fs.as_any().downcast_ref::<dyn Searchable>()?;

            // 创建搜索流
            let entry_stream = searchable.create_entry_stream(
                &request.resource.primary_path,
                true,
                &request.config,
            ).await?;

            // 处理每个条目
            while let Some(entry) = entry_stream.next_entry().await {
                // 并发搜索文件内容
            }
        }
    }
}

// 4. LocalFileSystem::create_entry_stream
impl Searchable for LocalFileSystem {
    async fn create_entry_stream(...) -> Result<Box<dyn SearchEntryStream>> {
        // 返回本地文件列表
    }
}

// 5. AgentProxyFS::create_entry_stream
impl Searchable for AgentProxyFS {
    async fn create_entry_stream(...) -> Result<Box<dyn SearchEntryStream>> {
        // HTTP POST 到 Agent
        // 返回 SSE 流
    }
}
```

---

## 5. 迁移路径

### 5.1 阶段划分

#### 阶段 1: 保留兼容层
```rust
// 保留旧 API
pub async fn stream_search(
    State(pool): State<SqlitePool>,
    Json(body): Json<SearchBody>,
) -> Result<HttpResponse<Body>, LogSeekApiError> {
    // 内部使用新的 SearchService
    stream_search_v2(State(new_resolver), Json(body)).await
}
```

#### 阶段 2: 重构 SearchExecutor
```rust
// SearchExecutor 变为 SearchService 的薄包装
pub struct SearchExecutor {
    inner: Arc<SearchService>,
}

impl SearchExecutor {
    pub async fn search(...) -> Result<Receiver<SearchEvent>> {
        self.inner.search_multiple(requests, sid, token).await
    }
}
```

#### 阶段 3: 完全迁移
```rust
// Router 直接使用 SearchService
pub async fn stream_search(
    State(resolver): State<Arc<ResourceResolver>>,
    Json(body): Json<SearchBody>,
) -> Result<HttpResponse<Body>, LogSeekApiError> {
    // 直接调用 SearchService
}
```

### 5.2 兼容性保证

```rust
// 1. 保留 SearchEvent 枚举（可能需要重命名字段）
pub enum SearchEvent {
    Success(SearchResult),
    Error { source: String, message: String, recoverable: bool },
    Complete { source: String, elapsed_ms: u64 },
}

// 2. 保留 NDJSON 格式
// {"type":"result","data":{...}}
// {"type":"error","source":...,"message":"...","recoverable":true}
// {"type":"complete","source":"...","elapsed_ms":...}

// 3. 保留 X-Logseek-SID header
```

---

## 附录

### A. 关键文件清单

| 文件 | 作用 | 变化 |
|------|------|------|
| `logseek/src/routes/search.rs` | HTTP 路由 | 需要适配 |
| `logseek/src/service/search_executor.rs` | 搜索编排 | 重构为 SearchService |
| `logseek/src/service/searchable.rs` | 搜索接口 | 保留或重构 |
| `opsbox-core/src/dfs/services/search.rs` | DFS 搜索服务 | 新增 |
| `opsbox-core/src/dfs/fs/searchable.rs` | Searchable trait | 新增 |
| `logseek/src/infra/local_fs.rs` | LocalFS 实现 | 实现 Searchable |
| `logseek/src/infra/agent_proxy_fs.rs` | Agent 代理 | 实现 Searchable |

### B. 数据流图

```
HTTP Request (SearchBody)
        │
        ├─── parse_query_to_resources()
        │
        ▼
    Vec<Resource>
        │
        ├─── Resource { endpoint: Local, path: "/var/log" }
        ├─── Resource { endpoint: S3("prod"), path: "logs/app" }
        └─── Resource { endpoint: Agent(...), path: "/logs" }
        │
        ▼
    SearchService::search_multiple()
        │
        ├─── LocalFileSystem::create_entry_stream()
        │     └──► walkdir → entries → files
        │
        ├─── S3Storage::create_entry_stream()
        │     └──► ListObjectsV2 → objects → files
        │
        └─── AgentProxyFS::create_entry_stream()
              └──► HTTP POST → Agent → LocalFS
```

---

**文档版本**: 1.0
**最后更新**: 2026-02-02
