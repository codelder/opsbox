# OpsBox 日志检索平台

基于 Rust 后端和 SvelteKit 前端的日志搜索分析平台。

## 架构概览

### 后端 (`backend/`)

**Monorepo 结构，包含多个 crate：**

- **opsbox-server** (主程序，输出二进制名 `opsbox-server`)
  - 模块化结构：config、logging、daemon、server
  - 内嵌前端静态资源
  - SQLite 数据库管理
  - 监听 127.0.0.1:4000

- **opsbox-core** (共享库)
  - 统一错误处理 (RFC 7807 Problem Details)
  - 数据库连接池管理
  - 标准响应格式封装
  - ORL 协议 (`orl://`) - 统一的跨端点资源定位符

- **logseek** (日志检索模块)
  - 分层架构：api、service、repository、domain、utils
  - 支持本地文件系统、S3/MinIO 和远程 Agent
  - NDJSON 流式搜索
  - 自然语言转查询（基于 Ollama/OpenAI）
  - Starlark 脚本化源规划

- **explorer** (分布式资源浏览器模块)
  - 统一浏览 Local、S3、Agent 端点
  - 支持归档文件内浏览（tar、tar.gz、zip 等）
  - 文件下载功能
  - 内容类型自动检测

- **agent-manager** (代理管理模块)
  - 代理注册、健康监控和标签管理
  - 基于标签的代理组织和过滤
  - 代理日志配置代理

- **agent** (独立代理二进制)
  - 远程日志访问代理
  - 支持与主服务器通信

### 前端 (`web/`)

- **SvelteKit** SPA (使用 adapter-static)
- **模块化架构** (`src/lib/modules/`)
  - `logseek/`: 日志搜索相关组件和 API 客户端
  - `explorer/`: 分布式资源浏览器 UI 和 API 客户端
  - `agent/`: 代理管理相关组件和 API 客户端
- **Vite** 开发服务器（代理 /api 到后端）
- **TailwindCSS 4.0** 样式框架
- **Maple Mono NF CN** 字体系统（5 种字重嵌入）

## 快速开始

### 环境要求

- **Rust**: 1.90.0 (通过 rust-toolchain.toml 固定)
- **Node.js**: 22 (使用 nvm: `nvm use 22`)
- **pnpm**: 通过 corepack 启用

### 安装依赖

```bash
# 前端依赖
corepack enable
corepack prepare pnpm@10.17.1 --activate
pnpm --dir web install
```

### 启动开发服务器

```bash
# 后端（终端1）
cargo run --manifest-path backend/Cargo.toml -p opsbox-server

# 前端（终端2）
pnpm --dir web dev
```

访问：http://localhost:5173

### 生产构建

```bash
# 构建前端（输出到 backend/opsbox-server/static，构建前会清空该目录）
pnpm --dir web build

# 构建后端（会将静态资源嵌入二进制）
cargo build --manifest-path backend/Cargo.toml -p opsbox-server --release
```

## 主要功能

### 日志搜索

- GitHub 风格的查询语法（AND/OR/NOT、正则、短语）
- 支持本地文件系统、S3/MinIO 和远程 Agent
- NDJSON 流式结果返回
- 上下文窗口和关键词高亮
- Starlark 脚本化源规划
- 自然语言转查询（基于 Ollama/OpenAI）

### 分布式资源浏览器

- 统一浏览 Local、S3、Agent 端点
- 支持归档文件内浏览（tar、tar.gz、zip 等）
- 文件下载功能
- 内容类型自动检测
- 隐藏文件计数和目录子项统计

### 代理管理

- 代理注册、健康监控和心跳机制
- 基于标签的代理组织和过滤
- 完整的标签 CRUD 操作（添加/移除/清空）
- 代理日志配置代理（级别、保留策略）

### 对象存储设置（S3 Profiles）

- 通过 Web UI 管理多个 S3 Profile（endpoint/bucket/credentials）
- 首次启动会自动迁移旧的单一 S3 设置到 `default` profile
- 保留 `/settings/s3` 端点以兼容旧前端，推荐使用 Profiles 管理

### ORL 协议（统一资源定位）

- `orl://` 协议统一标识 Local、Agent、S3 资源
- 支持归档文件内条目访问（`?entry=` 参数）
- 向后兼容旧的 `odfi://` 格式

## 配置

### 数据库

- 默认：`$HOME/.opsbox/opsbox.db`
- 覆盖：`--database-url` 或 `OPSBOX_DATABASE_URL`/`DATABASE_URL` 环境变量

### 日志级别

