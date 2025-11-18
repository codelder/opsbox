# 日志系统架构

本文档描述 OpsBox 日志系统的架构设计和实现细节。

## 概述

OpsBox 使用 `tracing` 生态系统替代传统的 `log` crate，提供结构化日志和追踪能力。日志系统支持：

- 滚动日志文件（按日期自动滚动）
- 动态日志级别调整（无需重启）
- 多输出目标（控制台 + 文件）
- 异步日志写入（避免阻塞主线程）
- 持久化配置（SQLite 数据库）
- REST API 管理接口

## 架构图

### 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                    Server/Agent 进程                         │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────┐         ┌─────────────────────────────────┐  │
│  │ main.rs  │────────>│  logging::init()                │  │
│  └──────────┘         │  - 创建 Console Layer           │  │
│                       │  - 创建 File Layer              │  │
│                       │  - 配置 tracing-subscriber      │  │
│                       └─────────────────────────────────┘  │
│                                 │                           │
│                                 v                           │
│                       ┌─────────────────────────────────┐  │
│                       │  tracing-subscriber             │  │
│                       │  ┌──────────────────────────┐   │  │
│                       │  │  EnvFilter (日志过滤)    │   │  │
│                       │  └──────────────────────────┘   │  │
│                       │  ┌──────────────────────────┐   │  │
│                       │  │  Console Layer (彩色)    │   │  │
│                       │  └──────────────────────────┘   │  │
│                       │  ┌──────────────────────────┐   │  │
│                       │  │  File Layer (滚动)       │   │  │
│                       │  └──────────────────────────┘   │  │
│                       └─────────────────────────────────┘  │
│                                 │                           │
│                    ┌────────────┴────────────┐             │
│                    v                         v              │
│          ┌──────────────────┐    ┌──────────────────────┐ │
│          │  stdout/stderr   │    │  RollingFileAppender │ │
│          └──────────────────┘    │  - 按日期滚动        │ │
│                                   │  - 自动清理旧文件    │ │
│                                   └──────────────────────┘ │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  REST API (log_routes.rs)                            │  │
│  │  - GET  /api/v1/log/config                           │  │
│  │  - PUT  /api/v1/log/level                            │  │
│  │  - PUT  /api/v1/log/retention                        │  │
│  └──────────────────────────────────────────────────────┘  │
│                    │                                         │
│                    v                                         │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  LogConfigRepository (repository.rs)                 │  │
│  │  - get()                                             │  │
│  │  - update_level()                                    │  │
│  │  - update_retention()                                │  │
│  └──────────────────────────────────────────────────────┘  │
│                    │                                         │
│                    v                                         │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  SQLite Database (log_config 表)                     │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Agent 日志代理架构

```
┌──────────────┐         ┌──────────────┐         ┌──────────────┐
│   Frontend   │────────>│    Server    │────────>│    Agent     │
│              │         │              │         │              │
│  日志管理UI  │         │ Agent Manager│         │  日志 API    │
│              │         │   (代理)     │         │              │
└──────────────┘         └──────────────┘         └──────────────┘
      │                         │                         │
      │ PUT /agents/{id}/       │ PUT /log/level         │
      │     log/level           │                         │
      └────────────────────────>│                         │
                                └────────────────────────>│
                                                          │
                                                          v
                                                  ┌──────────────┐
                                                  │ ReloadHandle │
                                                  │ (动态重载)   │
                                                  └──────────────┘
```

## 核心组件

### 1. logging 模块 (opsbox-core/src/logging.rs)

日志系统的核心模块，负责初始化和配置 tracing。

#### 主要类型

```rust
/// 日志配置
pub struct LogConfig {
    /// 日志级别
    pub level: LogLevel,
    /// 日志目录
    pub log_dir: PathBuf,
    /// 日志保留数量（天）
    pub retention_count: usize,
    /// 是否启用控制台输出
    pub enable_console: bool,
    /// 是否启用文件输出
    pub enable_file: bool,
}

/// 日志级别
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// 重载句柄（用于动态修改日志级别）
pub struct ReloadHandle {
    inner: tracing_subscriber::reload::Handle<...>,
}
```

