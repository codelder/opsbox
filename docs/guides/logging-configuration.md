# 日志配置指南

这份文档只说明当前代码里的真实日志行为。

## 当前能力

OpsBox 的 Server 和 Agent 都使用 `opsbox_core::logging` 初始化 `tracing`：

- 控制台输出 + 文件输出同时开启
- 文件按天滚动
- 支持运行时调整日志级别
- Server 日志配置会持久化到 SQLite
- Agent 日志级别可热更新，但不持久化

## 启动参数

### Server

常用参数：

```bash
opsbox-server \
  --host 0.0.0.0 \
  --port 4000 \
  --log-dir ~/.opsbox/logs \
  --log-retention 7
```

默认值：

- 监听地址：`0.0.0.0:4000`
- 日志目录：`~/.opsbox/logs`
- 日志保留：`7`
- 默认日志级别：`info`

日志级别优先级：

1. `RUST_LOG`
2. `--log-level`
3. `-v` / `-vv`
4. Server 数据库里的 `log_config.level`
5. 默认 `info`

说明：

- `--log-level` 和 `-v` 会在启动时同步写回 `log_config`
- 如果显式设置了 `RUST_LOG`，`tracing_subscriber::EnvFilter` 会优先采用它

### Agent

常用参数：

```bash
opsbox-agent \
  --server-endpoint http://localhost:4000 \
  --listen-port 3976 \
  --log-dir ~/.opsbox-agent/logs \
  --log-retention 7
```

默认值：

- 监听端口：`3976`
- 日志目录：`~/.opsbox-agent/logs`
- 日志保留：`7`
- 默认日志级别：`info`

Agent 不读取 Server 侧的 `log_config` 表。它的热更新只作用于当前进程。

## 动态调整

### Server

可通过以下接口修改：

- `GET /api/v1/log/config`
- `PUT /api/v1/log/level`
- `PUT /api/v1/log/retention`

其中：

- `level` 更新后立即生效
- `retention_count` 会写入数据库，但当前实现不会热重建 `RollingFileAppender`

### Agent

可通过以下代理接口修改：

- `GET /api/v1/agents/{agent_id}/log/config`
- `PUT /api/v1/agents/{agent_id}/log/level`
- `PUT /api/v1/agents/{agent_id}/log/retention`

其中：

- `level` 更新后立即生效
- `retention_count` 只返回成功消息，不会持久化，重启后失效

## 文件滚动与保留

底层使用 `tracing_appender::rolling::RollingFileAppender`：

- 滚动周期：`DAILY`
- 文件前缀：Server 为 `opsbox-server`，Agent 为 `opsbox-agent`
- `retention_count` 传给 `max_log_files`

因此，当前实现更准确的说法是“保留日志文件数量”，不是强语义上的“按天保留策略”。由于每天滚动一次，两者通常接近。

## 常用排障级别

推荐从低到高逐级放大：

- `info`：只看系统级流程和结果
- `debug`：看模块细节，适合排障
- `trace`：看最细粒度执行细节，噪音很大

LogSeek 链路常用组合：

```bash
RUST_LOG=info,logseek=debug opsbox-server
```

深度排障时再打开更细的 target：

```bash
RUST_LOG=info,logseek=debug,logseek::service::entry_stream=trace opsbox-server
```

## 相关文档

- `docs/api/logging-api.md`
- `docs/architecture/logging-architecture.md`
- `docs/guides/tracing-usage.md`
