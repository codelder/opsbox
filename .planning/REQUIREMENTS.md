# Requirements: OpsBox 平台改进

**Defined:** 2026-03-13
**Core Value:** 搜索性能和系统可靠性 — 搜索是核心功能，必须快速、稳定、可靠

## v1 Requirements

### 阶段 1: 止血 — 生产稳定性

<!-- SAFE-01 已取消：研究发现搜索路径生产代码已 panic-safe，175+82 个 unwrap 都在测试代码中 -->

- [ ] **SAFE-02**: HTTP handler 中的 `mutex.lock().unwrap()` 修复，防止 mutex 中毒导致 DoS
- [ ] **SAFE-03**: 边界测试 (boundary_integration.rs) 中 5 个 stub 测试实现真实断言
- [ ] **SAFE-04**: S3 集成测试 (s3_integration.rs) 跳过的测试实现（使用 mock）

### 阶段 2: 结构改进

- [ ] **STRC-01**: search_executor.rs 内联测试提取到独立测试文件 (2942→~383 行)
- [ ] **STRC-02**: search.rs 内联测试提取到独立测试文件 (2152→~861 行)
- [ ] **STRC-03**: S3 客户端缓存从 `Mutex<HashMap>` 迁移到 `DashMap`，消除争用

### 阶段 3: 性能优化

- [ ] **PERF-01**: 使用 `cargo-flamegraph` 建立性能基线，识别真实瓶颈
- [ ] **PERF-02**: 搜索路径 `.clone()` 减少，共享字符串迁移到 `Arc<str>`
- [ ] **PERF-03**: 重复查询编译缓存实现 (目标 50-90% 加速)
- [ ] **PERF-04**: SQLite 写批处理优化 (目标 5-10x 提升)

### 阶段 4: 前端覆盖 (14.85% → 70%)

- [ ] **FE-01**: 大路由组件 (1104/757 行) 拆分，提取业务逻辑到可测试模块
- [ ] **FE-02**: API clients 测试覆盖提升至 80%+
- [ ] **FE-03**: Composables 测试覆盖提升至 70%+
- [ ] **FE-04**: 消除前端代码中的 `as any` 强制转换，提升类型安全

## v2 Requirements

### 待定改进

- **QUAL-01**: 模块提取 — query_qualifiers.rs, result_handler.rs, grep_search.rs (依赖阶段 2 完成)
- **QUAL-02**: 添加 `clippy::unwrap_used` lint 配置 (依赖阶段 1 完成)
- **QUAL-03**: 统一 async mutex 策略 (parking_lot vs tokio::sync)

## Out of Scope

| Feature | Reason |
|---------|--------|
| 认证/授权系统 | 本次聚焦质量，安全加固作为单独里程碑 |
| SQLite 迁移到其他数据库 | 迁移成本高，优化并发模型即可 |
| 前端框架迁移 | SvelteKit 已满足需求 |
| 实时功能 (WebSocket) | 不影响搜索性能核心目标 |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| ~~SAFE-01~~ | ~~Phase 1~~ | **Cancelled** — search path already panic-safe |
| SAFE-02 | Phase 1 | Pending |
| SAFE-03 | Phase 1 | Pending |
| SAFE-04 | Phase 1 | Pending |
| STRC-01 | Phase 2 | Pending |
| STRC-02 | Phase 2 | Pending |
| STRC-03 | Phase 2 | Pending |
| PERF-01 | Phase 3 | Pending |
| PERF-02 | Phase 3 | Pending |
| PERF-03 | Phase 3 | Pending |
| PERF-04 | Phase 3 | Pending |
| FE-01 | Phase 4 | Pending |
| FE-02 | Phase 4 | Pending |
| FE-03 | Phase 4 | Pending |
| FE-04 | Phase 4 | Pending |

**Coverage:**
- v1 requirements: 14 total (1 cancelled after research)
- Mapped to phases: 14
- Unmapped: 0 ✓

---
*Requirements defined: 2026-03-13*
*Last updated: 2026-03-13 after initial definition*
