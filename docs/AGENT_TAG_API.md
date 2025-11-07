# Agent 标签管理 API

## 🎯 **API 概览**

Agent Manager 提供了完整的标签管理 API，支持用户手动设置、查询和管理 Agent 标签。

## 📋 **API 端点**

### **1. 标签查询**

#### **获取所有标签**
```bash
GET /api/v1/agents/tags
```

**响应示例：**
```json
{
  "tags": [
    "env=production",
    "env=development", 
    "region=us-west",
    "region=eu-central",
    "server=nginx",
    "server=apache",
    "team=frontend",
    "team=backend"
  ],
  "total": 8
}
```

#### **获取 Agent 的标签**
```bash
GET /api/v1/agents/{agent_id}/tags
```

**响应示例：**
```json
[
  {"key": "env", "value": "production"},
  {"key": "region", "value": "us-west"},
  {"key": "server", "value": "nginx"},
  {"key": "team", "value": "frontend"}
]
```

### **2. 标签设置**

#### **批量设置标签**
```bash
POST /api/v1/agents/{agent_id}/tags
Content-Type: application/json

{
  "tags": [
    {"key": "env", "value": "production"},
    {"key": "region", "value": "us-west"},
    {"key": "server", "value": "nginx"},
    {"key": "team", "value": "frontend"}
  ]
}
```

**响应示例：**
```json
{
  "message": "标签设置成功"
}
```

#### **添加单个标签**
```bash
POST /api/v1/agents/{agent_id}/tags/add
Content-Type: application/json

{
  "key": "priority",
  "value": "high"
}
```

**响应示例：**
```json
{
  "message": "标签添加成功"
}
```

#### **移除单个标签**
```bash
DELETE /api/v1/agents/{agent_id}/tags/remove
Content-Type: application/json

{
  "key": "priority",
  "value": "high"
}
```

**响应示例：**
```json
{
  "message": "标签移除成功"
}
```

#### **清空所有标签**
```bash
DELETE /api/v1/agents/{agent_id}/tags/clear
```

**响应示例：**
```json
{
  "message": "标签清空成功"
}
```

### **3. Agent 查询（带标签筛选）**

#### **按标签筛选 Agent**
```bash
GET /api/v1/agents/?tags=env=production,team=frontend&online_only=true
```

**查询参数：**
- `tags`: 标签筛选（多个标签用逗号分隔，key=value 格式）
- `online_only`: 是否只返回在线 Agent（true/false）

**响应示例：**
```json
{
  "agents": [
    {
      "id": "agent-prod-web-01",
      "name": "Production Web Agent 01",
      "hostname": "web-server-prod-01",
      "tags": [
        {"key": "env", "value": "production"},
        {"key": "team", "value": "frontend"}
      ],
      "search_roots": ["/var/log/nginx"],
      "status": {"type": "Online"},
      "last_heartbeat": 1703123456
    }
  ],
  "total": 1
}
```

## 🚀 **使用示例**

### **场景 1：为生产环境的前端 Agent 设置标签**

```bash
# 1. 查看所有 Agent
curl -X GET "http://localhost:4000/api/v1/agents/"

# 2. 为特定 Agent 设置标签
curl -X POST "http://localhost:4000/api/v1/agents/agent-prod-web-01/tags" \
  -H "Content-Type: application/json" \
  -d '{
    "tags": [
      {"key": "env", "value": "production"},
      {"key": "team", "value": "frontend"},
      {"key": "server", "value": "nginx"},
      {"key": "priority", "value": "high"}
    ]
  }'

# 3. 验证标签设置
curl -X GET "http://localhost:4000/api/v1/agents/agent-prod-web-01/tags"
```

### **场景 2：按标签筛选 Agent**

```bash
# 只获取生产环境的前端 Agent
curl -X GET "http://localhost:4000/api/v1/agents/?tags=env=production,team=frontend&online_only=true"

# 只获取 nginx 服务器的 Agent
curl -X GET "http://localhost:4000/api/v1/agents/?tags=server=nginx&online_only=true"

# 获取所有高优先级的 Agent
curl -X GET "http://localhost:4000/api/v1/agents/?tags=priority=high&online_only=true"
```

### **场景 3：动态标签管理**

```bash
# 添加新标签
curl -X POST "http://localhost:4000/api/v1/agents/agent-prod-web-01/tags/add" \
  -H "Content-Type: application/json" \
  -d '{"key": "maintenance", "value": "scheduled"}'

# 移除标签
curl -X DELETE "http://localhost:4000/api/v1/agents/agent-prod-web-01/tags/remove" \
  -H "Content-Type: application/json" \
  -d '{"key": "maintenance", "value": "scheduled"}'

# 清空所有标签
curl -X DELETE "http://localhost:4000/api/v1/agents/agent-prod-web-01/tags/clear"
```

## 🎯 **最佳实践**

### **1. 标签命名规范**
- 使用小写字母和连字符：`env`, `team`, `server-type`
- 保持标签键名简洁明了
- 使用一致的标签值格式

### **2. 标签分类建议**
- **环境标签**：`env=production`, `env=development`, `env=testing`
- **团队标签**：`team=frontend`, `team=backend`, `team=devops`
- **服务标签**：`server=nginx`, `server=apache`, `server=tomcat`
- **区域标签**：`region=us-west`, `region=eu-central`, `region=asia-pacific`
- **优先级标签**：`priority=high`, `priority=medium`, `priority=low`

### **3. 标签管理策略**
- 定期清理无用的标签
- 使用标签进行环境隔离
- 通过标签实现自动化运维
- 建立标签变更审批流程

## ⚠️ **注意事项**

1. **标签格式**：必须使用 `key=value` 格式
2. **Agent 存在性**：设置标签前确保 Agent 已注册
3. **标签唯一性**：同一个 Agent 不能有重复的 key-value 对
4. **权限控制**：建议在生产环境中添加适当的权限控制
5. **标签数量**：避免为单个 Agent 设置过多标签，建议不超过 10 个
