# Tracing 使用指南

本指南介绍如何在 OpsBox 项目中使用 `tracing` 框架进行日志记录和追踪。

## 概述

OpsBox 使用 `tracing` 替代传统的 `log` crate，提供更强大的结构化日志和分布式追踪能力。

**主要优势：**
- 结构化日志（key-value pairs）
- Span 追踪（跨函数调用的上下文）
- 零成本抽象（编译时优化）
- 丰富的生态系统

## 快速开始

### 导入 tracing

```rust
// 在文件顶部导入
use tracing::{debug, error, info, trace, warn};

// 或者导入所有宏
use tracing::*;
```

### 基本日志记录

```rust
// 简单日志
tracing::info!("Server started");

// 带格式化的日志
tracing::info!("Server started on port {}", port);

// 带结构化字段的日志
tracing::info!(
    port = 4000,
    host = "127.0.0.1",
    "Server started"
);
```

## 日志级别

### 五个日志级别

```rust
// ERROR - 错误信息（最高优先级）
tracing::error!("Failed to connect to database");

// WARN - 警告信息
tracing::warn!("Connection pool is running low");

// INFO - 信息日志（默认级别）
tracing::info!("Request processed successfully");

// DEBUG - 调试信息
tracing::debug!("Cache hit for key: {}", key);

// TRACE - 追踪信息（最低优先级）
tracing::trace!("Entering function");
```

### 级别选择指南

| 级别 | 使用场景 | 示例 |
|------|----------|------|
| **ERROR** | 系统错误、异常情况 | 数据库连接失败、文件读取错误 |
| **WARN** | 警告、潜在问题 | 连接池不足、配置缺失（使用默认值） |
| **INFO** | 关键操作、状态变化 | 服务启动/关闭、请求处理、配置更新 |
| **DEBUG** | 详细调试信息 | 函数参数、中间结果、缓存命中 |
| **TRACE** | 最详细的追踪 | 函数进入/退出、循环迭代 |

## 结构化字段

### 基本用法

```rust
// 使用 = 添加字段
tracing::info!(
    user_id = 123,
    action = "login",
    "User logged in"
);

// 输出：
// 2024-01-15T10:30:45.123Z  INFO app: User logged in user_id=123 action="login"
```

### 字段格式化

```rust
// % - Display 格式化
tracing::info!(user_id = %user_id, "User logged in");

// ? - Debug 格式化
tracing::debug!(config = ?config, "Loaded configuration");

// 默认 - 使用 Display（如果实现了）
tracing::info!(count = count, "Processed items");
```

### 字段类型

```rust
// 数字
tracing::info!(count = 42, "Items processed");

// 字符串
tracing::info!(name = "Alice", "User created");

// 布尔值
tracing::info!(success = true, "Operation completed");

// 自定义类型（需要实现 Display 或 Debug）
tracing::info!(user = ?user, "User details");

// 引用
tracing::info!(path = %path.display(), "File opened");
```

## Span 追踪

### 什么是 Span？

Span 表示一段时间内的操作，可以跨越多个函数调用，用于追踪请求的完整生命周期。

### 创建 Span

```rust
// 手动创建 span
let span = tracing::info_span!("request", method = "GET", path = "/api/search");

// 进入 span
let _enter = span.enter();

// 在 span 内的所有日志都会包含 span 的上下文
tracing::info!("Processing request");

// _enter 在作用域结束时自动退出 span
```

### 使用 instrument 宏

```rust
// 自动为函数创建 span
#[tracing::instrument]
async fn process_request(user_id: i64) -> Result<Response> {
    tracing::info!("Processing request");
    // ...
    Ok(Response::success())
}

// 输出：
// 2024-01-15T10:30:45.123Z  INFO process_request{user_id=123}: Processing request
```

### instrument 宏选项

```rust
// 跳过某些参数（避免记录敏感信息）
#[tracing::instrument(skip(password))]
async fn authenticate(username: &str, password: &str) -> Result<Token> {
    // ...
}

// 自定义 span 名称
#[tracing::instrument(name = "db_query")]
async fn query_database(sql: &str) -> Result<Vec<Row>> {
    // ...
}

// 自定义日志级别
#[tracing::instrument(level = "debug")]
async fn internal_function() {
    // ...
}

// 记录返回值
#[tracing::instrument(ret)]
async fn calculate(x: i32, y: i32) -> i32 {
    x + y
}

// 记录错误
#[tracing::instrument(err)]
async fn may_fail() -> Result<(), Error> {
    // ...
}
```

