# Agent HTTP API 规范

**文档版本**: v1.1  
**最后更新**: 2026-03-20

本文档描述 `opsbox-agent` 当前已经实现的 HTTP 接口。

## 概述

`opsbox-agent` 是独立运行的 Rust 服务，用于：

- 在远端主机本地执行日志搜索
- 浏览允许范围内的文件和归档内容
- 返回原始文件内容
- 提供 Agent 自身的日志配置接口
- 向 `opsbox-server` 注册并发送心跳

默认监听端口：`3976`

## 路由一览

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/health` | 健康检查 |
| `GET` | `/api/v1/info` | 获取 Agent 信息 |
| `GET` | `/api/v1/paths` | 列出可用子目录 |
| `POST` | `/api/v1/search` | 流式搜索 |
| `POST` | `/api/v1/cancel/{task_id}` | 取消任务，当前未实现 |
| `GET` | `/api/v1/list_files` | 列目录/归档 |
| `GET` | `/api/v1/file_raw` | 读取原始文件/归档条目 |
| `GET` | `/api/v1/log/config` | 获取 Agent 日志配置 |
| `PUT` | `/api/v1/log/level` | 更新 Agent 日志级别 |
| `PUT` | `/api/v1/log/retention` | 更新 Agent 日志保留数 |

## 1. 健康检查

### `GET /health`

响应：

```text
OK
```

## 2. 获取 Agent 信息

### `GET /api/v1/info`

响应示例：

```json
{
  "id": "agent-host-01",
  "name": "Agent@host-01",
  "version": "0.2.0",
  "hostname": "host-01",
  "tags": [],
  "search_roots": ["/var/log", "/tmp"],
  "last_heartbeat": 1760000000,
  "status": { "type": "Online" }
}
```

字段说明：

- `version` 来自当前 `opsbox-agent` 包版本
- `status` 是 tagged enum，可能值：
  - `{ "type": "Online" }`
  - `{ "type": "Busy", "tasks": 2 }`
  - `{ "type": "Offline" }`

## 3. 列出可用路径

### `GET /api/v1/paths`

返回 `search_roots` 下可见的一级子目录名称数组。

响应示例：

```json
["app", "nginx", "system"]
```

## 4. 流式搜索

### `POST /api/v1/search`

请求体示例：

```json
{
  "task_id": "task-abc123",
  "query": "ERROR timeout",
  "context_lines": 3,
  "path_filter": "*.log",
  "path_includes": ["app/"],
  "path_excludes": ["vendor/", "*.tmp"],
  "target": {
    "type": "dir",
    "path": "app",
    "recursive": true
  },
  "encoding": null
}
```

请求字段：

- `task_id`: 调用方生成的任务 ID
- `query`: 查询字符串
- `context_lines`: 上下文行数
- `path_filter`: 可选，兼容旧的单一路径过滤字段
- `path_includes`: 可选，附加包含过滤
- `path_excludes`: 可选，附加排除过滤
- `target`: 搜索目标
- `encoding`: 可选，强制编码，例如 `gbk`

`target` 当前支持：

```json
{ "type": "dir", "path": "app", "recursive": true }
```

```json
{ "type": "files", "paths": ["app/a.log", "app/b.log"] }
```

```json
{ "type": "archive", "path": "logs.tar.gz", "entry": "inner/app.log" }
```

响应类型：`application/x-ndjson`

事件示例：

```ndjson
{"type":"result","data":{"path":"orl://local/var/log/app.log","keywords":[{"type":"literal","text":"ERROR"}],"chunks":[{"range":[10,12],"lines":[{"no":10,"text":"ERROR timeout"}]}]}}
{"type":"error","data":{"source":"agent-target","message":"Target 解析失败","recoverable":false}}
{"type":"complete","data":{"source":"agent:complete","elapsed_ms":1250}}
```

说明：

- `result` 表示命中文件结果
- `error` 表示错误事件，`recoverable` 表示是否还能继续其他搜索源
- `complete` 表示该请求搜索结束

常见错误：

- `400` 请求体或查询无效
- `404` 访问路径不存在或超出允许范围时会模糊化返回
- `500` 内部错误

## 5. 取消搜索

### `POST /api/v1/cancel/{task_id}`

当前实现状态：

- 路由已存在
- 当前固定返回 `501 Not Implemented`

## 6. 列目录与归档浏览

### `GET /api/v1/list_files`

查询参数：

- `path`: 目标路径
- `entry`: 可选，归档内路径

行为：

- `path` 为空或 `/` 时，返回所有 `search_roots` 作为虚拟根节点
- 指向目录时返回目录项
- 指向归档文件时可返回归档内条目

响应示例：

```json
{
  "items": [
    {
      "name": "app.log",
      "path": "/var/log/app.log",
      "is_dir": false,
      "is_symlink": false,
      "size": 1024,
      "modified": 1760000000,
      "child_count": null,
      "hidden_child_count": null,
      "mime_type": "text/plain"
    }
  ]
}
```

## 7. 原始文件读取

### `GET /api/v1/file_raw`

查询参数：

- `path`: 文件路径
- `entry`: 可选，归档内条目

行为：

- 直接返回文件内容流
- 归档场景下可返回归档中的单个条目

## 8. Agent 日志配置

### `GET /api/v1/log/config`

响应示例：

```json
{
  "level": "info",
  "retention_count": 7,
  "log_dir": "/home/user/.opsbox-agent/logs"
}
```

### `PUT /api/v1/log/level`

请求：

```json
{ "level": "debug" }
```

### `PUT /api/v1/log/retention`

请求：

```json
{ "retention_count": 14 }
```

注意：

- Agent 的 retention 更新当前只影响运行期，不持久化到数据库
- 重启后仍会回到启动参数指定的值

## 与 `opsbox-server` 的关系

Agent 启动后会向 Server 发送：

- `POST /api/v1/agents/register`
- `POST /api/v1/agents/{agent_id}/heartbeat`

`listen_port` 会一起上报，服务端据此生成 `host` / `listen_port` 标签并用于后续代理访问。
