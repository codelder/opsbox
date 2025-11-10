# 异常处理分层设计评估

## 概述

你的代码分层（routes → service → repository → domain）基本合理，但**异常处理分层不够清晰**。目前错误类型零散分布在各层，缺乏统一的分层转换策略。

---

## 当前分层结构分析

### 现状：错误类型分布

```
domain/           -> FileUrlError (单一错误类型)
service/          -> SearchError (单一错误类型)
repository/       -> 依赖 AppError (core)
api/              -> LogSeekApiError + AppError (混合)
routes/           -> LogSeekApiError → Problem (转换)
opsbox-core/      -> AppError (统一类型)
```

### 问题

1. **错误类型过多且职责模糊**
   - `FileUrlError`: 只用于 URL 解析
   - `SearchError`: 只用于搜索处理
   - `LogSeekApiError`: 混合了来自 service、repository 的错误
   - `AppError`: 核心统一类型，但被直接在各层使用

2. **错误转换不系统**
   - `SearchError` → `LogSeekApiError` 的转换规则没有明确定义
   - `S3Error` 在 repository 层被直接转换为 `AppError`
   - Domain 层的 `FileUrlError` 在何处转换不清楚

3. **错误上下文丢失**
   - 底层错误被转换时，原始路径/操作信息可能丢失
   - 例如：`SearchError::Io { path, error }` 被转为 `AppError::Internal`

4. **流式 API 中的错误处理不一致**
   - Search 路由返回空流（吞掉错误）
   - 其他路由返回 Problem Details
   - 没有统一的流式错误处理机制

---

## 推荐的分层错误处理架构

### 分层模型

```
┌─────────────────────────────────────────┐
│ HTTP Layer (routes)                      │
│ - 转换为 RFC 7807 Problem Details        │
│ - 或流式错误消息                        │
│ - 设置正确的 HTTP 状态码                 │
└─────────────────┬───────────────────────┘
                  │
┌─────────────────▼───────────────────────┐
│ API Error Layer (api/)                   │
│ - ModuleError 或 LogSeekApiError         │
│ - From<ServiceError>                    │
│ - From<RepositoryError>                 │
│ - From<DomainError>                     │
└─────────────────┬───────────────────────┘
                  │
  ┌───────────────┼───────────────────┐
  │               │                   │
  ▼               ▼                   ▼
Service Layer  Repo Layer        Domain Layer
ServiceError   RepoError         DomainError
```

---

## 具体改进建议

### 1. 定义层级错误类型 (layer-specific errors)

#### 1a. Domain 层保持不变 ✓
```rust
// domain/file_url.rs (已经很好)
#[derive(Debug, Error)]
pub enum FileUrlError {
  #[error("无效的 URL 格式: {0}")]
  InvalidFormat(String),
  #[error("不支持的协议: {0}")]
  UnsupportedScheme(String),
  #[error("缺少必需字段: {0}")]
  MissingField(&'static str),
  #[error("嵌套层级过深（最多支持 1 层 tar 嵌套）")]
  TooManyNestingLevels,
}
```

#### 1b. Service 层定义明确的错误 ✓ (现有SearchError已基本可行)
```rust
// service/mod.rs
#[derive(Debug, Error)]
pub enum ServiceError {
  #[error("路径处理失败: {0}")]
  PathError(String),
  
  #[error("文件搜索失败: path={path}, error={error}")]
  SearchFailed { path: String, error: String },
  
  #[error("数据处理错误: {0}")]
  DataProcessing(String),
  
  // 数据库和存储错误通过 From 转换
  #[error("数据库错误")]
  Database(#[from] sqlx::Error),
}
```

#### 1c. Repository 层定义专用错误
```rust
// repository/error.rs (新建)
#[derive(Debug, Error)]
pub enum RepositoryError {
  #[error("查询失败: {0}")]
  QueryFailed(String),
  
  #[error("对象存储错误: {0}")]
  StorageError(String),
  
  #[error("缓存操作失败: {0}")]
  CacheFailed(String),
  
  #[error("配置不存在: {0}")]
  NotFound(String),
  
  #[error("数据库错误: {0}")]
  Database(#[from] sqlx::Error),
}
```