### Span 嵌套

```rust
async fn handle_request(req: Request) -> Result<Response> {
    let span = tracing::info_span!("handle_request", request_id = %req.id);
    let _enter = span.enter();
    
    tracing::info!("Received request");
    
    // 嵌套 span
    let user = {
        let span = tracing::debug_span!("fetch_user", user_id = req.user_id);
        let _enter = span.enter();
        
        tracing::debug!("Fetching user from database");
        db.get_user(req.user_id).await?
    };
    
    tracing::info!(username = %user.name, "User found");
    
    Ok(Response::success())
}

// 输出：
// 2024-01-15T10:30:45.123Z  INFO handle_request{request_id="abc123"}: Received request
// 2024-01-15T10:30:45.456Z DEBUG handle_request{request_id="abc123"}:fetch_user{user_id=123}: Fetching user from database
// 2024-01-15T10:30:45.789Z  INFO handle_request{request_id="abc123"}: User found username="Alice"
```

## 错误处理

### 记录错误

```rust
// 基本错误记录
match operation().await {
    Ok(result) => Ok(result),
    Err(e) => {
        tracing::error!(error = %e, "Operation failed");
        Err(e)
    }
}

// 带上下文的错误记录
match db.query(sql).await {
    Ok(rows) => Ok(rows),
    Err(e) => {
        tracing::error!(
            error = %e,
            sql = %sql,
            "Database query failed"
        );
        Err(e)
    }
}
```

### 使用 instrument 自动记录错误

```rust
#[tracing::instrument(err)]
async fn risky_operation() -> Result<(), Error> {
    // 如果返回 Err，会自动记录错误日志
    Err(Error::new("Something went wrong"))
}
```

### 错误链追踪

```rust
use tracing::error;

fn handle_error(e: &dyn std::error::Error) {
    error!(error = %e, "Operation failed");
    
    // 记录错误链
    let mut source = e.source();
    while let Some(e) = source {
        error!(cause = %e, "Caused by");
        source = e.source();
    }
}
```

## 性能优化

### 条件日志

```rust
// ❌ 不好的实践：总是计算参数
tracing::debug!("Result: {}", expensive_computation());

// ✅ 好的实践：使用闭包延迟计算
if tracing::enabled!(tracing::Level::DEBUG) {
    tracing::debug!("Result: {}", expensive_computation());
}

// 或者使用 tracing 的内置优化
tracing::debug!(result = ?expensive_computation(), "Computed result");
```

### 避免过度日志

```rust
// ❌ 不好的实践：在循环中记录每次迭代
for item in items {
    tracing::debug!("Processing item: {:?}", item);
    process(item);
}

// ✅ 好的实践：记录批次信息
tracing::debug!(count = items.len(), "Processing items");
for item in items {
    process(item);
}
tracing::debug!("Items processed");
```

### 使用 span 而不是重复字段

```rust
// ❌ 不好的实践：在每条日志中重复字段
tracing::info!(request_id = %req_id, "Starting request");
tracing::debug!(request_id = %req_id, "Fetching data");
tracing::info!(request_id = %req_id, "Request completed");

// ✅ 好的实践：使用 span
let span = tracing::info_span!("request", request_id = %req_id);
let _enter = span.enter();

tracing::info!("Starting request");
tracing::debug!("Fetching data");
tracing::info!("Request completed");
```

## 常见模式

### HTTP 请求处理

```rust
#[tracing::instrument(
    skip(req, db),
    fields(
        method = %req.method(),
        path = %req.uri().path(),
        request_id = %generate_request_id()
    )
)]
async fn handle_request(
    req: Request<Body>,
    db: Database,
) -> Result<Response<Body>> {
    tracing::info!("Received request");
    
    // 处理请求
    let result = process_request(&req, &db).await?;
    
    tracing::info!(status = 200, "Request completed");
    Ok(Response::new(Body::from(result)))
}
```