- `--log-level error|warn|info|debug|trace`
- 或使用 `-V`/`-VV`/`-VVV`
- 或设置 `RUST_LOG` 环境变量
- 检索日志分层建议见 `docs/guides/logging-configuration.md`

### 守护进程（macOS/Linux）

```bash
# 启动守护进程
cargo run -p opsbox-server -- start --daemon

# 停止守护进程
cargo run -p opsbox-server -- stop
```

## 📚 开发文档

### 项目文档
- **架构说明**: [docs/architecture/architecture.md](docs/architecture/architecture.md) - 系统架构设计
- **项目指南**: [WARP.md](WARP.md) - WARP AI 开发指南

### 架构文档
- **架构复盘**: [docs/architecture/architecture.md](docs/architecture/architecture.md) - 项目架构详细分析
- **模块架构**: [docs/architecture/module-architecture.md](docs/architecture/module-architecture.md) - 模块系统设计
- **错误处理**: [docs/architecture/error-handling-architecture.md](docs/architecture/error-handling-architecture.md) - 错误处理架构
- **日志系统**: [docs/architecture/logging-architecture.md](docs/architecture/logging-architecture.md) - 日志系统架构设计

### 模块文档
- **Agent Manager**: [docs/modules/agent-manager.md](docs/modules/agent-manager.md) - Agent 管理模块
- **Agent API**: [docs/modules/agent-api-spec.md](docs/modules/agent-api-spec.md) - Agent HTTP API 规范

### 功能文档
- **FileUrl 设计**: [docs/features/file-url.md](docs/features/file-url.md) - 文件 URL 抽象层
- **S3 Profiles**: [docs/features/s3-profiles.md](docs/features/s3-profiles.md) - S3 配置管理
- **Agent 标签**: [docs/features/agent-tags.md](docs/features/agent-tags.md) - Agent 标签管理

### 使用指南
- **查询语法**: [docs/guides/query-syntax.md](docs/guides/query-syntax.md) - 搜索查询语法
- **日志配置**: [docs/guides/logging-configuration.md](docs/guides/logging-configuration.md) - 日志系统配置和管理
- **Tracing 使用**: [docs/guides/tracing-usage.md](docs/guides/tracing-usage.md) - 开发者日志使用指南
- **前端开发**: [docs/guides/frontend-development.md](docs/guides/frontend-development.md) - 前端模块化架构
- **CPU 资源控制**: [docs/guides/cpu-resource-control.md](docs/guides/cpu-resource-control.md) - Agent CPU 资源控制

### 脚本工具
- **运行脚本** (`scripts/run/`):
  - [start-server.sh](scripts/run/start-server.sh) - 启动 Server
  - [start-agent.sh](scripts/run/start-agent.sh) - 启动 Agent
  - [run-agent.sh](scripts/run/run-agent.sh) - 运行 Agent（完整配置）
- **测试脚本** (`scripts/test/`):
  - [test-agent-api.sh](scripts/test/test-agent-api.sh) - Agent API 测试
  - [test-graceful-shutdown.sh](scripts/test/test-graceful-shutdown.sh) - 优雅关闭测试
  - [bench-ndjson.sh](scripts/test/bench-ndjson.sh) - NDJSON 性能测试
- **构建脚本** (`scripts/build/`):
  - [build-frontend.sh](scripts/build/build-frontend.sh) - 构建前端
- **数据生成脚本** (`scripts/generate/`):
  - [generate-test-logs.py](scripts/generate/generate-test-logs.py) - 生成测试日志

## 代码规范

### Rust

```bash
# 格式化
cargo fmt --all

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# 测试
cargo test
```

### 前端

```bash
# 格式化
pnpm --dir web format

# Lint
pnpm --dir web lint

# 类型检查
pnpm --dir web check

# 测试
pnpm --dir web test
```

## 技术栈

### 后端

- Rust 1.90.0 (2024 edition)
- Axum (HTTP 框架)
- SQLite + sqlx (数据库)
- tokio (异步运行时)
- tracing (结构化日志系统)
- starlark (脚本化源规划)
- grep (字节级正则搜索)
- fluent-uri (RFC 3986 URI 解析)
- async_zip + tokio-tar (异步归档支持)
- aws-sdk-s3 (S3 存储支持)

### 前端

- SvelteKit 2.22 (框架)
- Svelte 5 (UI 库，Runes API)
- TypeScript (类型系统)
- TailwindCSS 4.0 (样式框架，使用 @tailwindcss/vite 插件)
- Vite 7.0 (构建工具)
- @tanstack/svelte-virtual (虚拟滚动)
- lucide-svelte (图标库)
- bits-ui (UI 组件)
- Maple Mono NF CN 字体系统（5 种字重嵌入）

## License

MIT
