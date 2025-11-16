# 日志配置 API 文档

本文档描述 OpsBox 日志配置的 REST API 接口。

## 概述

OpsBox 提供 REST API 用于动态管理日志配置，包括：

- 查询当前日志配置
- 更新日志级别（立即生效）
- 更新日志保留策略（下次滚动时生效）

## 基础信息

**Base URL**: `http://localhost:4000`

**认证**: 当前版本不需要认证（未来版本可能添加）

**Content-Type**: `application/json`

## Server 日志配置 API

### 获取 Server 日志配置

获取当前 Server 的日志配置信息。

**端点**: `GET /api/v1/log/config`

**请求示例**:
```bash
curl http://localhost:4000/api/v1/log/config
```

**响应示例**:
```json
{
  "level": "info",
  "retention_count": 7,
  "log_dir": "/Users/username/.opsbox/logs"
}
```

**响应字段**:
| 字段 | 类型 | 说明 |
|------|------|------|
| `level` | string | 当前日志级别：`error`, `warn`, `info`, `debug`, `trace` |
| `retention_count` | number | 日志保留天数 |
| `log_dir` | string | 日志文件目录路径 |

**状态码**:
- `200 OK`: 成功
- `500 Internal Server Error`: 服务器内部错误

---

### 更新 Server 日志级别

动态更新 Server 的日志级别，立即生效，无需重启。

**端点**: `PUT /api/v1/log/level`

**请求体**:
```json
{
  "level": "debug"
}
```

**请求字段**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `level` | string | 是 | 日志级别：`error`, `warn`, `info`, `debug`, `trace` |

**请求示例**:
```bash
curl -X PUT http://localhost:4000/api/v1/log/level \
  -H "Content-Type: application/json" \
  -d '{"level": "debug"}'
```

**响应示例**:
```json
{
  "message": "Log level updated to debug"
}
```

**状态码**:
- `200 OK`: 成功
- `400 Bad Request`: 参数无效
- `500 Internal Server Error`: 服务器内部错误

**错误响应示例**:
```json
{
  "error": "Invalid log level: invalid",
  "detail": "Valid levels are: error, warn, info, debug, trace"
}
```

---

### 更新 Server 日志保留策略

更新 Server 的日志保留天数，在下次日志滚动时生效（通常是第二天午夜）。

**端点**: `PUT /api/v1/log/retention`

**请求体**:
```json
{
  "retention_count": 30
}
```

**请求字段**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `retention_count` | number | 是 | 日志保留天数（1-365） |

**请求示例**:
```bash
curl -X PUT http://localhost:4000/api/v1/log/retention \
  -H "Content-Type: application/json" \
  -d '{"retention_count": 30}'
```

**响应示例**:
```json
{
  "message": "Log retention updated to 30 days"
}
```

**状态码**:
- `200 OK`: 成功
- `400 Bad Request`: 参数无效
- `500 Internal Server Error`: 服务器内部错误

**错误响应示例**:
```json
{
  "error": "Invalid retention count",
  "detail": "Retention count must be between 1 and 365"
}
```

---

## Agent 日志配置 API（通过 Server 代理）

### 获取 Agent 日志配置

通过 Server 代理获取指定 Agent 的日志配置。

**端点**: `GET /api/v1/agents/{agent_id}/log/config`

**路径参数**:
| 参数 | 类型 | 说明 |
|------|------|------|
| `agent_id` | string | Agent ID |

**请求示例**:
```bash
curl http://localhost:4000/api/v1/agents/agent-123/log/config
```

**响应示例**:
```json
{
  "level": "info",
  "retention_count": 7,
  "log_dir": "/Users/username/.opsbox-agent/logs"
}
```

**状态码**:
- `200 OK`: 成功
- `404 Not Found`: Agent 不存在
- `502 Bad Gateway`: Agent 离线或无法连接
- `504 Gateway Timeout`: Agent 响应超时
- `500 Internal Server Error`: 服务器内部错误

**错误响应示例**:
```json
{
  "error": "Agent not found",
  "detail": "Agent with ID 'agent-123' does not exist"
}
```

```json
{
  "error": "Agent offline",
  "detail": "Unable to connect to agent at 192.168.1.100:4001"
}
```

---

### 更新 Agent 日志级别

通过 Server 代理更新指定 Agent 的日志级别。