#### 初始化流程

```rust
pub fn init(config: LogConfig) -> Result<ReloadHandle, LogError> {
    // 1. 创建 EnvFilter（支持 RUST_LOG 环境变量）
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(config.level.as_str()))
        .unwrap();

    // 2. 创建 Console Layer（带彩色输出）
    let console_layer = if config.enable_console {
        Some(
            fmt::layer()
                .with_writer(std::io::stdout)
                .with_ansi(true)
                .with_target(true)
                .with_level(true)
        )
    } else {
        None
    };

    // 3. 创建 File Layer（使用 RollingFileAppender）
    let file_layer = if config.enable_file {
        let file_appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("opsbox-server")
            .filename_suffix("log")
            .max_log_files(config.retention_count)
            .build(config.log_dir)?;

        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
        
        Some(
            fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_target(true)
                .with_level(true)
        )
    } else {
        None
    };

    // 4. 组合 Layers 并设置为全局默认
    let (filter, reload_handle) = reload::Layer::new(filter);
    
    tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    // 5. 返回 reload handle
    Ok(ReloadHandle { inner: reload_handle })
}
```

### 2. LogConfigRepository (opsbox-core/src/logging/repository.rs)

负责日志配置的持久化存储。

#### 数据库 Schema

```sql
CREATE TABLE IF NOT EXISTS log_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),  -- 单例配置
    component TEXT NOT NULL,                 -- 'server' 或 'agent'
    level TEXT NOT NULL,                     -- 日志级别
    retention_count INTEGER NOT NULL,        -- 保留文件数量
    updated_at INTEGER NOT NULL              -- 更新时间戳
);
```

#### 主要方法

```rust
impl LogConfigRepository {
    /// 获取日志配置
    pub async fn get(&self, component: &str) -> Result<LogConfig, RepositoryError>;
    
    /// 更新日志级别
    pub async fn update_level(&self, component: &str, level: LogLevel) 
        -> Result<(), RepositoryError>;
    
    /// 更新日志保留数量
    pub async fn update_retention(&self, component: &str, count: usize) 
        -> Result<(), RepositoryError>;
}
```

### 3. REST API (opsbox-server/src/log_routes.rs)

提供 HTTP API 用于管理日志配置。

#### API 端点

**Server 日志配置：**
- `GET /api/v1/log/config` - 获取当前日志配置
- `PUT /api/v1/log/level` - 更新日志级别
- `PUT /api/v1/log/retention` - 更新日志保留数量

**Agent 日志配置（通过 Server 代理）：**
- `GET /api/v1/agents/{agent_id}/log/config` - 获取 Agent 日志配置
- `PUT /api/v1/agents/{agent_id}/log/level` - 更新 Agent 日志级别
- `PUT /api/v1/agents/{agent_id}/log/retention` - 更新 Agent 日志保留数量

#### 请求/响应格式

```rust
// 获取配置响应
#[derive(Serialize, Deserialize)]
pub struct LogConfigResponse {
    pub level: String,
    pub retention_count: usize,
    pub log_dir: String,
}

// 更新日志级别请求
#[derive(Serialize, Deserialize)]
pub struct UpdateLogLevelRequest {
    pub level: String,  // "error" | "warn" | "info" | "debug" | "trace"
}

// 更新保留数量请求
#[derive(Serialize, Deserialize)]
pub struct UpdateRetentionRequest {
    pub retention_count: usize,
}
```

### 4. Agent Manager 代理 (agent-manager/src/routes.rs)

负责将前端的 Agent 日志配置请求代理到对应的 Agent。

#### 代理流程