#### 1d. API 层定义转换器
```rust
// api/error.rs (改进现有 LogSeekApiError)
#[derive(Debug, Error)]
pub enum LogSeekApiError {
  #[error(transparent)]
  Service(#[from] ServiceError),
  
  #[error(transparent)]
  Repository(#[from] RepositoryError),
  
  #[error(transparent)]
  Domain(#[from] FileUrlError),
  
  #[error("JSON 解析失败: {0}")]
  BadJson(#[from] JsonRejection),
  
  #[error("查询语法错误: {0}")]
  QueryParse(#[from] ParseError),
  
  // 对接 core 错误
  #[error(transparent)]
  Internal(#[from] opsbox_core::AppError),
}

impl From<LogSeekApiError> for Problem {
  fn from(error: LogSeekApiError) -> Self {
    let (status, title) = match &error {
      LogSeekApiError::Service(e) => match e {
        ServiceError::SearchFailed { .. } => 
          (StatusCode::INTERNAL_SERVER_ERROR, "搜索失败"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "服务错误"),
      },
      LogSeekApiError::Repository(e) => match e {
        RepositoryError::NotFound(_) => 
          (StatusCode::NOT_FOUND, "资源不存在"),
        RepositoryError::StorageError(_) => 
          (StatusCode::BAD_GATEWAY, "存储服务错误"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "数据访问错误"),
      },
      LogSeekApiError::Domain(FileUrlError::InvalidFormat(_)) => 
        (StatusCode::BAD_REQUEST, "无效的 URL 格式"),
      LogSeekApiError::BadJson(_) => 
        (StatusCode::BAD_REQUEST, "JSON 请求错误"),
      LogSeekApiError::QueryParse(_) => 
        (StatusCode::BAD_REQUEST, "查询语法错误"),
      _ => (StatusCode::INTERNAL_SERVER_ERROR, "内部错误"),
    };

    let detail = error.to_string();
    problemdetails::new(status)
      .with_title(title)
      .with_detail(detail)
  }
}
```

### 2. 错误转换规则（隐式 From 链）

```rust
// 数据流向
FileUrlError (domain) 
    ↓ (自动 From in LogSeekApiError)
LogSeekApiError
    ↓ (axum 自动调用 IntoResponse)
Problem Details + HTTP Response

SearchError (service)
    ↓ (应通过 From<ServiceError> for LogSeekApiError)
LogSeekApiError
    ↓
Problem Details + HTTP Response

RepositoryError (repo)
    ↓ (应通过 From<RepositoryError> for LogSeekApiError)
LogSeekApiError
    ↓
Problem Details + HTTP Response
```

### 3. 流式 API 错误处理标准化

在 search.rs 中，而不是返回空流，应该发送结构化错误：

```rust
// routes/search.rs
pub async fn stream_search(...) -> Result<HttpResponse<Body>, LogSeekApiError> {
  let cap = stream_channel_capacity();
  let (tx, rx) = mpsc::channel::<Result<SearchEvent, String>>(cap);

  // 分层限流
  let io_sem = Arc::new(tokio::sync::Semaphore::new(s3_max_concurrency()));

  // 1. 获取存储源配置列表
  let (source_configs, cleaned_query) = get_storage_source_configs(&pool, &body.q)
    .await
    .map_err(|e| {
      // 立即尝试通过通道发送错误
      let tx_err = tx.clone();
      tokio::spawn(async move {
        let err_msg = format!("{{\"error\":{:?}}}\n", e.to_string());
        let _ = tx_err.send(Err(err_msg)).await;
      });
      e
    })?;

  if source_configs.is_empty() {
    // 返回带错误的流
    let (tx, rx) = mpsc::channel(1);
    tokio::spawn(async move {
      let _ = tx.send(Err("没有可用的存储源".to_string())).await;
    });
    return respond_stream(rx);
  }

  // ... 继续处理
}

// 定义统一的流事件类型
#[derive(Debug, Serialize)]
pub enum SearchEvent {
  #[serde(rename = "result")]
  Result(SearchResult),
  #[serde(rename = "error")]
  Error { message: String },
  #[serde(rename = "stats")]
  Stats { total: usize, duration_ms: u64 },
}
```

### 4. Routes 层错误处理标准

```rust
// routes/search.rs - 推荐模式
pub async fn stream_search(
  State(pool): State<SqlitePool>,
  Json(body): Json<SearchBody>,
) -> Result<HttpResponse<Body>, LogSeekApiError> {  // ← 使用统一的 LogSeekApiError
  log::info!("[Search] 开始搜索: q={}", body.q);

  // 验证输入（返回 Domain 错误）
  let spec = Query::parse_github_like(&body.q)
    .map_err(LogSeekApiError::from)?;  // FileUrlError / ParseError 自动转换

  // 调用 service（返回 ServiceError）
  let results = service::search(&pool, &spec)
    .await
    .map_err(LogSeekApiError::from)?;  // ServiceError 自动转换

  // ... 构建响应

  Ok(response)
}
```

### 5. Service 层错误处理标准