**端点**: `PUT /api/v1/agents/{agent_id}/log/level`

**路径参数**:
| 参数 | 类型 | 说明 |
|------|------|------|
| `agent_id` | string | Agent ID |

**请求体**:
```json
{
  "level": "debug"
}
```

**请求示例**:
```bash
curl -X PUT http://localhost:4000/api/v1/agents/agent-123/log/level \
  -H "Content-Type: application/json" \
  -d '{"level": "debug"}'
```

**响应示例**:
```json
{
  "message": "Log level updated to debug"
}
```

**状态码**:
- `200 OK`: 成功
- `400 Bad Request`: 参数无效
- `404 Not Found`: Agent 不存在
- `502 Bad Gateway`: Agent 离线或无法连接
- `504 Gateway Timeout`: Agent 响应超时
- `500 Internal Server Error`: 服务器内部错误

---

### 更新 Agent 日志保留策略

通过 Server 代理更新指定 Agent 的日志保留天数。

**端点**: `PUT /api/v1/agents/{agent_id}/log/retention`

**路径参数**:
| 参数 | 类型 | 说明 |
|------|------|------|
| `agent_id` | string | Agent ID |

**请求体**:
```json
{
  "retention_count": 14
}
```

**请求示例**:
```bash
curl -X PUT http://localhost:4000/api/v1/agents/agent-123/log/retention \
  -H "Content-Type: application/json" \
  -d '{"retention_count": 14}'
```

**响应示例**:
```json
{
  "message": "Log retention updated to 14 days"
}
```

**状态码**:
- `200 OK`: 成功
- `400 Bad Request`: 参数无效
- `404 Not Found`: Agent 不存在
- `502 Bad Gateway`: Agent 离线或无法连接
- `504 Gateway Timeout`: Agent 响应超时
- `500 Internal Server Error`: 服务器内部错误

---

## Agent 本地 API

Agent 自身也提供日志配置 API，可以直接访问（如果网络可达）。

### 获取 Agent 本地日志配置

**端点**: `GET /api/v1/log/config`

**Base URL**: `http://<agent-host>:<agent-port>` (默认端口: 4001)

**请求示例**:
```bash
curl http://192.168.1.100:4001/api/v1/log/config
```

**响应格式**: 与 Server API 相同

---

### 更新 Agent 本地日志级别

**端点**: `PUT /api/v1/log/level`

**请求示例**:
```bash
curl -X PUT http://192.168.1.100:4001/api/v1/log/level \
  -H "Content-Type: application/json" \
  -d '{"level": "debug"}'
```

**响应格式**: 与 Server API 相同

---

### 更新 Agent 本地日志保留策略

**端点**: `PUT /api/v1/log/retention`

**请求示例**:
```bash
curl -X PUT http://192.168.1.100:4001/api/v1/log/retention \
  -H "Content-Type: application/json" \
  -d '{"retention_count": 14}'
```

**响应格式**: 与 Server API 相同

---

## 数据类型

### LogLevel

日志级别枚举值：

| 值 | 说明 |
|----|------|
| `error` | 错误级别 - 仅记录错误信息 |
| `warn` | 警告级别 - 记录警告和错误 |
| `info` | 信息级别 - 记录信息、警告和错误（默认） |
| `debug` | 调试级别 - 记录调试信息及以上 |
| `trace` | 追踪级别 - 记录所有日志 |

### LogConfigResponse

日志配置响应对象：

```typescript
interface LogConfigResponse {
  level: "error" | "warn" | "info" | "debug" | "trace";
  retention_count: number;  // 1-365
  log_dir: string;
}
```

### UpdateLogLevelRequest

更新日志级别请求对象：

```typescript
interface UpdateLogLevelRequest {
  level: "error" | "warn" | "info" | "debug" | "trace";
}
```

### UpdateRetentionRequest

更新日志保留策略请求对象：

```typescript
interface UpdateRetentionRequest {
  retention_count: number;  // 1-365
}
```

### SuccessResponse

成功响应对象：

```typescript
interface SuccessResponse {
  message: string;
}
```

### ErrorResponse

错误响应对象：

```typescript
interface ErrorResponse {
  error: string;
  detail?: string;
}
```

---

## 错误处理

### 错误响应格式

所有错误响应都遵循统一的格式：

