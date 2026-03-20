# Agent 标签 API

**文档版本**: v1.1  
**最后更新**: 2026-03-20

本文档只覆盖当前已经公开的标签相关 HTTP API。

## API 总览

前缀：`/api/v1/agents`

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/tags` | 获取所有标签字符串 |
| `GET` | `/{agent_id}/tags` | 获取单个 Agent 标签 |
| `POST` | `/{agent_id}/tags` | 覆盖设置标签 |
| `POST` | `/{agent_id}/tags/add` | 添加单个标签 |
| `DELETE` | `/{agent_id}/tags/remove` | 删除单个标签 |
| `DELETE` | `/{agent_id}/tags/clear` | 清空所有标签 |
| `GET` | `/` | 列表查询时支持 `tags` 和 `online_only` 参数 |

## 1. 获取所有标签

### `GET /api/v1/agents/tags`

响应：

```json
{
  "tags": [
    "env=production",
    "service=web",
    "region=cn",
    "host=10.0.0.8",
    "listen_port=3976"
  ],
  "total": 5
}
```

说明：

- 返回值是字符串数组，不是对象数组
- 会包含系统标签，如 `host` 和 `listen_port`

## 2. 获取单个 Agent 标签

### `GET /api/v1/agents/{agent_id}/tags`

响应：

```json
[
  { "key": "env", "value": "production" },
  { "key": "service", "value": "web" },
  { "key": "host", "value": "10.0.0.8" },
  { "key": "listen_port", "value": "3976" }
]
```

找不到 Agent 时返回：

- `404 Not Found`

## 3. 覆盖设置标签

### `POST /api/v1/agents/{agent_id}/tags`

请求：

```json
{
  "tags": [
    { "key": "env", "value": "production" },
    { "key": "team", "value": "frontend" }
  ]
}
```

响应：

```json
{
  "message": "标签设置成功"
}
```

说明：

- 这是覆盖式设置，不是追加
- 现有标签会被整组替换

## 4. 添加单个标签

### `POST /api/v1/agents/{agent_id}/tags/add`

请求：

```json
{
  "key": "priority",
  "value": "high"
}
```

响应：

```json
{
  "message": "标签添加成功"
}
```

说明：

- 如果同一个 key-value 已存在，不会重复添加

## 5. 删除单个标签

### `DELETE /api/v1/agents/{agent_id}/tags/remove`

请求：

```json
{
  "key": "priority",
  "value": "high"
}
```

响应：

```json
{
  "message": "标签移除成功"
}
```

## 6. 清空标签

### `DELETE /api/v1/agents/{agent_id}/tags/clear`

响应：

```json
{
  "message": "标签清空成功"
}
```

说明：

- 当前实现会清空该 Agent 的全部标签
- 包括系统补充的标签也会被清空；但 Agent 后续再次注册时，服务端会重新补充 `host` / `listen_port`

## 7. 按标签筛选 Agent

### `GET /api/v1/agents?tags=env=production,team=frontend&online_only=true`

查询参数：

- `tags`
  - 多个条件逗号分隔
  - 每项必须是 `key=value`
- `online_only`
  - `true` / `false`

响应：

```json
{
  "agents": [
    {
      "id": "agent-prod-web-01",
      "name": "Production Web Agent 01",
      "version": "0.2.0",
      "hostname": "web-server-prod-01",
      "tags": [
        { "key": "env", "value": "production" },
        { "key": "team", "value": "frontend" }
      ],
      "search_roots": ["/var/log/nginx"],
      "last_heartbeat": 1760000000,
      "status": { "type": "Online" }
    }
  ],
  "total": 1
}
```

规则：

- 多标签是 AND 关系
- `online_only=true` 只返回在线 Agent

## 当前未公开的接口

当前并没有以下 HTTP 路由：

- 获取全部标签键列表
- 获取指定标签键的所有值

虽然 manager / repository 内部有对应方法，但目前只作为内部能力存在。
