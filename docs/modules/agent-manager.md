# Agent Manager 模块

**文档版本**: v1.1  
**最后更新**: 2026-03-20

`agent-manager` 是 OpsBox 的独立后端模块，前缀为 `/api/v1/agents`。

## 模块职责

- 接收 Agent 注册请求
- 维护 Agent 列表与心跳状态
- 管理 Agent 标签
- 代理访问 Agent 的日志配置接口
- 为 `explorer` / `logseek` 等模块提供 Agent 元数据

## 当前项目结构

```text
backend/agent-manager/
├── src/lib.rs
├── src/manager.rs
├── src/models.rs
├── src/repository.rs
└── src/routes.rs
```

关键实现点：

- `lib.rs` 中实现 `opsbox_core::Module`
- `init_schema()` 初始化数据库表并创建全局 `AgentManager`
- 路由层直接使用全局 `AgentManager`，避免运行时阻塞初始化

## API 一览

完整前缀：`/api/v1/agents`

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `POST` | `/register` | Agent 注册 |
| `GET` | `/` | 列出 Agent |
| `GET` | `/tags` | 列出所有标签 |
| `GET` | `/{agent_id}` | 获取 Agent 详情 |
| `DELETE` | `/{agent_id}` | 注销 Agent |
| `POST` | `/{agent_id}/heartbeat` | 更新心跳 |
| `GET` | `/{agent_id}/tags` | 获取单个 Agent 标签 |
| `POST` | `/{agent_id}/tags` | 覆盖设置标签 |
| `POST` | `/{agent_id}/tags/add` | 添加标签 |
| `DELETE` | `/{agent_id}/tags/remove` | 删除单个标签 |
| `DELETE` | `/{agent_id}/tags/clear` | 清空标签 |
| `GET` | `/{agent_id}/log/config` | 代理读取 Agent 日志配置 |
| `PUT` | `/{agent_id}/log/level` | 代理更新 Agent 日志级别 |
| `PUT` | `/{agent_id}/log/retention` | 代理更新 Agent 日志保留数 |

## 注册流程

当 `opsbox-agent` 启动时：

1. Agent 向 `/api/v1/agents/register` 提交 `AgentInfo` 和 `listen_port`
2. `agent-manager` 保存 Agent 基础信息
3. 服务端根据请求来源地址和上报端口补充两个标签：
   - `host`
   - `listen_port`
4. 之后日志代理与其他模块都可通过这些标签访问 Agent

## 标签模型

标签结构：

```json
{
  "key": "env",
  "value": "prod"
}
```

查询支持：

- `GET /api/v1/agents?tags=env=prod,region=cn`
- `GET /api/v1/agents?online_only=true`
- 两者可组合使用

## 在线状态

Agent 状态字段为 tagged enum：

```json
{ "type": "Online" }
```

仓库中同时保存：

- `status`
- `last_heartbeat`

列表筛选时可以根据在线状态过滤。

## 日志代理

`agent-manager` 并不直接管理 Agent 日志文件，而是将请求转发给目标 Agent：

- `GET /api/v1/agents/{agent_id}/log/config`
- `PUT /api/v1/agents/{agent_id}/log/level`
- `PUT /api/v1/agents/{agent_id}/log/retention`

代理访问时会：

1. 从 Agent 标签中提取 `host`
2. 从 Agent 标签中提取 `listen_port`
3. 组合出 `http://host:port/api/v1/log/*`

超时默认 10 秒，可通过 `OPSBOX_PROXY_TIMEOUT_SECS` 调整。

## 与其他模块的关系

### 与 `opsbox-agent`

- Agent 主动注册和心跳
- Server 通过标签回推可访问地址

### 与 `explorer`

- `explorer` 在启用 `agent-manager` feature 时会尝试复用全局 `AgentManager`
- 这样浏览器侧可以枚举在线 Agent 并拼装 ORL

### 与 `logseek`

- `logseek` 不再自己维护 Agent 注册表
- 远程搜索所需 Agent 元信息来自 `agent-manager`

## 当前实现结论

- `agent-manager` 已是独立模块，不是 `logseek` 子模块
- 路由前缀固定为 `/api/v1/agents`
- 标签和日志代理接口都已经落地
- 全局 `AgentManager` 初始化发生在 `init_schema()` 阶段