1. 从请求中提取 `agent_id`
2. 从数据库查询 Agent 信息（包含 host 和 listen_port 标签）
3. 构造 Agent API URL
4. 使用 reqwest 转发请求到 Agent
5. 返回 Agent 的响应给前端

#### 错误处理

- **404 Not Found**: Agent 不存在
- **502 Bad Gateway**: Agent 离线或无法连接
- **504 Gateway Timeout**: Agent 响应超时
- **500 Internal Server Error**: 其他错误

## 技术细节

### 滚动日志实现

使用 `tracing-appender` 的 `RollingFileAppender`：

```rust
let file_appender = RollingFileAppender::builder()
    .rotation(Rotation::DAILY)           // 按日期滚动
    .filename_prefix("opsbox-server")    // 文件名前缀
    .filename_suffix("log")              // 文件名后缀
    .max_log_files(retention_count)      // 最大保留文件数
    .build(log_dir)?;
```

**滚动策略：**
- 每天午夜自动创建新的日志文件
- 文件命名格式：`opsbox-server.YYYY-MM-DD.log`
- 自动删除超过 `max_log_files` 数量的旧文件

### 异步日志写入

使用 `tracing-appender::non_blocking` 创建异步写入器：

```rust
let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
```

**工作原理：**
1. 创建一个后台线程专门处理日志写入
2. 主线程将日志消息发送到通道（channel）
3. 后台线程从通道接收消息并写入文件
4. `_guard` 在 Drop 时会等待所有日志写入完成

**性能优势：**
- 主线程不会被 I/O 操作阻塞
- 日志写入不影响请求处理性能
- 自动批量写入，提高吞吐量

### 动态日志级别调整

使用 `tracing-subscriber::reload` 实现运行时重载：

```rust
// 初始化时创建 reload layer
let (filter, reload_handle) = reload::Layer::new(filter);

tracing_subscriber::registry()
    .with(filter)
    .with(console_layer)
    .with(file_layer)
    .init();

// 运行时更新日志级别
reload_handle.modify(|filter| {
    *filter = EnvFilter::try_new("debug").unwrap();
})?;
```

**实现原理：**
- `reload::Layer` 包装了 `EnvFilter`
- `reload_handle` 持有对 filter 的引用
- 调用 `modify` 方法可以原子性地更新 filter
- 更新立即生效，无需重启进程

### 日志过滤

使用 `EnvFilter` 进行高效的日志过滤：

```rust
// 支持 RUST_LOG 环境变量
let filter = EnvFilter::try_from_default_env()
    .or_else(|_| EnvFilter::try_new("info"))
    .unwrap();
```

**过滤语法：**
```bash
# 全局级别
RUST_LOG=debug

# 模块级别
RUST_LOG=opsbox_server=debug,logseek=info

# 复杂过滤
RUST_LOG=debug,hyper=warn,sqlx=warn
```

### 结构化日志

使用 tracing 的结构化字段：

```rust
// 基本日志
tracing::info!("Server started");

// 带字段的日志
tracing::info!(
    port = 4000,
    host = "127.0.0.1",
    "Server started"
);

// 带 span 的日志（追踪上下文）
let span = tracing::info_span!("request", method = "GET", path = "/api/search");
let _enter = span.enter();
tracing::info!("Processing request");
```

**输出格式：**
```
2024-01-15T10:30:45.123Z  INFO opsbox_server::server: Server started port=4000 host="127.0.0.1"
```

## 数据流

### 日志写入流程

