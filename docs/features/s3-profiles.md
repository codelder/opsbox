# S3 Profiles 与默认配置

**文档版本**: v1.1  
**最后更新**: 2026-03-20

本文档描述当前仓库中已经实现的 S3 配置模型。

## 当前设计

OpsBox 现在有两层 S3 配置概念：

### 1. 默认配置

接口：

- `GET /api/v1/logseek/settings/s3`
- `POST /api/v1/logseek/settings/s3`

字段：

- `endpoint`
- `access_key`
- `secret_key`
- `configured`
- `connection_error`

说明：

- 默认配置本质上就是 profile 名为 `default` 的一条记录
- 当前不再单独保存 bucket

### 2. Profiles

接口：

- `GET /api/v1/logseek/profiles`
- `POST /api/v1/logseek/profiles`
- `DELETE /api/v1/logseek/profiles/{name}`

字段：

- `profile_name`
- `endpoint`
- `access_key`
- `secret_key`

说明：

- Profile 只描述连接端点和凭证
- bucket 不属于 profile 本身，而是在 ORL、Explorer 或搜索规划时指定

## 为什么去掉了 bucket

当前实现里：

- 同一个 endpoint/credential 往往需要访问多个 bucket
- bucket 更适合作为资源定位信息，而不是连接配置的一部分
- 因此前端与后端的 profile payload 都已经移除了 `bucket`

## 数据库存储

当前 `logseek` 模块使用单表：

```sql
CREATE TABLE IF NOT EXISTS s3_profiles (
    profile_name TEXT PRIMARY KEY,
    endpoint TEXT NOT NULL,
    access_key TEXT NOT NULL,
    secret_key TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

约定：

- `default` profile 对应旧的“默认 S3 配置”
- 其他 profile 用于多环境、多账号、多 endpoint

## API 示例

### 读取默认配置

```http
GET /api/v1/logseek/settings/s3
```

响应：

```json
{
  "endpoint": "http://minio.example.com:9000",
  "access_key": "minioadmin",
  "secret_key": "minioadmin",
  "configured": true,
  "connection_error": null
}
```

### 写入默认配置

```http
POST /api/v1/logseek/settings/s3
Content-Type: application/json

{
  "endpoint": "http://minio.example.com:9000",
  "access_key": "minioadmin",
  "secret_key": "minioadmin"
}
```

响应：`204 No Content`

### 列出 Profiles

```http
GET /api/v1/logseek/profiles
```

响应：

```json
{
  "profiles": [
    {
      "profile_name": "default",
      "endpoint": "http://minio.example.com:9000",
      "access_key": "minioadmin",
      "secret_key": "minioadmin"
    },
    {
      "profile_name": "prod",
      "endpoint": "https://s3.amazonaws.com",
      "access_key": "AKIA...",
      "secret_key": "secret"
    }
  ]
}
```

### 创建或更新 Profile

```http
POST /api/v1/logseek/profiles
Content-Type: application/json

{
  "profile_name": "staging",
  "endpoint": "http://staging-minio:9000",
  "access_key": "staging_key",
  "secret_key": "staging_secret"
}
```

响应：`204 No Content`

### 删除 Profile

```http
DELETE /api/v1/logseek/profiles/staging
```

限制：

- `default` 不能被删除

## 与 ORL 的关系

S3 bucket 在当前实现里来自 ORL 或查询规划。

常见 ORL 形式：

```text
orl://default@s3/my-bucket/path/to/file.log
orl://prod:my-bucket@s3/path/to/file.log
```

两种写法都被兼容：

- `profile@s3/bucket/key`
- `profile:bucket@s3/key`

## 与前端设置页的关系

`/settings` 页面当前包含：

- 对象存储
- Agent
- 规划脚本
- 大模型
- Server 日志

对象存储页默认以 Profile 管理为主，同时仍保留默认配置接口兼容。

## 当前实现边界

- 保存 S3 配置时当前不会主动验证 bucket 连接，因为 payload 中已经没有 bucket
- 真实 bucket 连通性会在实际搜索、浏览或读取对象时体现
