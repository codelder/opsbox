# Phase 1 Context: Production Stability (止血)

**Phase Goal:** 消除搜索路径 panic 点，修复 mutex 中毒 DoS 风险

**Decided:** 2026-03-13

## Unwrap 替换策略

### 分类规则

| 类型 | 处理方式 | 示例 |
|------|----------|------|
| 不可失败 | `expect("infallible: reason")` | `vec.first().expect("infallible: search results non-empty")` |
| 有默认值 | `warn!` + `unwrap_or_default()` | `config.parse().unwrap_or_else(\|e\| { warn!(...); default })` |
| 错误路径 | `?` 传播 | IO 失败、通道关闭、锁获取失败 |

### 优先级

1. `search_executor.rs` (175 个) — 核心搜索路径，最高优先级
2. `search.rs` (82 个) — 核心搜索路径
3. 其他文件暂不处理（Phase 1 只覆盖搜索路径）

### 禁止事项

- 不能用 `.expect("unwrap")` — 必须有描述性消息
- 不能在循环内 `.expect()` — 应该用 `?` 或 `continue`
- 不能静默吞掉错误 — 至少 `warn!` 日志

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

- `backend/logseek/src/service/search_executor.rs` — 175 个 unwrap，2942 行（60% 测试）
- `backend/logseek/src/service/search.rs` — 82 个 unwrap，2152 行（87% 测试）
- `backend/agent/src/routes.rs` — HTTP handler mutex 中毒风险
- `backend/logseek/tests/boundary_integration.rs` — 5 个 stub 测试
- `backend/logseek/tests/s3_integration.rs` — 跳过的 S3 测试

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

1. search_executor.rs 和 search.rs 生产代码路径零 unwrap
2. HTTP handler mutex 操作可从中毒恢复
3. 5 个边界测试有真实断言
4. S3 测试有真实断言或被移除并记录