```
应用代码
    │
    │ tracing::info!(...)
    │
    v
tracing-subscriber
    │
    ├─> EnvFilter (过滤)
    │       │
    │       v
    │   符合级别？
    │       │
    │       ├─> 是
    │       │   │
    │       │   v
    │       │   ┌─────────────────┐
    │       │   │  Console Layer  │
    │       │   │  (stdout)       │
    │       │   └─────────────────┘
    │       │   │
    │       │   v
    │       │   ┌─────────────────┐
    │       │   │  File Layer     │
    │       │   │  (async)        │
    │       │   └─────────────────┘
    │       │           │
    │       │           v
    │       │   ┌─────────────────┐
    │       │   │  Channel        │
    │       │   └─────────────────┘
    │       │           │
    │       │           v
    │       │   ┌─────────────────┐
    │       │   │  后台线程       │
    │       │   └─────────────────┘
    │       │           │
    │       │           v
    │       │   ┌─────────────────┐
    │       │   │  日志文件       │
    │       │   └─────────────────┘
    │       │
    │       └─> 否 (丢弃)
    │
    v
返回
```

### 配置更新流程

```
前端 UI
    │
    │ PUT /api/v1/log/level
    │
    v
Server API Handler
    │
    ├─> 验证参数
    │
    ├─> 更新数据库
    │       │
    │       v
    │   LogConfigRepository
    │       │
    │       v
    │   SQLite Database
    │
    ├─> 更新运行时配置
    │       │
    │       v
    │   ReloadHandle.modify()
    │       │
    │       v
    │   tracing-subscriber
    │       │
    │       v
    │   立即生效
    │
    └─> 返回成功响应
```

## 性能考虑

### 异步写入性能

- **吞吐量**：异步写入可以达到 100,000+ 条/秒
- **延迟**：主线程延迟 < 1μs（仅发送到通道）
- **内存**：默认缓冲区 8KB，可配置

### 过滤性能

- **编译时优化**：未启用的日志级别会被编译器优化掉
- **运行时过滤**：EnvFilter 使用高效的字符串匹配
- **零成本抽象**：tracing 使用宏和内联，几乎无运行时开销

### 磁盘 I/O 优化

- **批量写入**：后台线程自动批量写入，减少系统调用
- **缓冲写入**：使用 BufWriter 减少磁盘 I/O
- **异步刷新**：定期刷新缓冲区，避免数据丢失

## 错误处理

### 错误类型

```rust
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error("日志目录创建失败: {0}")]
    DirectoryCreation(#[from] std::io::Error),
    
    #[error("日志配置无效: {0}")]
    InvalidConfig(String),
    
    #[error("日志级别无效: {0}")]
    InvalidLevel(String),
    
    #[error("数据库错误: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("重载失败: {0}")]
    ReloadFailed(String),
}
```

### 错误处理策略

1. **初始化失败**：记录错误并退出程序
2. **运行时配置更新失败**：返回错误给调用者，保持当前配置
3. **文件写入失败**：tracing-appender 会自动处理，丢弃日志而不阻塞
4. **数据库错误**：返回 500 错误给前端

## 安全考虑

### 路径验证

```rust
// 验证日志目录路径，防止路径遍历攻击
fn validate_log_dir(path: &Path) -> Result<(), LogError> {
    let canonical = path.canonicalize()
        .map_err(|e| LogError::InvalidConfig(format!("无效路径: {}", e)))?;
    
    // 确保路径在允许的目录下
    if !canonical.starts_with("/var/log") && !canonical.starts_with(home_dir()) {
        return Err(LogError::InvalidConfig("路径不在允许的目录下".to_string()));
    }
    
    Ok(())
}
```

### 权限检查

```rust
// 确保日志目录有正确的写入权限
fn check_permissions(path: &Path) -> Result<(), LogError> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    
    // 测试写入权限
    let test_file = path.join(".write_test");
    fs::write(&test_file, b"test")?;
    fs::remove_file(&test_file)?;
    
    Ok(())
}
```

### 敏感信息过滤

```rust
// 避免在日志中记录敏感信息
tracing::info!(
    username = %username,  // 使用 % 格式化，避免记录原始值
    "User logged in"
);

// 不要记录密码、密钥等敏感信息
// ❌ 错误示例
tracing::debug!(password = %password, "Authenticating user");

// ✅ 正确示例
tracing::debug!("Authenticating user");
```

## 测试策略

