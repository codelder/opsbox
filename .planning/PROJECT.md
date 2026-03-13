# OpsBox 平台改进

## What This Is

OpsBox 是一个模块化的日志搜索和分析平台，基于 Rust 后端和 SvelteKit 前端。平台支持跨本地文件、S3/MinIO 存储和远程 Agent 的统一资源浏览，具备 DFS 子系统和 ORL 协议。本次改进旨在全面提升代码质量、前端覆盖率和搜索性能。

## Core Value

**搜索性能和系统可靠性** — 搜索是核心功能，必须快速、稳定、可靠。代码质量改进和前端覆盖都是为了支撑这个核心。

## Requirements

### Validated

- ✓ 模块化单体架构，基于 `inventory` crate 的编译时模块发现 — existing
- ✓ 分层架构 (API → Service → Repository → Domain) — existing
- ✓ DFS 子系统统一 Local/S3/Agent 资源访问 — existing
- ✓ ORL 协议统一资源标识 — existing
- ✓ 嵌入式 SPA 前端 (SvelteKit + TailwindCSS 4.0) — existing
- ✓ Starlark 脚本化源规划 — existing
- ✓ NL2Q 自然语言转查询 — existing
- ✓ 1,031 个后端测试 (99.7% 通过率) — existing

### Active

- [ ] 全面清理 `.unwrap()` — 所有生产代码中的 unwrap 替换为适当错误处理
- [ ] 搜索性能优化 — 减少 clone、优化内存分配、提升并发效率
- [ ] 前端覆盖率从 14.85% 提升到 70%
- [ ] 实现 stub 测试 — 补充边界测试和安全测试的实际断言
- [ ] 重构过大文件 — search_executor.rs (2942 行)、search.rs (2152 行)
- [ ] SQLite 写瓶颈优化 — 评估并发写入策略
- [ ] 类型安全改进 — 消除前端 `as any` 强制转换

### Out of Scope

- 添加认证/授权系统 — 本次聚焦质量，安全加固作为单独里程碑
- 实时功能 (WebSocket) — 不影响搜索性能核心目标
- 移动端适配 — 桌面端优先

## Context

**技术环境：**
- Rust 2024 edition, Tokio async runtime, mimalloc allocator
- SvelteKit 2.22 + Svelte 5 Runes, TailwindCSS 4.0, Vite 7.0
- SQLite 单文件数据库，所有模块共享
- 7 个 Rust workspace members + 前端 SPA

**当前痛点（来自代码库分析）：**
- `search_executor.rs` 有 175 个 `.unwrap()`，核心搜索路径存在 panic 风险
- 前端覆盖率仅 14.85%，大量路由组件未经测试
- 搜索路径中 303 个 `.clone()` 调用，潜在性能损失
- 多个 stub 测试只有 TODO 标记，没有实际断言
- `async-tar` 和 `tokio-tar` 两个竞争库共存

## Constraints

- **技术栈**：必须保持 Rust + SvelteKit 架构，不引入新语言
- **兼容性**：API 接口不能破坏性变更，保持现有模块注册机制
- **测试要求**：改进后测试通过率不能低于 99%
- **性能**：搜索延迟不能比当前版本增加

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| 全面清理 unwrap | 核心搜索路径 175 个 unwrap 是生产事故隐患 | — Pending |
| 前端全覆盖策略 | 14.85% → 70% 需要系统性投入，非零散补充 | — Pending |
| 搜索性能优先 | 核心功能，用户最直接感知 | — Pending |
| 保持 SQLite | 迁移成本高，优化策略是调整并发模型 | — Pending |

---
*Last updated: 2026-03-13 after initialization*
