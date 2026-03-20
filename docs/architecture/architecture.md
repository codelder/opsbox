# OpsBox 当前架构概览

**文档版本**: v2.0  
**最后更新**: 2026-03-20

本文档只描述仓库当前已经落地的实现，不覆盖已过时的草案。

## 系统组成

```text
web (SvelteKit SPA)
        |
        v
opsbox-server (Axum)
  |- /healthy
  |- /api/v1/log/*
  |- /api/v1/logseek/*
  |- /api/v1/agents/*
  |- /api/v1/explorer/*
  `- embedded frontend static assets

opsbox-agent (standalone Axum service)
  |- /health
  |- /api/v1/info
  |- /api/v1/search
  |- /api/v1/paths
  |- /api/v1/list_files
  |- /api/v1/file_raw
  `- /api/v1/log/*
```

## 后端分层

### `opsbox-server`

职责：

- 解析 CLI
- 初始化日志与数据库
- 通过 `inventory` 发现模块
- 依次调用模块的 `configure()`、`init_schema()`、`router()`
- 合并系统日志路由和业务模块路由
- 以 `rust-embed` 形式提供前端静态资源

关键文件：

- `backend/opsbox-server/src/main.rs`
- `backend/opsbox-server/src/server.rs`
- `backend/opsbox-server/src/config.rs`
- `backend/opsbox-server/src/log_routes.rs`

### `opsbox-core`

职责：

- `Module` trait 和模块注册机制
- 统一错误与响应模型
- SQLite 连接池
- 日志初始化与动态日志级别重载
- DFS/ORL 抽象
- Agent / S3 共享类型与客户端能力

### `logseek`

职责：

- 搜索入口 `/search.ndjson`
- 结果查看 `/view.*`
- S3 默认配置 `/settings/s3`
- S3 profiles `/profiles`
- LLM backends `/settings/llm/*`
- planners `/settings/planners/*`
- 自然语言转查询 `/nl2q`

模块内部主要采用：

- routes
- service
- repository
- domain
- utils

### `agent-manager`

职责：

- Agent 注册与心跳
- Agent 列表、详情、标签管理
- 按标签和在线状态筛选
- 将服务端请求代理到 Agent 的 `/api/v1/log/*`

关键点：

- 使用数据库持久化 Agent 信息
- 通过标签保存 `host` 和 `listen_port`
- 提供全局 `AgentManager` 实例给其他模块复用

### `explorer`

职责：

- ORL 驱动的统一资源浏览与下载
- 支持 Local、S3、Agent
- 支持归档内浏览

接口：

- `POST /api/v1/explorer/list`
- `GET /api/v1/explorer/download`

### `opsbox-agent`

职责：

- 启动本地搜索与文件访问 HTTP 服务
- 向 `opsbox-server` 注册自己
- 周期性发送心跳
- 对外暴露搜索、列目录、原始文件读取与日志配置接口

默认监听端口：`3976`

## 模块装配方式

`opsbox-server` 并不手写业务模块路由，而是：

1. 编译期通过 `opsbox_core::register_module!` 收集模块工厂
2. 运行时 `get_all_modules()` 取回模块实例
3. 对每个模块执行：
   - `configure()`
   - `init_schema()`
   - `router()`
4. 将模块路由嵌套到自己的 `api_prefix()`

当前默认启用模块：

- `logseek`
- `agent-manager`
- `explorer`

## ORL 资源定位

系统当前统一使用 `orl://`。

常见形式：

```text
orl://local/var/log/app.log
orl://agent/
orl://s3/
orl://web-01@agent/var/log/app.log
orl://web-01@10.0.0.8:3976@agent/var/log/app.log
orl://default@s3/my-bucket/path/to/file.log
orl://prod:my-bucket@s3/path/to/file.log
orl://local/var/log/archive.tar.gz?entry=inner/file.log
orl://local/var/log/?glob=*.log
```

实现特点：

- `?entry=` 表示归档内条目
- `?glob=` 表示 ORL 自带路径过滤
- S3 bucket 兼容两种表达：
  - endpoint identity 中携带 bucket：`profile:bucket@s3`
  - path 第一段为 bucket：`profile@s3/bucket/...`

## 前端结构

前端位于 `web/`，主要特征：

- SvelteKit 静态站点
- 通过 Vite 开发代理访问后端 API
- 模块化目录：
  - `src/lib/modules/logseek`
  - `src/lib/modules/agent`
  - `src/lib/modules/explorer`

主要页面：

- `/`
- `/search`
- `/view`
- `/image-view`
- `/explorer`
- `/settings`
- `/prompt`

## 数据与配置

### Server

- 数据库默认：`$HOME/.opsbox/opsbox.db`
- 日志目录默认：`$HOME/.opsbox/logs`

### Agent

- 日志目录默认：`$HOME/.opsbox-agent/logs`
- `server_endpoint` 默认：`http://localhost:4000`
- `search_roots` 默认：当前用户主目录

### LogSeek 调优

由 `opsbox-server` 启动时注入环境变量给模块读取：

- `LOGSEEK_IO_MAX_CONCURRENCY`
- `LOGSEEK_IO_TIMEOUT_SEC`
- `LOGSEEK_IO_MAX_RETRIES`
- `LOGSEEK_SERVER_ID`

## 当前实现里的关键事实

- 前端产物已内嵌到 `opsbox-server` 二进制
- Server 级日志 API 不属于 `logseek`，而是 `/api/v1/log/*`
- S3 默认配置和 S3 profile 已统一存储到 `s3_profiles` 表，`default` 只是保留 profile
- Agent 的取消搜索接口目前仍返回 `501 Not Implemented`
- Explorer 和前端已经以 ORL 为主，不再以旧的 FileUrl/ODFI 命名作为对外协议