### 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_conversion() {
        assert_eq!(LogLevel::Info.as_str(), "info");
        assert_eq!(LogLevel::from_str("debug").unwrap(), LogLevel::Debug);
    }

    #[test]
    fn test_log_config_validation() {
        let config = LogConfig {
            level: LogLevel::Info,
            log_dir: PathBuf::from("/tmp/logs"),
            retention_count: 7,
            enable_console: true,
            enable_file: true,
        };
        assert!(config.validate().is_ok());
    }
}
```

### 集成测试

```rust
#[tokio::test]
async fn test_log_api_endpoints() {
    // 启动测试服务器
    let app = create_test_app().await;
    
    // 测试获取配置
    let response = app.get("/api/v1/log/config").await;
    assert_eq!(response.status(), 200);
    
    // 测试更新日志级别
    let response = app.put("/api/v1/log/level")
        .json(&json!({"level": "debug"}))
        .await;
    assert_eq!(response.status(), 200);
}
```

## 监控和调试

### 日志统计

可以添加日志统计功能来监控日志系统：

```rust
use std::sync::atomic::{AtomicU64, Ordering};

static LOG_COUNT: AtomicU64 = AtomicU64::new(0);

// 在 Layer 实现中统计日志数量
impl<S> Layer<S> for StatsLayer {
    fn on_event(&self, event: &Event, ctx: Context<S>) {
        LOG_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

// 定期输出统计信息
tokio::spawn(async {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let count = LOG_COUNT.swap(0, Ordering::Relaxed);
        tracing::info!(count, "Log statistics");
    }
});
```

### 调试工具

```bash
# 查看日志文件大小
du -sh ~/.opsbox/logs/*

# 统计日志级别分布
grep -o "ERROR\|WARN\|INFO\|DEBUG\|TRACE" ~/.opsbox/logs/opsbox-server.log | sort | uniq -c

# 查找特定错误
grep "ERROR" ~/.opsbox/logs/opsbox-server.log | tail -n 20

# 实时监控日志
tail -f ~/.opsbox/logs/opsbox-server.log | grep --color=auto "ERROR\|WARN"
```

## 最佳实践

### 日志级别使用

- **ERROR**: 仅用于真正的错误（需要人工介入）
- **WARN**: 用于警告和潜在问题（可能需要关注）
- **INFO**: 用于关键操作和状态变化（正常运行信息）
- **DEBUG**: 用于详细调试信息（开发和排查问题）
- **TRACE**: 用于最详细的追踪信息（深度调试）

### 结构化字段

```rust
// ✅ 好的实践：使用结构化字段
tracing::info!(
    user_id = %user_id,
    action = "login",
    ip = %ip_addr,
    "User logged in"
);

// ❌ 不好的实践：将所有信息放在消息中
tracing::info!("User {} logged in from {}", user_id, ip_addr);
```

### Span 使用

```rust
// 使用 span 追踪请求上下文
#[tracing::instrument(skip(db))]
async fn handle_request(db: &Database, user_id: i64) -> Result<Response> {
    tracing::info!("Processing request");
    
    let user = db.get_user(user_id).await?;
    tracing::debug!(username = %user.name, "User found");
    
    Ok(Response::success())
}
```

### 错误日志

```rust
// ✅ 好的实践：记录错误上下文
match db.query().await {
    Ok(result) => Ok(result),
    Err(e) => {
        tracing::error!(
            error = %e,
            query = %query,
            "Database query failed"
        );
        Err(e)
    }
}

// ❌ 不好的实践：仅记录错误消息
match db.query().await {
    Ok(result) => Ok(result),
    Err(e) => {
        tracing::error!("Query failed");
        Err(e)
    }
}
```

## 相关文档

- [日志配置指南](../guides/logging-configuration.md) - 用户配置指南
- [Tracing 使用指南](../guides/tracing-usage.md) - 开发者使用指南
- [API 文档](../api/logging-api.md) - REST API 参考
