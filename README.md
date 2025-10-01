# OpsBox 日志检索平台

基于 Rust 后端和 SvelteKit 前端的日志搜索分析平台。

## 架构概览

### 后端 (`server/`)

**Monorepo 结构，包含三个 crate：**

- **api-gateway** (主程序，输出二进制名 `opsbox`)
  - 模块化结构：config、logging、daemon、server
  - 内嵌前端静态资源
  - SQLite 数据库管理
  - 监听 127.0.0.1:4000
  
- **opsbox-core** (共享库)
  - 统一错误处理 (RFC 7807 Problem Details)
  - 数据库连接池管理
  - 标准响应格式封装
  
- **logseek** (日志检索模块)
  - 分层架构：api、service、repository、utils
  - 支持本地文件系统和 S3/MinIO
  - NDJSON 流式搜索
  - 自然语言转查询（基于本地 Ollama）

### 前端 (`ui/`)

- **SvelteKit** SPA (使用 adapter-static)
- **模块化架构** (`src/lib/modules/logseek/`)
  - types/: TypeScript 类型定义
  - api/: 后端 API 客户端封装
  - utils/: 文本处理工具
  - composables/: Svelte 5 Runes 状态管理
- **Vite** 开发服务器（代理 /api 到后端）

## 快速开始

### 环境要求

- **Rust**: 1.90.0 (通过 rust-toolchain.toml 固定)
- **Node.js**: 20 (使用 nvm: `nvm use 20`)
- **pnpm**: 通过 corepack 启用

### 安装依赖

```bash
# 前端依赖
corepack enable
corepack prepare pnpm@latest --activate
pnpm --dir ui install
```

### 启动开发服务器

```bash
# 后端（终端1）
cargo run --manifest-path server/Cargo.toml -p api-gateway

# 前端（终端2）
pnpm --dir ui dev
```

访问：http://localhost:5173

### 生产构建

```bash
# 构建前端（输出到 server/api-gateway/static）
node scripts/build-frontend.mjs

# 构建后端
cargo build --manifest-path server/Cargo.toml -p api-gateway --release
```

## 主要功能

### 日志搜索

- GitHub 风格的查询语法（AND/OR/NOT、正则、短语）
- 本地文件系统和 S3/MinIO 支持
- NDJSON 流式结果返回
- 上下文窗口和关键词高亮

### MinIO 设置

- 通过 Web UI 配置 MinIO 连接
- 设置持久化到 SQLite 数据库
- 连接验证和错误提示

### AI 查询生成

- 将自然语言转换为查询字符串
- 依赖本地 Ollama (默认 http://127.0.0.1:11434)
- 默认模型：qwen3:8b
- 环境变量配置：`OLLAMA_BASE_URL`、`OLLAMA_MODEL`

## 配置

### 数据库

- 默认：`./opsbox.db`
- 覆盖：`--database-url` 或 `DATABASE_URL` 环境变量

### 日志级别

- `--log-level error|warn|info|debug|trace`
- 或使用 `-V`/`-VV`/`-VVV`
- 或设置 `RUST_LOG` 环境变量

### 守护进程（macOS/Linux）

```bash
# 启动守护进程
cargo run -p api-gateway -- start --daemon

# 停止守护进程
cargo run -p api-gateway -- stop
```

## 开发文档

- **项目指南**: `WARP.md` - WARP AI 开发指南
- **前端开发**: `docs/FRONTEND_DEVELOPMENT.md` - 前端模块化架构使用
- **重构进度**: `docs/REFACTORING_PROGRESS.md` - 重构历史和进度
- **阶段总结**: `docs/PHASE4_SUMMARY.md` - 阶段4完成总结

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
pnpm --dir ui format

# Lint
pnpm --dir ui lint

# 类型检查
pnpm --dir ui check

# 测试
pnpm --dir ui test
```

## 技术栈

### 后端

- Rust 1.90.0
- Axum (HTTP 框架)
- SQLite + sqlx (数据库)
- tokio (异步运行时)
- ollama-rs (AI 集成)

### 前端

- SvelteKit (框架)
- Svelte 5 (UI 库，Runes API)
- TypeScript (类型系统)
- TailwindCSS (样式)
- Vite (构建工具)

## License

MIT

