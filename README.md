# OpsBox 日志检索与资源浏览平台

OpsBox 是一个基于 Rust 后端和 SvelteKit 前端的运维工具箱，当前重点提供日志检索、分布式资源浏览、Agent 管理以及 LLM/Planner 配置能力。

## 架构概览

### 后端 (`backend/`)

Rust workspace 当前包含以下 crate：

- `opsbox-server`
  - 主服务二进制，默认监听 `0.0.0.0:4000`
  - 负责模块发现、数据库初始化、日志配置 API 和嵌入式前端资源分发
- `opsbox-core`
  - 共享基础设施：模块系统、错误模型、数据库、日志、DFS/ORL 抽象、Agent/S3 通用能力
- `logseek`
  - 日志检索模块
  - 提供搜索、文件查看、S3 配置、LLM 后端、Planner 脚本、自然语言转查询等 API
- `explorer`
  - 统一资源浏览模块
  - 支持 Local、Agent、S3 以及归档内浏览与下载
- `agent-manager`
  - Agent 注册、心跳、标签管理、Agent 日志配置代理
- `opsbox-agent`
  - 独立 Agent 二进制
  - 向服务端注册并暴露本地搜索、文件浏览、原始文件读取、日志配置接口
- `test-common`
  - 集成测试共用工具

### 前端 (`web/`)

- SvelteKit 2 + Svelte 5 + TypeScript
- 静态构建后输出到 `backend/opsbox-server/static`
- 主要路由：
  - `/` 首页查询入口
  - `/search` 搜索结果
  - `/view` 文本查看
  - `/image-view` 图片查看
  - `/explorer` 资源浏览器
  - `/settings` 对象存储、Agent、Planner、LLM、Server 日志设置
  - `/prompt` Prompt 调试页
- 模块化前端代码位于 `web/src/lib/modules/`
  - `logseek/`
  - `agent/`
  - `explorer/`

## 快速开始

### 环境要求

- Rust 1.90.0
- Node.js 22
- `corepack` 可用

### 安装依赖

```bash
corepack enable
corepack prepare pnpm@10.23.0 --activate
pnpm --dir web install
```

### 启动开发环境

```bash
# 终端 1：后端
cargo run --manifest-path backend/Cargo.toml -p opsbox-server

# 终端 2：前端
pnpm --dir web dev
```

默认访问：

- 前端：[http://localhost:5173](http://localhost:5173)
- 后端健康检查：[http://localhost:4000/healthy](http://localhost:4000/healthy)

### 启动 Agent（可选）

```bash
cargo run --manifest-path backend/Cargo.toml -p opsbox-agent -- \
  --server-endpoint http://localhost:4000 \
  --search-roots /var/log,/tmp
```

`opsbox-agent` 默认监听端口为 `3976`。

### 生产构建

```bash
# 构建前端静态资源
pnpm --dir web build

# 构建后端主服务
cargo build --manifest-path backend/Cargo.toml -p opsbox-server --release

# 构建 Agent
cargo build --manifest-path backend/Cargo.toml -p opsbox-agent --release
```

## 主要功能

### 日志检索

- GitHub 风格查询语法
- NDJSON 流式搜索结果
- 本地文件、S3/MinIO、远程 Agent 混合检索
- 文件查看、下载、编码识别、上下文高亮
- 自然语言转查询
- Starlark Planner 脚本配置

### 资源浏览

- 基于 ORL (`orl://`) 的统一资源定位
- 浏览 Local、Agent、S3 三类端点
- 支持 tar、tar.gz、gz、zip 等归档内容浏览
- 资源下载

### Agent 管理

- Agent 注册与心跳
- 标签 CRUD 与按标签筛选
- 代理访问 Agent 的日志配置接口

### 配置管理

- S3 默认配置与多 Profile 管理
- 多个 LLM backend 管理（当前支持 `ollama`、`openai`）
- 默认 LLM backend 选择
- Planner 脚本 CRUD、测试与默认脚本设置
- Server 运行日志级别与保留数调整

## 运行与配置

### 服务监听

- 默认：`0.0.0.0:4000`
- 覆盖方式：
  - `--host` / `--port`
  - `--addr`

### 数据库

- 默认：`$HOME/.opsbox/opsbox.db`
- 覆盖优先级：
  - `--database-url`
  - `OPSBOX_DATABASE_URL`
  - `DATABASE_URL`

### 日志

- 服务日志目录默认：`$HOME/.opsbox/logs`
- Agent 日志目录默认：`$HOME/.opsbox-agent/logs`
- 服务端可通过以下方式设置日志级别：
  - `--log-level error|warn|info|debug|trace`
  - `-v` / `-vv` / `-vvv`
  - `RUST_LOG`

### LogSeek IO 调优

- `LOGSEEK_IO_MAX_CONCURRENCY`
- `LOGSEEK_IO_TIMEOUT_SEC`
- `LOGSEEK_IO_MAX_RETRIES`
- `LOGSEEK_SERVER_ID`

### 前端 API 基址

- `PUBLIC_API_BASE`，默认 `/api/v1/logseek`
- `PUBLIC_AGENTS_API_BASE`，默认 `/api/v1/agents`

### 守护进程

类 Unix 上 `opsbox-server` 和 `opsbox-agent` 都支持 `start` / `stop` 子命令。

```bash
# 后台启动服务
cargo run --manifest-path backend/Cargo.toml -p opsbox-server -- start --daemon

# 停止服务
cargo run --manifest-path backend/Cargo.toml -p opsbox-server -- stop
```

## 常用脚本

脚本集中在 `scripts/`，建议直接查看 [scripts/README.md](scripts/README.md)。

当前常用脚本包括：

- `scripts/run/start-server.sh`
- `scripts/run/start-agent.sh`
- `scripts/run/run-agent.sh`
- `scripts/build/build-frontend.sh`
- `scripts/test/bench-ndjson.sh`
- `scripts/test/bench-logging-performance.sh`
- `scripts/monitor/run_tests_with_monitoring.sh`

## 文档索引

- [CLAUDE.md](CLAUDE.md)
- [docs/README.md](docs/README.md)
- [docs/architecture/architecture.md](docs/architecture/architecture.md)
- [docs/guides/query-syntax.md](docs/guides/query-syntax.md)
- [docs/guides/frontend-development.md](docs/guides/frontend-development.md)
- [docs/modules/agent-api-spec.md](docs/modules/agent-api-spec.md)
- [docs/modules/agent-manager.md](docs/modules/agent-manager.md)
- [docs/features/file-url.md](docs/features/file-url.md)
- [docs/features/s3-profiles.md](docs/features/s3-profiles.md)

## 代码质量

### Rust

```bash
cargo fmt --manifest-path backend/Cargo.toml --all
cargo clippy --manifest-path backend/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path backend/Cargo.toml
```

### 前端

```bash
pnpm --dir web format
pnpm --dir web lint
pnpm --dir web check
pnpm --dir web test
pnpm --dir web test:e2e
```

## 技术栈

### 后端

- Rust 2024 edition
- Axum
- Tokio
- SQLite + sqlx
- tracing
- aws-sdk-s3
- starlark
- grep

### 前端

- SvelteKit 2
- Svelte 5
- TypeScript
- Tailwind CSS 4
- Vite 7
- Playwright
- Vitest

## License

MIT
