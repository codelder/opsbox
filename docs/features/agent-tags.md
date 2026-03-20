# Agent 标签功能

**文档版本**: v1.1  
**最后更新**: 2026-03-20

本文档描述 `agent-manager` 当前已经实现的标签能力。

## 概述

Agent 标签采用 `key=value` 结构，用于：

- 环境分类
- 服务分组
- 机房或区域筛选
- 前端设置页筛选 Agent
- 给日志搜索或资源浏览做上游选择

标签模型定义在：

- `backend/opsbox-core/src/agent/models.rs`

```json
{
  "key": "env",
  "value": "production"
}
```

## 当前实现的标签来源

标签来源分两类：

### 1. Agent 自带标签

Agent 注册时，`AgentInfo.tags` 可以带入一组标签。

### 2. Server 自动补充标签

当 Agent 调用 `/api/v1/agents/register` 时，服务端会根据连接来源自动追加：

- `host`
- `listen_port`

这两个标签很重要，因为后续：

- Agent 日志代理
- Explorer 访问 Agent
- 其他模块构造 Agent endpoint

都依赖它们。

因此它们既是系统标签，也是普通标签，会和用户标签一起出现在返回结果中。

## 当前支持的能力

- 查看所有标签集合
- 查看单个 Agent 标签
- 覆盖设置标签
- 添加单个标签
- 删除单个标签
- 清空标签
- 按标签筛选 Agent
- 结合 `online_only=true` 只看在线 Agent

## 标签筛选语义

查询参数：

```text
GET /api/v1/agents?tags=env=production,team=frontend&online_only=true
```

规则：

- 多个标签条件是 AND 关系
- 条件格式必须是 `key=value`
- `online_only=true` 会在标签过滤基础上再过滤在线 Agent

例如：

- `tags=env=production`
- `tags=env=production,region=cn`
- `tags=service=web&online_only=true`

## 数据库存储

当前标签持久化到：

- `agents`
- `agent_tags`

其中 `agent_tags` 具备约束：

- `(agent_id, tag_key, tag_value)` 唯一

因此同一个 Agent 不会重复存储完全相同的 key-value 对。

## 前端中的使用

设置页 `AgentManagement` 当前直接使用标签功能：

- 输入 `key=value,key2=value2` 进行筛选
- 为单个 Agent 增删标签
- 同时展示系统标签和业务标签

相关文件：

- `web/src/routes/settings/AgentManagement.svelte`
- `web/src/lib/modules/agent/api/agents.ts`

## 当前未开放为 HTTP API 的能力

`AgentManager` 内部还有两个辅助能力：

- `get_all_tag_keys()`
- `get_tag_values_by_key(key)`

它们当前存在于 manager / repository 层，但**没有暴露为公开 HTTP 路由**。

所以当前对外 API 里：

- 可以获取全部标签列表 `/api/v1/agents/tags`
- 不能直接通过 HTTP 单独列出“所有 key”或“某个 key 的所有值”

## 使用建议

- 将 `env`、`region`、`team`、`service` 作为业务标签
- 不要手工覆盖 `host`、`listen_port` 这类系统标签
- 标签值尽量稳定，便于前端和脚本复用

## 示例

### 注册后带业务标签

```json
{
  "id": "agent-prod-web-01",
  "name": "Production Web Agent 01",
  "version": "0.2.0",
  "hostname": "web-server-01",
  "tags": [
    { "key": "env", "value": "production" },
    { "key": "service", "value": "web" },
    { "key": "region", "value": "cn" }
  ],
  "search_roots": ["/var/log/nginx"],
  "last_heartbeat": 0,
  "status": { "type": "Online" },
  "listen_port": 3976
}
```

注册完成后，服务端通常还会补上：

```json
{ "key": "host", "value": "10.0.0.8" }
{ "key": "listen_port", "value": "3976" }
```
