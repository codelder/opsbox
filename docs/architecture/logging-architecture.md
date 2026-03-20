# 日志系统架构

这份文档描述当前代码中的日志链路，而不是理想化设计图。

## 总体结构

日志能力分三层：

1. `opsbox-core/src/logging.rs`
2. Server / Agent 启动入口中的初始化与配置同步
3. HTTP API 对日志级别和保留数量的读写

简化后的实际关系：

```text
main.rs / agent main.rs
  -> opsbox_core::logging::init(LogConfig)
  -> EnvFilter + reload::Layer
  -> console fmt layer
  -> rolling file fmt layer
  -> ReloadHandle

Server:
  ReloadHandle + SQLite log_config + /api/v1/log/*

Agent:
  ReloadHandle + 进程内 current_log_level + /api/v1/log/*

AgentManager:
  /api/v1/agents/{id}/log/*
  -> 读取 host/listen_port 标签
  -> 转发到 Agent
```

## 核心组件

### `opsbox_core::logging`

公共日志模块负责：

- 创建日志目录
- 初始化 `EnvFilter`
- 创建 console/file 两个 `fmt` layer
- 使用 `reload::Layer` 暴露 `ReloadHandle`
- 使用 `RollingFileAppender` 按天滚动日志

关键类型：

- `LogConfig`
- `LogLevel`
- `ReloadHandle`
- `LogError`

### `opsbox_core::logging::repository`

Server 侧日志配置持久化仓库。

当前 `log_config` 表是按 `component` 存储的：

- `server`
- 兼容保留了 `agent` 读写能力，但当前 Agent 主流程并不依赖这张表

Schema 的真实语义：

- `level`：日志级别字符串
- `retention_count`：保留文件数量
- `updated_at`：Unix 时间戳

### `opsbox-server/src/logging.rs`

Server 启动时有两步：

1. 用命令行参数先初始化 logging
2. 数据库 ready 后再执行 `setup_logging_config()`

`setup_logging_config()` 的优先级是：

1. `RUST_LOG` 先被 `EnvFilter` 消费
2. `--log-level`
3. `-v` / `-vv`
4. 数据库 `log_config.level`
5. 默认 `info`

如果使用了 `--log-level` 或 `-v`，Server 会把结果同步写回数据库。

### `opsbox-server/src/log_routes.rs`

系统日志配置接口：

- `GET /api/v1/log/config`
- `PUT /api/v1/log/level`
- `PUT /api/v1/log/retention`

其中：

- `level` 会写库并调用 `ReloadHandle::update_level()`
- `retention_count` 只写库，不热重建 file appender

### `backend/agent/src/routes.rs`

Agent 自身也暴露同样的 `/api/v1/log/*` 接口，但行为不同：

- `level`：更新 `ReloadHandle`，并更新进程内 `current_log_level`
- `retention_count`：只校验并返回成功消息，不持久化

这就是为什么 Agent 接口会明确提示“重启后失效”。

### `backend/agent-manager/src/routes.rs`

AgentManager 提供代理接口：

- `GET /api/v1/agents/{agent_id}/log/config`
- `PUT /api/v1/agents/{agent_id}/log/level`
- `PUT /api/v1/agents/{agent_id}/log/retention`

它会：

1. 查询 Agent 元信息
2. 从标签中提取 `host` 与 `listen_port`
3. 转发到对应 Agent

`opsbox-agent` 当前默认监听端口是 `3976`。代理层保留了缺省回退 `4001` 的兼容逻辑，用于旧记录。

## 输出形态

当前日志初始化固定为：

- 控制台输出开启
- 文件输出开启
- 本地时区 RFC3339 时间戳
- console 层带 ANSI 颜色
- file 层纯文本

Server 文件前缀：

- `opsbox-server`

Agent 文件前缀：

- `opsbox-agent`

## 已知边界

当前实现里最需要注意的边界有三点：

1. `retention_count` 更准确地说是“最大日志文件数”，不是严格意义上的按天保留策略。
2. 修改 `retention_count` 不会热重建 `RollingFileAppender`。
3. Agent 的日志配置不是持久化配置中心，尤其是 retention 更新只在当前进程消息层面体现。

## 相关文档

- `docs/api/logging-api.md`
- `docs/guides/logging-configuration.md`
- `docs/guides/tracing-usage.md`