### 数据库操作

```rust
#[tracing::instrument(skip(pool), err)]
async fn query_users(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<User>, sqlx::Error> {
    tracing::debug!("Executing query");
    
    let users = sqlx::query_as::<_, User>(
        "SELECT * FROM users LIMIT ?"
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    
    tracing::debug!(count = users.len(), "Query completed");
    Ok(users)
}
```

### 后台任务

```rust
async fn background_task() {
    let span = tracing::info_span!("background_task");
    let _enter = span.enter();
    
    tracing::info!("Task started");
    
    loop {
        match process_batch().await {
            Ok(count) => {
                tracing::debug!(count, "Batch processed");
            }
            Err(e) => {
                tracing::error!(error = %e, "Batch processing failed");
            }
        }
        
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
```

### 重试逻辑

```rust
#[tracing::instrument(skip(operation))]
async fn retry_with_backoff<F, T, E>(
    operation: F,
    max_retries: u32,
) -> Result<T, E>
where
    F: Fn() -> Future<Output = Result<T, E>>,
{
    let mut retries = 0;
    
    loop {
        match operation().await {
            Ok(result) => {
                if retries > 0 {
                    tracing::info!(retries, "Operation succeeded after retries");
                }
                return Ok(result);
            }
            Err(e) if retries < max_retries => {
                retries += 1;
                let delay = Duration::from_secs(2u64.pow(retries));
                
                tracing::warn!(
                    retries,
                    delay_secs = delay.as_secs(),
                    "Operation failed, retrying"
                );
                
                tokio::time::sleep(delay).await;
            }
            Err(e) => {
                tracing::error!(retries, "Operation failed after max retries");
                return Err(e);
            }
        }
    }
}
```

## 最佳实践

### 1. 使用合适的日志级别

```rust
// ✅ 好的实践
tracing::error!("Database connection failed");  // 真正的错误
tracing::warn!("Cache miss, fetching from database");  // 警告
tracing::info!("User logged in");  // 关键操作
tracing::debug!("Cache hit for key: {}", key);  // 调试信息
tracing::trace!("Entering function");  // 详细追踪

// ❌ 不好的实践
tracing::error!("User not found");  // 这不是错误，应该用 warn 或 info
tracing::info!("Variable x = {}", x);  // 这是调试信息，应该用 debug
```

### 2. 使用结构化字段

```rust
// ✅ 好的实践：使用结构化字段
tracing::info!(
    user_id = user.id,
    username = %user.name,
    action = "login",
    "User logged in"
);

// ❌ 不好的实践：将所有信息放在消息中
tracing::info!("User {} (ID: {}) logged in", user.name, user.id);
```

### 3. 避免记录敏感信息

```rust
// ❌ 不好的实践：记录敏感信息
tracing::info!(password = %password, "User authenticated");
tracing::debug!(api_key = %api_key, "Making API request");

// ✅ 好的实践：跳过敏感信息
#[tracing::instrument(skip(password))]
async fn authenticate(username: &str, password: &str) -> Result<Token> {
    tracing::info!(username, "Authenticating user");
    // ...
}
```

### 4. 使用 instrument 宏

```rust
// ✅ 好的实践：使用 instrument 宏
#[tracing::instrument(skip(db), err)]
async fn get_user(db: &Database, user_id: i64) -> Result<User> {
    // 自动创建 span 和记录错误
    db.query_user(user_id).await
}

// ❌ 不好的实践：手动创建 span
async fn get_user(db: &Database, user_id: i64) -> Result<User> {
    let span = tracing::info_span!("get_user", user_id);
    let _enter = span.enter();
    
    match db.query_user(user_id).await {
        Ok(user) => Ok(user),
        Err(e) => {
            tracing::error!(error = %e, "Failed to get user");
            Err(e)
        }
    }
}
```

### 5. 提供有意义的消息

```rust
// ✅ 好的实践：清晰的消息
tracing::info!("Server started successfully");
tracing::error!("Failed to connect to database");

// ❌ 不好的实践：模糊的消息
tracing::info!("Done");
tracing::error!("Error");
```

