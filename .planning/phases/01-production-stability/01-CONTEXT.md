# Phase 1 Context: Production Stability (止血)

**Phase Goal:** 修复 mutex 中毒 DoS 风险，实现真实集成测试断言

**Decided:** 2026-03-13

## 研究发现：SAFE-01 已取消

研究确认：搜索路径生产代码（search_executor.rs 1-384 行，search.rs 1-861 行）**已经是 panic-safe 的**。175 + 82 个 unwrap 全部在 `#[cfg(test)]` 测试代码中，这是 Rust 惯用写法。

Phase 1 聚焦于：
- SAFE-02: mutex 中毒修复（**真实风险**）
- SAFE-03: 边界测试实现
- SAFE-04: S3 测试实现

## Mutex 恢复模式

### 应用场景

| 场景 | 模式 | 文件 |
|------|------|------|
| 同步代码 (daemon, init) | `parking_lot::Mutex` | `daemon.rs`, `daemon_windows.rs`, `main.rs` |
| HTTP handler | `tokio::sync::Mutex` | `agent/src/routes.rs` |
| 全局缓存 | **Phase 2 处理** (DashMap) | `opsbox-core/src/storage/s3.rs` |

### 迁移步骤

1. 添加 `parking_lot` 依赖（如不存在）
2. 同步 mutex: `std::sync::Mutex` → `parking_lot::Mutex`（消除中毒）
3. HTTP handler: `std::sync::Mutex` → `tokio::sync::Mutex` + `.lock().await`

### 网络初始化 (network.rs)

`ENV_MUTEX` 和 `unsafe { env::set_var }` **不在 Phase 1 范围**。
原因：涉及全局环境变量修改，需要更仔细的设计。记录为后续改进。

## 边界测试深度

### 测试断言标准

| 测试 | 断言要求 |
|------|----------|
| `test_mixed_encoding_search` | 搜索返回结果 + 结果内容包含正确文本 |
| `test_malicious_orl_protection` | 实际尝试路径遍历 payload，验证被拦截或安全处理 |
| `test_concurrent_search_boundary` | 10+ 并发搜索，验证结果完整性 + 无 panic |
| `test_permission_denied_scenarios` | 访问无权限路径，验证返回适当错误 |
| `test_large_file_boundary` | 搜索大文件，验证结果正确 + 内存可控 |

### 测试基础设施

- 使用 `test-common` crate 的 `create_test_file()` 等辅助函数
- 使用 `tempfile` 创建临时测试目录
- 搜索结果通过 `mpsc::Receiver<SearchEvent>` 收集

## S3 测试决策

### 实现方式

- 使用 mock 实现（不依赖真实 AWS）
- 可以使用 `test-common` 的 mock server 基础设施
- 测试 S3 profile 管理 API（CRUD 操作）
- 测试 S3 搜索流程（mock S3 返回预设数据）

### 范围

- 实现 `test_s3_api_endpoints` 的真实断言
- 测试 profile 创建/列表/删除
- 测试 S3 搜索路径（使用 mock）

## Code Context

### 关键文件

- `backend/agent/src/routes.rs` — HTTP handler mutex 中毒风险（lines 108, 137）
- `backend/opsbox-server/src/network.rs` — ENV_MUTEX（**不在 Phase 1 范围**）
- `backend/logseek/tests/boundary_integration.rs` — 5 个 stub 测试
- `backend/logseek/tests/s3_integration.rs` — 跳过的 S3 测试
- `backend/test-common/` — Mock 基础设施（MockS3Server, TestFileGenerator, TestDatabase）

### 现有模式

- 错误类型：`ServiceError` (logseek), `AppError` (opsbox-core)
- 测试模式：`#[tokio::test]`, `test-common` 辅助函数
- 异步模式：`tokio::sync::mpsc`, `Semaphore`

### 依赖

- `parking_lot` — 可能需要添加到 Cargo.toml
- `tokio::sync::Mutex` — 已有 tokio 依赖

## Deferred Ideas

以下想法超出 Phase 1 范围，记录供后续参考：

- network.rs `ENV_MUTEX` 和 unsafe env 操作重构
- 添加 `clippy::unwrap_used` lint（Phase 1 完成后）
- 统一全项目的 mutex 策略
- shutdown timeout 可配置化

## Success Criteria

Phase 1 完成时必须满足：

1. HTTP handler mutex 使用 `tokio::sync::Mutex`（异步）或 `parking_lot::Mutex`（同步）— 不会发生中毒级联
2. 5 个边界测试有真实断言（编码、路径遍历、并发、权限、大文件）
3. S3 测试使用 MockS3Server 实现真实断言