```rust
// service/search.rs
pub async fn search(
  pool: &SqlitePool,
  spec: &Query,
) -> Result<Vec<SearchResult>, ServiceError> {
  // 从 repository 获取配置（返回 RepositoryError）
  let config = crate::repository::settings::load_required_s3_settings(pool)
    .await
    .map_err(|e| ServiceError::ConfigNotFound(e.to_string()))?;

  // 调用存储层（返回 S3Error）
  let objects = storage::list_objects(&config)
    .await
    .map_err(|e| ServiceError::StorageFailed(e.to_string()))?;

  // ... 处理搜索

  Ok(results)
}
```

### 6. Repository 层错误处理标准

```rust
// repository/settings.rs
pub async fn load_required_s3_settings(pool: &SqlitePool) 
  -> Result<S3Settings, RepositoryError> {
  
  let row = sqlx::query_as(...)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
      RepositoryError::QueryFailed(format!("查询 S3 配置失败: {}", e))
    })?;

  row.ok_or(RepositoryError::NotFound(
    "S3 配置不存在".to_string()
  ))
}
```

---

## 实施路线图

### Phase 1: 基础设施 (1-2天)
- [ ] 创建 `repository/error.rs` 定义 `RepositoryError`
- [ ] 创建 `service/error.rs` 定义 `ServiceError`（迁移现有 `SearchError`）
- [ ] 改进 `api/error.rs` 的 `LogSeekApiError` 实现完整的 From 转换

### Phase 2: 逐层改进 (3-5天)
- [ ] Repository 层：统一所有 `AppError` 为 `RepositoryError`
- [ ] Service 层：统一所有 `AppError` 为 `ServiceError`
- [ ] Routes 层：所有处理器返回 `Result<T, LogSeekApiError>`

### Phase 3: 流式 API 标准化 (2-3天)
- [ ] 定义 `SearchEvent` 枚举
- [ ] 改进 `stream_search` 错误处理，发送结构化错误而非空流
- [ ] 其他流式端点应用相同模式

### Phase 4: 测试和文档 (2天)
- [ ] 添加错误转换的单元测试
- [ ] 更新 API 文档说明错误响应格式
- [ ] 补充错误处理最佳实践文档

---

## 分层对比表

| 层级 | 职责 | 错误类型 | 处理方式 |
|------|------|--------|--------|
| **Domain** | 业务模型、验证 | `FileUrlError`, `ParseError` | 定义清晰的错误类型 |
| **Service** | 业务逻辑、流程 | `ServiceError` | 捕获下层错误，转换为上层类型 |
| **Repository** | 数据持久化 | `RepositoryError` | 捕获 sqlx/storage 错误，转换 |
| **API** | 协议转换 | `LogSeekApiError` | 聚合所有下层错误类型 |
| **Routes** | HTTP 处理 | `LogSeekApiError` | 转换为 Problem Details |

---

## 关键原则

1. **向上不向下** - 错误沿调用链向上传播，在 API 层统一转换
2. **语义清晰** - 每层错误类型反映该层的问题域
3. **上下文保留** - 错误转换时保留原始信息（路径、操作等）
4. **一致的日志** - 每个错误类型都有明确的日志级别
5. **客户端友好** - 最终的 Problem Details 包含清晰的用户友好消息

---

## 额外建议

### 1. 为 Routes 层添加全局错误处理器
```rust
// 可选：更高级的错误处理
impl IntoResponse for LogSeekApiError {
  fn into_response(self) -> Response {
    let status = match &self { ... };
    let problem = Problem::from(self);
    
    log::error!("[LogSeek API] {:?}", self);  // 统一日志
    
    (status, Json(problem)).into_response()
  }
}
```

### 2. 添加错误链追踪
使用 `anyhow` 或 `eyre` 在开发环境保留完整的错误链：
```rust
#[cfg(debug_assertions)]
impl LogSeekApiError {
  pub fn with_context(self, context: &str) -> Self {
    log::debug!("Error context: {}", context);
    self
  }
}
```

### 3. 监控和告警
在 Problem Details 中添加 `trace_id` 便于追踪：
```rust
struct ProblemDetail {
  r#type: String,
  title: String,
  status: u16,
  detail: String,
  trace_id: String,  // ← 新增
}
```

---

## 总结

你的**分层本身合理**（domain → service → repo → routes），但需要在**错误处理上建立对应的分层策略**。

关键是：
- ✅ 每层定义自己的错误类型
- ✅ 使用 From 特征建立隐式转换链
- ✅ API 层作为聚合点统一转换为客户端格式
- ✅ 流式 API 需要特殊处理，发送结构化错误事件而不是吞掉

这样做的好处：
- 代码更可维护（每层职责清晰）
- 错误信息更完整（上下文不丢失）
- 测试更容易（可独立测试错误转换）
- 新人更容易理解（遵循清晰的约定）
