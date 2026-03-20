# Tracing 使用指南

OpsBox 当前使用 `tracing` 做结构化日志，但并没有接入 OpenTelemetry 或完整分布式 tracing 栈。这份文档只覆盖项目里实际采用的写法。

## 当前约定

项目里最常见的模式是：

- 用 `info!` / `warn!` / `error!` 记录系统流程和异常
- 用 `debug!` / `trace!` 记录排障细节
- 优先写结构化字段，而不是只拼长字符串
- 尽量在边界层打日志：HTTP 路由、模块初始化、搜索执行、外部 IO

当前代码中几乎没有系统性使用 `#[instrument]`；不要把通用 tracing 教程里的 span-heavy 风格直接当成项目既有规范。

## 推荐写法

### 结构化字段优先

```rust
tracing::info!(
    agent_id = %agent_id,
    level = %level,
    "日志级别已更新"
);
```

比起：

```rust
tracing::info!("agent {} log level changed to {}", agent_id, level);
```

前者更容易过滤和检索。

### 错误日志带上下文

```rust
tracing::error!(
    agent_id = %agent_id,
    error = %e,
    "代理请求失败"
);
```

### 大对象用 `?`

```rust
tracing::debug!(config = ?config, "模块配置已加载");
```

### 路径显示用 `%path.display()`

```rust
tracing::info!(path = %path.display(), "开始处理文件");
```

## 级别选择

- `error`：请求失败、外部依赖失败、数据损坏
- `warn`：可恢复异常、兼容分支、降级路径
- `info`：启动、关闭、配置变更、关键请求开始/结束
- `debug`：排障所需的中间状态
- `trace`：非常细的执行细节，只在深度排障时打开

一个简单判断：

- 正常生产环境长期保留的信息，用 `info`
- 如果高频打印会明显淹没日志，用 `debug` 或 `trace`

## 现有代码风格

当前代码里常见的日志位置：

- `backend/opsbox-server/src/main.rs`：启动、模块发现、数据库初始化
- `backend/opsbox-server/src/server.rs`：HTTP 请求/响应与优雅关闭
- `backend/logseek/src/service/search_executor.rs`：搜索来源规划和执行进度
- `backend/agent-manager/src/routes.rs`：Agent 注册、标签补全、代理转发
- `backend/agent/src/routes.rs`：Agent 搜索请求和日志 API

## 什么时候用 span

目前只在 HTTP 层做了轻量 span 包装。除非你要表达一个跨多个异步步骤的共享上下文，否则直接写结构化事件日志通常更符合这个仓库现状。

如果确实需要 span，优先在这些场景使用：

- 一次请求跨多个内部步骤
- 一次批处理要串起多条子日志
- 你希望多个子模块共享同一组上下文字段

## 不建议的写法

- 为每个小函数都加 `#[instrument]`
- 把敏感内容整段打进日志
- 在热路径里无差别打印大型对象
- 用纯字符串日志替代可结构化字段

## 相关文档

- `docs/guides/logging-configuration.md`
- `docs/architecture/logging-architecture.md`
