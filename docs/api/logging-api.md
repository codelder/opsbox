# 日志配置 API

本文档只描述当前已经实现的日志配置接口。

## 基本信息

- Server 默认监听：`http://0.0.0.0:4000`
- Content-Type：`application/json`
- 认证：当前未实现认证
- 日志级别枚举：`error`、`warn`、`info`、`debug`、`trace`

`retention_count` 这个字段名沿用了早期设计。当前实现里它对应每天滚动日志时保留的最大日志文件数，通常可近似理解为保留天数。

## Server API

### `GET /api/v1/log/config`

读取当前 Server 日志配置。

响应示例：

```json
{
  "level": "info",
  "retention_count": 7,
  "log_dir": "/Users/username/.opsbox/logs"
}
```

字段：

- `level`：当前日志级别
- `retention_count`：保留文件数量
- `log_dir`：日志目录

### `PUT /api/v1/log/level`

动态更新 Server 日志级别，立即生效，同时会持久化到 `log_config` 表。

请求体：

```json
{
  "level": "debug"
}
```

响应示例：

```json
{
  "success": true,
  "message": "日志级别已更新为: debug"
}
```

无效级别会返回 `400 Bad Request`。

### `PUT /api/v1/log/retention`

更新 Server 日志保留数量。当前实现会把值写入数据库，但对已经初始化好的 `RollingFileAppender` 不做热重建，所以新值会在下次进程启动后稳定生效。

请求体：

```json
{
  "retention_count": 30
}
```

响应示例：

```json
{
  "success": true,
  "message": "日志保留数量已更新为: 30 天"
}
```

取值范围是 `1..=365`，否则返回 `400 Bad Request`。

## Agent 代理 API

这些接口由 Server 暴露，再转发到具体 Agent 的 `/api/v1/log/*` 接口。

### `GET /api/v1/agents/{agent_id}/log/config`

读取指定 Agent 的日志配置。

响应结构与 Server 相同：

```json
{
  "level": "info",
  "retention_count": 7,
  "log_dir": "/Users/username/.opsbox-agent/logs"
}
```

### `PUT /api/v1/agents/{agent_id}/log/level`

动态更新指定 Agent 的日志级别。

请求体：

```json
{
  "level": "debug"
}
```

响应示例：

```json
{
  "success": true,
  "message": "日志级别已更新为: debug"
}
```

### `PUT /api/v1/agents/{agent_id}/log/retention`

更新指定 Agent 的日志保留数量。

请求体：

```json
{
  "retention_count": 14
}
```

响应示例：

```json
{
  "success": true,
  "message": "日志保留数量已更新为: 14 天（重启后失效）"
}
```

注意：

- Agent 当前不会把日志配置写回数据库
- Agent 的 `retention_count` 更新不会热重建文件 appender
- Agent 重启后仍会回到启动参数里的 `--log-retention`

## Agent 本地 API

如果网络可达，也可以直接访问 Agent 自身接口：

- `GET /api/v1/log/config`
- `PUT /api/v1/log/level`
- `PUT /api/v1/log/retention`

当前 `opsbox-agent` 默认监听端口是 `3976`。

## 路由与转发边界

Server 侧系统日志接口位于：

- `/api/v1/log/config`
- `/api/v1/log/level`
- `/api/v1/log/retention`

Agent 代理接口位于：

- `/api/v1/agents/{agent_id}/log/config`
- `/api/v1/agents/{agent_id}/log/level`
- `/api/v1/agents/{agent_id}/log/retention`

AgentManager 会根据 Agent 标签里的 `host` 和 `listen_port` 生成转发地址。正常情况下 `listen_port` 来自 Agent 注册时上报的监听端口；当前 `opsbox-agent` 默认监听端口是 `3976`。为兼容旧记录，如果标签缺失，代理层仍会回退到 `4001`。
