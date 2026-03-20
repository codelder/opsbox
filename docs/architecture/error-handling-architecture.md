# LogSeek 错误处理架构

**文档版本**: v1.1  
**最后更新**: 2026-03-20

本文档描述 `logseek` 模块当前已经落地的错误分层，而不是理想化的未来设计。

## 当前分层

```text
query / dfs domain
  |- ParseError
  `- OrlParseError

repository
  `- RepositoryError

service
  `- ServiceError

api
  `- LogSeekApiError

core
  `- opsbox_core::AppError
```

## 各层职责

### Query / DFS / Domain 级

#### `ParseError`

定义在：

- `backend/logseek/src/query.rs`

负责：

- 查询语法错误
- 正则错误
- 路径 glob 错误
- 括号不匹配

#### `OrlParseError`

定义在：

- `backend/opsbox-core/src/dfs/orl_parser.rs`

负责：

- ORL 格式非法
- endpoint 类型非法
- Agent / S3 endpoint 结构非法

在 API 层会被转成 `LogSeekApiError::Domain(String)`。

### Repository 层

定义在：

- `backend/logseek/src/repository/error.rs`

当前类型：

- `QueryFailed`
- `StorageError`
- `NotFound`
- `CacheFailed`
- `Database`

特点：

- `sqlx::Error` 会统一转成 `RepositoryError::Database`
- repository 不直接返回 HTTP 语义

### Service 层

定义在：

- `backend/logseek/src/service/error.rs`

当前类型：

- `ConfigError`
- `SearchFailed { path, error }`
- `ProcessingError`
- `IoError { path, error }`
- `ChannelClosed`
- `Repository(RepositoryError)`

特点：

- service 可以保留业务上下文，例如路径和具体操作
- repository 错误通过 `From` 自动抬升到 service

### API 层

定义在：

- `backend/logseek/src/api/error.rs`

当前 `LogSeekApiError` 聚合：

- `Service(ServiceError)`
- `Repository(RepositoryError)`
- `Domain(String)`
- `BadJson(JsonRejection)`
- `QueryParse(ParseError)`
- `StorageError(S3Error)`
- `Internal(AppError)`

这是 `logseek` HTTP 路由直接返回的模块级错误。

## HTTP 响应映射

`LogSeekApiError` 实现了 `IntoResponse`，统一返回：

- `application/problem+json; charset=utf-8`
- RFC 7807 风格字段：
  - `type`
  - `title`
  - `detail`
  - `status`

典型映射：

| 错误来源 | HTTP 状态 | title |
| --- | --- | --- |
| `BadJson` | `400` | `JSON 请求格式错误` |
| `QueryParse` | `400` | `查询语法错误` |
| `Domain` | `400` | `业务验证失败` |
| `Repository::NotFound` | `404` | `资源不存在` |
| `Repository::StorageError` | `502` | `存储服务错误` |
| `Internal(AppError::BadRequest)` | `400` | `内部错误` |
| `Internal(AppError::NotFound)` | `404` | `内部错误` |
| `Internal(AppError::ExternalService)` | `502` | `内部错误` |

注意：

- `title` 不是直接照搬底层错误名，而是经过 API 层重新归类
- `detail` 才会保留更具体的底层信息

## 流式搜索的特殊处理

`POST /api/v1/logseek/search.ndjson` 与普通 JSON 路由有一个关键差异：

- 在流开始之前出现错误，仍通过 `LogSeekApiError` 返回 Problem JSON
- 一旦流开始，后续错误不会再切换成 Problem JSON，而是通过 NDJSON 事件输出

当前事件类型定义在：

- `backend/logseek/src/routes/search.rs`
- `backend/logseek/src/service/search.rs`

主要事件：

- `result`
- `error`
- `complete`
- `finished`

因此搜索接口存在两层错误语义：

1. HTTP 建连前错误：Problem JSON
2. 流内运行时错误：NDJSON `error` 事件

这是当前实现的既定行为，不是异常。

## 错误传播路径

### 普通 JSON 路由

```text
repository/service/query error
  -> LogSeekApiError
  -> IntoResponse
  -> application/problem+json
```

### 搜索 NDJSON 路由

```text
plan/build error before stream
  -> LogSeekApiError
  -> application/problem+json

runtime error after stream started
  -> SearchEvent::Error
  -> NDJSON line
```

## 当前架构的优点

- 分层边界已经清晰：repository / service / api 各有自己的错误类型
- `LogSeekApiError` 是对外统一出口
- `ParseError` 和 `OrlParseError` 仍保留较细粒度的领域信息
- 流式接口没有吞错，而是明确输出 `error` 事件

## 当前仍存在的现实边界

- `Domain` 在 `LogSeekApiError` 中目前是字符串，而不是更强类型的 enum 聚合
- `opsbox_core::AppError` 仍会跨入 `logseek` API 层
- 不同模块并没有完全共享同一套模块级错误类型
- 流式错误和普通 HTTP 错误是两套输出模型，调用方需要分别处理

这些属于当前实现状态，不影响使用，但在继续统一错误模型时需要注意。
