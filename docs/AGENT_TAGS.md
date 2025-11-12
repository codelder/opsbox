# Agent Manager 标签功能与管理指南

## 概述

Agent Manager 支持为 Agent 添加 `key=value` 格式的标签，并通过标签完成筛选与调度。标签机制对于管理不同环境（如 production、development）或不同服务类型（如 web、api）的 Agent 非常重要。

## 功能特性

### 1. Agent 标签（key=value 格式）
- 每个 Agent 可以拥有多个标签。
- 标签在 Agent 注册后由管理员维护，Agent 本身保持无状态。
- 标签用于分类和筛选 Agent，支撑环境隔离、服务区分等需求。

### 2. 标签筛选能力
- 支持按单个或多个标签组合筛选。
- 支持筛选在线 Agent。
- 支持获取所有可用标签，以及按标签键获取所有可能的值。

## 标签管理策略

我们采用 **用户手动标签管理** 的模式：

- **Agent 无状态**：Agent 在注册时只报告自身信息，不内置标签逻辑。
- **标签存储在 Agent Manager 中**：管理员通过 API 统一维护标签，保证可审计性与一致性。
- **灵活的人工控制**：管理员可以批量设置、单个添加或移除标签，以适应快速变化的运维需求。

### 标签管理流程

1. **Agent 注册** — 仅提供基础信息，标签字段为空：

```json
{
  "id": "agent-prod-web-01",
  "name": "Production Web Agent 01",
  "hostname": "web-server-prod-01",
  "search_roots": ["/var/log/nginx", "/var/log/app"],
  "tags": []
}
```

2. **管理员通过 API 设置标签** — 支持批量设置、单个添加、单个移除或清空：

```bash
# 批量设置标签
POST /api/v1/agents/agent-prod-web-01/tags
{
  "tags": [
    {"key": "env", "value": "production"},
    {"key": "region", "value": "us-west"},
    {"key": "server", "value": "nginx"},
    {"key": "team", "value": "frontend"}
  ]
}

# 添加单个标签
POST /api/v1/agents/agent-prod-web-01/tags/add
{
  "key": "priority",
  "value": "high"
}

# 移除单个标签
DELETE /api/v1/agents/agent-prod-web-01/tags/remove
{
  "key": "priority",
  "value": "high"
}
```

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

> 提示：实际业务中推荐通过上面的标签管理 API 维护标签，避免 Agent 侧写死配置。

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

### 环境隔离
```bash
# 只使用生产环境的 Agent
GET /api/v1/agents/?tags=env=production&online_only=true
```

### 服务类型筛选
```bash
# 只使用 nginx 服务器的 Agent
GET /api/v1/agents/?tags=server=nginx&online_only=true
```

### 团队维度管理
```bash
# 只使用 platform 团队的 Agent
GET /api/v1/agents/?tags=team=platform&online_only=true
```

### 复合筛选
```bash
# 生产环境的 nginx 服务器
GET /api/v1/agents/?tags=env=production,server=nginx&online_only=true
```

## 编程接口

```rust
use agent_manager::{
    get_online_agent_endpoints,
    get_online_agent_endpoints_by_tags,
    get_all_tags,
    models::AgentTag
};

// 获取所有在线 Agent 端点
let endpoints = get_online_agent_endpoints().await?;

// 按标签获取在线 Agent 端点
let production_endpoints = get_online_agent_endpoints_by_tags(&[
    AgentTag::new("env".to_string(), "production".to_string())
]).await?;

// 获取所有可用标签
let all_tags = get_all_tags().await?;
```

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

- **空标签列表**：返回所有 Agent。
- **单个标签**：返回包含该标签的所有 Agent（key=value 完全匹配）。
- **多个标签**：返回同时包含所有指定标签的 Agent（AND 逻辑）。
- **在线筛选**：结合标签筛选，只返回在线的 Agent。

## 配置示例

### Docker 部署
```dockerfile
# Dockerfile
ENV AGENT_ENV=production
ENV AGENT_SERVICE=logseek
ENV AGENT_REGION=us-west
ENV AGENT_TEAM=platform
```

### Kubernetes 部署
```yaml
# deployment.yaml
env:
- name: AGENT_ENV
  value: "production"
- name: AGENT_SERVICE
  value: "logseek"
- name: AGENT_REGION
  value: "us-west"
- name: AGENT_TEAM
  value: "platform"
```

### 系统服务部署
```bash
# /etc/systemd/system/opsbox-agent.service
[Service]
Environment=AGENT_ENV=production
Environment=AGENT_SERVICE=logseek
Environment=AGENT_REGION=us-west
Environment=AGENT_TEAM=platform
```

## 优势

1. **灵活性**：可按需调整标签，支持环境变量或配置中心动态注入。
2. **一致性**：标签策略集中在 Agent Manager，易于治理与审计。
3. **可维护性**：便于版本控制与团队协作，减少配置分散。
4. **可扩展性**：易于引入新的标签类型，支持与外部系统集成。

## 最佳实践

1. **环境变量优先**：使用环境变量或配置中心注入基础标签。
2. **命名规范**：统一标签键名，例如 `env`、`service`、`team`。
3. **分层管理**：Agent 负责上报基础属性，Manager 负责增强与治理。
4. **监控标签**：定期校验标签准确性，避免陈旧数据。
5. **文档化**：维护标签使用说明和示例，降低沟通成本。

## 示例：LogSeek 中的使用

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

以上内容覆盖了标签的功能特性、管理策略、常用 API 以及运维实践，帮助团队快速掌握并扩展标签体系。
