# Agent Manager 标签功能

**文档版本**: v1.0  
**最后更新**: 2025-11-10

## 概述

Agent Manager 现在支持为 Agent 添加标签，并通过标签进行筛选。标签采用 `key=value` 格式，这对于管理不同环境（如 production、development）或不同服务类型（如 web、api）的 Agent 非常有用。

## 功能特性

### 1. Agent 标签（key=value 格式）
- 每个 Agent 可以拥有多个标签
- 标签采用 `key=value` 格式，如 `env=production`、`service=web`
- 标签在 Agent 注册时提供，也可通过 API 手动管理
- 标签用于分类和筛选 Agent

### 2. 标签筛选
- 支持按单个或多个标签筛选
- 支持筛选在线 Agent
- 支持获取所有可用标签
- 支持按标签键获取所有可能的值

## API 接口

### 1. 注册带标签的 Agent

```bash
POST /api/v1/agents/register
Content-Type: application/json

{
  "id": "agent-prod-web-01",
  "name": "Production Web Agent 01",
  "version": "1.0.0",
  "hostname": "web-server-01",
  "tags": [
    {"key": "env", "value": "production"},
    {"key": "service", "value": "web"},
    {"key": "server", "value": "nginx"}
  ],
  "search_roots": ["/var/log/nginx"],
  "last_heartbeat": 0,
  "status": {"type": "Online"}
}
```

### 2. 获取所有 Agent（支持标签筛选）

```bash
# 获取所有 Agent
GET /api/v1/agents/

# 按标签筛选（多个标签用逗号分隔，key=value 格式）
GET /api/v1/agents/?tags=env=production,service=web

# 只获取在线的 Agent
GET /api/v1/agents/?online_only=true

# 按标签筛选在线 Agent
GET /api/v1/agents/?tags=env=production&online_only=true
```

### 3. 获取所有可用标签

```bash
GET /api/v1/agents/tags
```

响应示例：
```json
{
  "tags": ["env=development", "env=production", "service=api", "service=web", "server=nginx"],
  "total": 5
}
```

## 使用场景

### 1. 环境分类
```json
{
  "tags": [
    {"key": "env", "value": "production"}
  ]
}
```

### 2. 服务类型分类
```json
{
  "tags": [
    {"key": "service", "value": "web"},
    {"key": "server", "value": "nginx"}
  ]
}
```

### 3. 复合标签
```json
{
  "tags": [
    {"key": "env", "value": "production"},
    {"key": "service", "value": "web"},
    {"key": "server", "value": "nginx"},
    {"key": "region", "value": "us-west"}
  ]
}
```

## 编程接口

### 1. 获取按标签筛选的在线 Agent 端点

```rust
use agent_manager::{get_online_agent_endpoints, get_online_agent_endpoints_by_tags, get_all_tags, models::AgentTag};

// 获取所有在线 Agent 端点
let endpoints = get_online_agent_endpoints().await?;

// 按标签获取在线 Agent 端点
let production_endpoints = get_online_agent_endpoints_by_tags(&[
  AgentTag::new("env".to_string(), "production".to_string())
]).await?;

// 获取所有可用标签
let all_tags = get_all_tags().await?;
```

### 2. AgentManager 直接使用

```rust
use agent_manager::manager::AgentManager;
use agent_manager::models::AgentTag;

let manager = AgentManager::new();

// 按标签筛选 Agent
let production_agents = manager.list_agents_by_tags(&[
  AgentTag::new("env".to_string(), "production".to_string())
]).await;

// 按标签筛选在线 Agent
let online_production_agents = manager.list_online_agents_by_tags(&[
  AgentTag::new("env".to_string(), "production".to_string())
]).await;

// 获取所有标签
let all_tags = manager.get_all_tags().await;

// 获取所有标签键
let tag_keys = manager.get_all_tag_keys().await;

// 获取指定键的所有标签值
let env_values = manager.get_tag_values_by_key("env").await;
```

## 标签筛选逻辑

- **空标签列表**：返回所有 Agent
- **单个标签**：返回包含该标签的所有 Agent（key=value 完全匹配）
- **多个标签**：返回同时包含所有指定标签的 Agent（AND 逻辑）
- **在线筛选**：结合标签筛选，只返回在线的 Agent

## 示例：LogSeek 中的使用

在 LogSeek 中，可以根据标签选择不同的 Agent 进行搜索：

```rust
use agent_manager::{get_online_agent_endpoints_by_tags, models::AgentTag};

// 优先使用生产环境的 Agent
let production_endpoints = get_online_agent_endpoints_by_tags(&[
  AgentTag::new("env".to_string(), "production".to_string())
]).await?;

if !production_endpoints.is_empty() {
    // 使用生产环境 Agent 进行搜索
    search_with_agents(&production_endpoints).await;
} else {
    // 回退到所有在线 Agent
    let all_endpoints = get_online_agent_endpoints().await?;
    search_with_agents(&all_endpoints).await;
}
```

这样可以根据业务需求灵活地选择不同环境的 Agent 进行日志搜索。
