# Agent 标签管理策略

## 🎯 **用户手动标签管理**

我们采用了 **用户手动标签管理** 的策略，这是最灵活和可控的架构设计：

- **Agent 本身是无状态的**：所有 Agent 功能一致，只是部署在不同的环境中
- **标签用于筛选**：通过标签决定对哪些 Agent 进行操作
- **用户手动设置标签**：管理员根据实际需求手动分配标签
- **Agent Manager 存储标签**：负责存储和管理用户设置的标签

## 📋 **标签管理流程**

### **1. Agent 注册**
Agent 在注册时只提供基础信息，**完全不处理标签**：
```json
{
  "id": "agent-prod-web-01",
  "name": "Production Web Agent 01",
  "hostname": "web-server-prod-01",
  "search_roots": ["/var/log/nginx", "/var/log/app"],
  "tags": [] // 始终为空，Agent 不管理标签
}
```

**Agent 的职责：**
- 提供搜索服务
- 报告基础信息（id、name、hostname、search_roots）
- 保持简单和无状态

### **2. 用户手动设置标签**
管理员通过 API 手动为 Agent 设置标签：

#### **标签设置方式**
- **批量设置**：一次性设置所有标签
- **单个添加**：逐个添加标签
- **单个移除**：移除特定标签
- **清空标签**：清空所有标签

#### **标签设置示例**
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

## 🎯 **使用场景**

### **1. 环境隔离**
```bash
# 只使用生产环境的 Agent
GET /api/v1/agents/?tags=env=production&online_only=true
```

### **2. 服务类型筛选**
```bash
# 只使用 nginx 服务器的 Agent
GET /api/v1/agents/?tags=server=nginx&online_only=true
```

### **3. 团队管理**
```bash
# 只使用 platform 团队的 Agent
GET /api/v1/agents/?tags=team=platform&online_only=true
```

### **4. 复合筛选**
```bash
# 生产环境的 nginx 服务器
GET /api/v1/agents/?tags=env=production,server=nginx&online_only=true
```

## ⚙️ **配置示例**

### **Docker 部署**
```dockerfile
# Dockerfile
ENV AGENT_ENV=production
ENV AGENT_SERVICE=logseek
ENV AGENT_REGION=us-west
ENV AGENT_TEAM=platform
```

### **Kubernetes 部署**
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

### **系统服务部署**
```bash
# /etc/systemd/system/opsbox-agent.service
[Service]
Environment=AGENT_ENV=production
Environment=AGENT_SERVICE=logseek
Environment=AGENT_REGION=us-west
Environment=AGENT_TEAM=platform
```

## 🔧 **优势**

### **1. 灵活性**
- Agent 可以根据实际环境设置标签
- 支持环境变量动态配置
- 无需重新编译即可调整标签

### **2. 智能化**
- Agent Manager 自动推断和增强标签
- 减少手动配置错误
- 保证标签的一致性和完整性

### **3. 可维护性**
- 标签策略集中在 Agent Manager
- 便于统一管理和调整
- 支持标签的版本控制

### **4. 可扩展性**
- 易于添加新的标签类型
- 支持复杂的标签推断逻辑
- 便于集成外部标签系统

## 🚀 **最佳实践**

1. **环境变量优先**：使用环境变量设置基础标签
2. **命名规范**：遵循统一的标签键名规范
3. **分层管理**：Agent 提供基础标签，Manager 进行增强
4. **监控标签**：定期检查标签的准确性和完整性
5. **文档化**：维护标签使用文档和示例

这种混合方案既保证了标签的准确性，又提供了集中管理的便利性，是一个平衡的解决方案。