```json
{
  "error": "错误类型",
  "detail": "详细错误信息（可选）"
}
```

### 常见错误

#### 400 Bad Request

参数验证失败：

```json
{
  "error": "Invalid log level: invalid",
  "detail": "Valid levels are: error, warn, info, debug, trace"
}
```

```json
{
  "error": "Invalid retention count",
  "detail": "Retention count must be between 1 and 365"
}
```

#### 404 Not Found

Agent 不存在：

```json
{
  "error": "Agent not found",
  "detail": "Agent with ID 'agent-123' does not exist"
}
```

#### 502 Bad Gateway

Agent 离线或无法连接：

```json
{
  "error": "Agent offline",
  "detail": "Unable to connect to agent at 192.168.1.100:4001"
}
```

```json
{
  "error": "Agent error",
  "detail": "Agent returned status code 500"
}
```

#### 504 Gateway Timeout

Agent 响应超时：

```json
{
  "error": "Gateway timeout",
  "detail": "Agent did not respond within 10 seconds"
}
```

#### 500 Internal Server Error

服务器内部错误：

```json
{
  "error": "Internal server error",
  "detail": "Database connection failed"
}
```

---

## 使用示例

### JavaScript/TypeScript

```typescript
// 获取 Server 日志配置
async function getServerLogConfig() {
  const response = await fetch('http://localhost:4000/api/v1/log/config');
  if (!response.ok) {
    throw new Error(`HTTP error! status: ${response.status}`);
  }
  return await response.json();
}

// 更新 Server 日志级别
async function updateServerLogLevel(level: string) {
  const response = await fetch('http://localhost:4000/api/v1/log/level', {
    method: 'PUT',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ level }),
  });
  
  if (!response.ok) {
    const error = await response.json();
    throw new Error(error.error);
  }
  
  return await response.json();
}

// 更新 Agent 日志级别
async function updateAgentLogLevel(agentId: string, level: string) {
  const response = await fetch(
    `http://localhost:4000/api/v1/agents/${agentId}/log/level`,
    {
      method: 'PUT',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ level }),
    }
  );
  
  if (!response.ok) {
    const error = await response.json();
    throw new Error(error.error);
  }
  
  return await response.json();
}
```

### Python

```python
import requests

# 获取 Server 日志配置
def get_server_log_config():
    response = requests.get('http://localhost:4000/api/v1/log/config')
    response.raise_for_status()
    return response.json()

# 更新 Server 日志级别
def update_server_log_level(level):
    response = requests.put(
        'http://localhost:4000/api/v1/log/level',
        json={'level': level}
    )
    response.raise_for_status()
    return response.json()

# 更新 Agent 日志级别
def update_agent_log_level(agent_id, level):
    response = requests.put(
        f'http://localhost:4000/api/v1/agents/{agent_id}/log/level',
        json={'level': level}
    )
    response.raise_for_status()
    return response.json()
```

### Rust

```rust
use reqwest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct LogConfigResponse {
    level: String,
    retention_count: usize,
    log_dir: String,
}

#[derive(Debug, Serialize)]
struct UpdateLogLevelRequest {
    level: String,
}

// 获取 Server 日志配置
async fn get_server_log_config() -> Result<LogConfigResponse, reqwest::Error> {
    let client = reqwest::Client::new();
    let response = client
        .get("http://localhost:4000/api/v1/log/config")
        .send()
        .await?;
    
    response.json().await
}

// 更新 Server 日志级别
async fn update_server_log_level(level: &str) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();
    let request = UpdateLogLevelRequest {
        level: level.to_string(),
    };
    
    client
        .put("http://localhost:4000/api/v1/log/level")
        .json(&request)
        .send()
        .await?;
    
    Ok(())
}
```

---

## 速率限制

当前版本没有速率限制。未来版本可能会添加速率限制以防止滥用。

---

## 版本控制

当前 API 版本：`v1`

API 版本包含在 URL 路径中：`/api/v1/...`

未来的 API 变更将使用新的版本号（如 `v2`），旧版本将继续支持一段时间。

---

## 相关文档

- [日志配置指南](../guides/logging-configuration.md) - 用户配置指南
- [日志系统架构](../architecture/logging-architecture.md) - 架构设计文档
- [Tracing 使用指南](../guides/tracing-usage.md) - 开发者使用指南