### 6. 记录上下文信息

```rust
// ✅ 好的实践：记录足够的上下文
tracing::error!(
    error = %e,
    file_path = %path,
    line_number = line,
    "Failed to parse configuration file"
);

// ❌ 不好的实践：缺少上下文
tracing::error!("Parse failed");
```

## 测试中的日志

### 启用测试日志

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber;

    #[test]
    fn test_with_logging() {
        // 初始化测试日志
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .try_init();
        
        tracing::info!("Running test");
        
        // 测试代码
        assert_eq!(2 + 2, 4);
    }
}
```

### 捕获日志输出

```rust
#[cfg(test)]
mod tests {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    #[test]
    fn test_log_output() {
        let (writer, handle) = tracing_subscriber::fmt::TestWriter::new();
        
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_writer(writer))
            .init();
        
        tracing::info!("Test message");
        
        let output = handle.to_string();
        assert!(output.contains("Test message"));
    }
}
```

## 迁移指南

### 从 log 迁移到 tracing

```rust
// 旧代码（使用 log）
use log::{debug, error, info, warn};

info!("Server started on port {}", port);
debug!("Processing request");
error!("Failed to connect: {}", e);

// 新代码（使用 tracing）
use tracing::{debug, error, info, warn};

info!(port = port, "Server started");
debug!("Processing request");
error!(error = %e, "Failed to connect");
```

### 添加结构化字段

```rust
// 旧代码
log::info!("User {} logged in from {}", user_id, ip);

// 新代码
tracing::info!(
    user_id = user_id,
    ip = %ip,
    "User logged in"
);
```

### 使用 span 替代重复字段

```rust
// 旧代码
log::info!("[req:{}] Starting request", req_id);
log::debug!("[req:{}] Fetching data", req_id);
log::info!("[req:{}] Request completed", req_id);

// 新代码
let span = tracing::info_span!("request", request_id = %req_id);
let _enter = span.enter();

tracing::info!("Starting request");
tracing::debug!("Fetching data");
tracing::info!("Request completed");
```

## 常见问题

### Q: 什么时候使用 % 和 ? 格式化？

A: 
- `%` 使用 `Display` trait，适合用户友好的输出
- `?` 使用 `Debug` trait，适合调试信息
- 默认（无前缀）尝试使用 `Display`，如果没有实现则编译错误

```rust
tracing::info!(count = count, "Items");  // 使用 Display
tracing::info!(count = %count, "Items");  // 显式使用 Display
tracing::debug!(config = ?config, "Config");  // 使用 Debug
```

### Q: instrument 宏会影响性能吗？

A: 影响很小。tracing 使用零成本抽象，未启用的日志级别会被编译器优化掉。只有在启用对应日志级别时才会有运行时开销。

### Q: 如何在异步代码中使用 span？

A: 使用 `instrument` 宏或 `Instrument` trait：

```rust
// 使用 instrument 宏（推荐）
#[tracing::instrument]
async fn my_async_fn() {
    // ...
}

// 使用 Instrument trait
use tracing::Instrument;

async fn my_async_fn() {
    // ...
}

let span = tracing::info_span!("my_async_fn");
my_async_fn().instrument(span).await;
```

### Q: 如何避免日志过多？

A: 
1. 使用合适的日志级别
2. 避免在循环中记录每次迭代
3. 使用 span 避免重复字段
4. 在生产环境使用 INFO 级别

### Q: 如何记录第三方库的日志？

A: 使用 `RUST_LOG` 环境变量过滤：

```bash
# 只显示自己的日志
RUST_LOG=opsbox_server=debug

# 显示自己的日志和特定第三方库
RUST_LOG=opsbox_server=debug,sqlx=info

# 显示所有日志但过滤掉噪音
RUST_LOG=debug,hyper=warn,tokio=warn
```

## 相关文档

- [日志配置指南](./logging-configuration.md) - 用户配置指南
- [日志系统架构](../architecture/logging-architecture.md) - 架构设计文档
- [tracing 官方文档](https://docs.rs/tracing/) - 官方 API 文档
- [tracing-subscriber 文档](https://docs.rs/tracing-subscriber/) - Subscriber 配置
