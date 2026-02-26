# 测试覆盖率改进方案设计文档

**文档版本**: 1.0
**创建日期**: 2026-02-26
**作者**: Claude Code
**状态**: 待实施

---

## 执行摘要

本文档描述了 OpsBox 项目测试覆盖率的渐进式改进方案，旨在 6-8 周内系统性提升测试质量，重点解决高风险区域的测试缺失问题。

### 核心目标

- **Explorer 模块**: 从 60 分提升到 85 分
- **DFS 模块**: 从 50 分提升到 80 分
- **前端测试**: 从 30 分提升到 70 分
- **后端整体覆盖率**: 从 ~60% 提升到 ~75%
- **前端整体覆盖率**: 从 ~1% 提升到 ~60%

---

## 1. 背景和问题分析

### 1.1 当前测试分布

```
测试金字塔分布（总计 ~1221 个测试用例）
├── E2E 测试: 217 个 (~18%)
├── 集成测试: 125 个 (~10%)
└── 单元测试: 879 个 (~72%)
    ├── 后端: 854 个 (492 同步 + 362 异步)
    └── 前端: 79 个
```

### 1.2 关键问题

#### 高优先级问题

1. **前端单元测试严重不足**
   - 覆盖率仅 6.5% (5/31 源文件)
   - 大量业务逻辑未测试
   - 过度依赖 E2E 测试

2. **关键模块缺少后端集成测试**
   - Explorer: 0 个集成测试文件
   - DFS: 0 个集成测试文件
   - 依赖 E2E 测试验证后端逻辑

#### 中优先级问题

3. **边界测试未完成**
   - 5 个 TODO 标记的测试未实现
   - 混合编码、并发搜索、大文件测试缺失

4. **测试环境依赖**
   - NL2Q 和部分 View 测试需要 `network-tests` feature
   - 可能在 CI 中被跳过

### 1.3 风险评估

| 核心功能 | 单元测试 | 集成测试 | E2E 测试 | 总体评分 |
|---------|---------|---------|---------|---------|
| LogSeek 搜索 | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | **95/100** ✅ |
| Explorer 浏览 | ⭐⭐⭐ | ❌ | ⭐⭐⭐⭐⭐ | **60/100** ⚠️ |
| Agent Manager | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | **85/100** ✅ |
| DFS 文件系统 | ⭐⭐⭐⭐ | ❌ | N/A | **50/100** ⚠️ |
| 前端业务逻辑 | ⭐ | N/A | ⭐⭐⭐⭐ | **30/100** ❌ |
| 安全防护 | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | N/A | **90/100** ✅ |

---

## 2. 改进方案概述

### 2.1 方案选择：渐进式改进

**核心理念**: 平衡成本和效果，按风险优先级渐进式提升测试覆盖

**实施原则**:
- ✅ **风险驱动**: 优先测试高风险、高价值的代码路径
- ✅ **渐进交付**: 每个迭代独立交付，快速反馈
- ✅ **实用主义**: 不追求 100% 覆盖，关注关键场景
- ✅ **可维护性**: 建立长期机制，便于持续改进

### 2.2 整体时间表

```
总周期: 6-8 周

迭代 1 (2-3 周) → 迭代 2 (2-3 周) → 迭代 3 (2 周)
   ↓                  ↓                  ↓
高风险修复         中风险补充          优化完善
 25 个测试          35 个测试          15 个测试
```

### 2.3 预期成果

| 维度 | 改进前 | 改进后 | 提升 |
|------|--------|--------|------|
| 后端覆盖率 | ~60% | ~75% | +15% |
| 前端覆盖率 | ~1% | ~60% | +59% |
| 测试用例总数 | ~979 | ~1054 | +75 |
| Explorer 评分 | 60 | 85 | +25 |
| DFS 评分 | 50 | 80 | +30 |
| 前端评分 | 30 | 70 | +40 |

---

## 3. 迭代 1: 高风险区域修复（2-3 周）

### 3.1 目标

修复最严重的测试缺失，优先覆盖关键业务路径和用户高频使用场景。

### 3.2 具体任务

#### 任务 1.1: Explorer 后端集成测试（优先级 P0）

**创建文件**: `backend/explorer/tests/integration_test.rs`

**测试场景**:

```rust
// 本地文件浏览
- test_list_local_directory_with_files
- test_list_local_empty_directory
- test_list_local_with_permission_denied

// Agent 文件浏览（需要 Mock Agent）
- test_list_agent_files_success
- test_list_agent_with_offline_agent
- test_list_agent_with_network_error

// 归档导航
- test_navigate_tar_archive
- test_navigate_tar_gz_archive
- test_navigate_nested_archive

// 文件下载
- test_download_local_file
- test_download_agent_file
- test_download_archive_entry
```

**工作量**: 8-10 个测试用例，约 15-20 小时

---

#### 任务 1.2: DFS 跨系统集成测试（优先级 P0）

**创建文件**: `backend/opsbox-core/tests/dfs_integration_test.rs`

**测试场景**:

```rust
// S3 + Archive 组合
- test_s3_archive_tar_read
- test_s3_archive_zip_read
- test_s3_archive_nested

// Agent + Archive 组合
- test_agent_archive_tar_read
- test_agent_archive_with_mock
```

**工作量**: 5-6 个测试用例，约 10-12 小时

---

#### 任务 1.3: 前端 API 客户端测试（优先级 P1）

**补充文件**:

1. `web/src/lib/modules/logseek/api/search.test.ts` (已存在，需补充)
2. `web/src/lib/modules/explorer/api.test.ts` (新建)
3. `web/src/lib/utils/orl.test.ts` (已存在，需补充)

**测试场景**:

```typescript
// logseek/api/search.test.ts
- test_build_search_request
- test_parse_search_response
- test_handle_search_error

// explorer/api.test.ts
- test_build_list_request
- test_parse_list_response
- test_build_download_url

// utils/orl.test.ts
- test_parse_orl_with_archive
- test_build_orl_for_s3
- test_build_orl_for_agent
```

**工作量**: 10-12 个测试用例，约 12-15 小时

---

### 3.3 交付物

- ✅ Explorer 集成测试文件 (~300 行代码)
- ✅ DFS 集成测试文件 (~200 行代码)
- ✅ 前端 API 测试补充 (~150 行代码)
- ✅ 总计新增测试用例: ~25 个

### 3.4 验收标准

```
✅ 功能验收
   [ ] 所有新测试通过
   [ ] 覆盖关键用户路径
   [ ] CI 构建成功

✅ 覆盖率验收
   [ ] Explorer 后端覆盖率 ≥ 50%
   [ ] DFS 后端覆盖率 ≥ 60%
   [ ] 前端覆盖率 ≥ 30%
```

---

## 4. 迭代 2: 中风险区域补充（2-3 周）

### 4.1 目标

补充中等风险的测试场景，完善现有测试套件，提升测试深度。

### 4.2 具体任务

#### 任务 2.1: Explorer 完整测试套件（优先级 P1）

**扩展文件**: `backend/explorer/tests/`

**测试场景**:

```rust
// S3 文件浏览（需要 Mock S3）
- test_list_s3_buckets
- test_list_s3_bucket_contents
- test_list_s3_with_empty_bucket
- test_list_s3_with_invalid_credentials

// 错误场景测试
- test_handle_malformed_orl
- test_handle_nonexistent_path
- test_handle_permission_denied
- test_handle_network_timeout

// 性能测试
- test_list_large_directory_performance
- test_concurrent_list_operations
```

**工作量**: 10-12 个测试用例，约 15-18 小时

---

#### 任务 2.2: DFS 完整测试套件（优先级 P1）

**扩展文件**: `backend/opsbox-core/tests/dfs_integration_test.rs`

**测试场景**:

```rust
// Local + Archive 组合
- test_local_archive_with_multiple_entries
- test_local_archive_with_large_file
- test_local_archive_corrupted

// ORL 边界测试
- test_orl_with_special_characters
- test_orl_with_unicode
- test_orl_with_very_long_path
- test_orl_with_invalid_encoding

// 并发访问测试
- test_concurrent_read_operations
- test_concurrent_archive_access
```

**工作量**: 8-10 个测试用例，约 12-15 小时

---

#### 任务 2.3: 前端工具函数测试（优先级 P2）

**新建/补充文件**:

1. `web/src/lib/modules/logseek/utils/highlight.test.ts` (已存在，需补充)
2. `web/src/lib/modules/logseek/utils/query-builder.test.ts` (新建)
3. `web/src/lib/modules/explorer/utils.test.ts` (新建)

**测试场景**:

```typescript
// highlight.test.ts
- test_highlight_with_chinese_characters
- test_highlight_with_regex_pattern
- test_highlight_performance_large_text

// query-builder.test.ts
- test_build_simple_query
- test_build_query_with_filters
- test_build_query_with_date_range

// explorer/utils.test.ts
- test_format_file_size
- test_parse_file_type
- test_build_file_path
```

**工作量**: 12-15 个测试用例，约 15-18 小时

---

### 4.3 交付物

- ✅ Explorer S3 和错误场景测试 (~400 行代码)
- ✅ DFS 边界和并发测试 (~300 行代码)
- ✅ 前端工具函数测试 (~200 行代码)
- ✅ 总计新增测试用例: ~35 个

### 4.4 验收标准

```
✅ 功能验收
   [ ] 所有新测试通过
   [ ] 错误场景覆盖率达到 80%
   [ ] 性能测试建立基准

✅ 覆盖率验收
   [ ] Explorer 后端覆盖率 ≥ 65%
   [ ] DFS 后端覆盖率 ≥ 70%
   [ ] 前端覆盖率 ≥ 50%
```

---

## 5. 迭代 3: 优化和完善（2 周）

### 5.1 目标

补全边界测试场景，建立测试覆盖率监控机制，优化 E2E 测试稳定性。

### 5.2 具体任务

#### 任务 3.1: 边界测试 TODO 补全（优先级 P2）

**修改文件**: `backend/logseek/tests/boundary_integration.rs`

**完成的 TODO**:

```rust
// TODO #1: 混合编码搜索测试
- 实现实际搜索逻辑
- 验证 UTF-8 + GBK 混合文件搜索
- 测试 BOM 标记处理

// TODO #2: ORL 解析安全检查
- 实现恶意 ORL 验证逻辑
- 测试所有恶意模式被正确拒绝
- 验证错误消息清晰

// TODO #3: 并发搜索边界测试
- 实现并发搜索执行
- 验证资源竞争处理
- 测试内存管理

// TODO #4: 权限拒绝测试
- 创建无权限文件场景
- 验证错误处理
- 测试优雅降级

// TODO #5: 大文件搜索测试
- 生成 100MB+ 测试文件
- 验证分块读取
- 测试内存使用
```

**工作量**: 10-12 个测试用例，约 15-18 小时

---

#### 任务 3.2: 测试覆盖率监控机制（优先级 P1）

**后端覆盖率监控**:

创建 `.github/workflows/coverage.yml`:

```yaml
name: Test Coverage

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

jobs:
  backend-coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: llvm-tools-preview

      - name: Install cargo-llvm-cov
        uses: taiki-e/install-action@cargo-llvm-cov

      - name: Generate coverage report
        env:
          OPSBOX_NO_PROXY: 1
        run: |
          cargo llvm-cov --workspace --lcov --output-path lcov.info

      - name: Check coverage threshold
        run: |
          COVERAGE=$(cargo llvm-cov --workspace --summary-only 2>&1 | grep -oP '\d+\.\d+%' | head -1 | tr -d '%')
          echo "Coverage: $COVERAGE%"
          if (( $(echo "$COVERAGE < 70" | bc -l) )); then
            echo "❌ Coverage $COVERAGE% is below threshold 70%"
            exit 1
          fi
          echo "✅ Coverage $COVERAGE% meets threshold 70%"

      - name: Upload coverage report
        uses: actions/upload-artifact@v4
        with:
          name: backend-coverage
          path: lcov.info

      - name: Comment PR with coverage
        if: github.event_name == 'pull_request'
        uses: romeovs/lcov-reporter-action@v0.3.1
        with:
          lcov-file: lcov.info
          github-token: ${{ secrets.GITHUB_TOKEN }}

  frontend-coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'

      - name: Install pnpm
        uses: pnpm/action-setup@v2
        with:
          version: 10.23.0

      - name: Install dependencies
        run: pnpm --dir web install

      - name: Run tests with coverage
        run: pnpm --dir web test:unit --coverage

      - name: Upload coverage report
        uses: actions/upload-artifact@v4
        with:
          name: frontend-coverage
          path: web/coverage/

      - name: Comment PR with coverage
        if: github.event_name == 'pull_request'
        uses: davelosert/vitest-coverage-report-action@v2
```

**前端覆盖率配置**:

更新 `web/vitest.config.ts`:

```typescript
export default defineConfig({
  plugins: [sveltekit()],
  test: {
    include: ['src/**/*.{test,spec}.{js,ts}'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html', 'lcov'],
      reportsDirectory: './coverage',

      thresholds: {
        lines: 70,
        functions: 70,
        statements: 70,
        branches: 60,
        'src/lib/utils/**': 80,
        'src/lib/modules/**/api/**': 75,
      },

      include: [
        'src/lib/**/*.{ts,js}',
        'src/routes/**/*.{ts,js}',
      ],
      exclude: [
        'src/**/*.d.ts',
        'src/**/*.test.ts',
        'src/**/*.spec.ts',
        'src/app.html',
      ],
    },
  },
});
```

**工作量**: 约 8-10 小时

---

#### 任务 3.3: E2E 测试优化（优先级 P2）

**优化策略**:

1. **减少测试间依赖**
   - 每个测试独立准备数据
   - 避免共享状态

2. **优化等待策略**
   - 使用更智能的等待条件
   - 减少硬编码的 sleep

3. **改进错误诊断**
   - 增强失败时的截图和日志
   - 保存更多调试信息

4. **并行执行优化**
   - 识别可并行的测试
   - 减少测试总时长

**工作量**: 约 10-12 小时

---

### 5.3 交付物

- ✅ 边界测试补全 (~300 行代码)
- ✅ 测试覆盖率 CI 集成
- ✅ E2E 测试稳定性提升
- ✅ 总计新增测试用例: ~15 个

### 5.4 验收标准

```
✅ 功能验收
   [ ] 所有边界测试 TODO 完成
   [ ] E2E 测试成功率 ≥ 95%

✅ 覆盖率验收
   [ ] 后端整体覆盖率 ≥ 75%
   [ ] 前端整体覆盖率 ≥ 60%

✅ CI 验收
   [ ] CI 中显示覆盖率报告
   [ ] PR 中自动显示覆盖率变化
   [ ] 覆盖率低于阈值时 CI 失败
```

---

## 6. 成功指标和验收标准

### 6.1 整体成功指标

#### 测试覆盖率提升目标

| 模块 | 当前覆盖率 | 目标覆盖率 | 提升幅度 |
|------|-----------|-----------|---------|
| **后端整体** | ~60% | **75%** | +15% ⬆️ |
| LogSeek | ~70% | 80% | +10% |
| Explorer | ~40% | 70% | +30% ⬆️⬆️ |
| DFS | ~50% | 75% | +25% ⬆️⬆️ |
| Agent Manager | ~65% | 70% | +5% |
| **前端整体** | ~1% | **60%** | +59% ⬆️⬆️⬆️ |
| API 客户端 | ~5% | 75% | +70% ⬆️⬆️⬆️ |
| 工具函数 | ~10% | 80% | +70% ⬆️⬆️⬆️ |
| 组件 | ~0% | 50% | +50% ⬆️⬆️ |

#### 测试质量指标

```
✅ 测试金字塔比例
   - 单元测试：≥ 70%
   - 集成测试：≥ 15%
   - E2E 测试：≤ 15%

✅ 测试稳定性
   - E2E 测试成功率：≥ 95%
   - Flaky 测试率：≤ 2%

✅ 测试执行时间
   - 单元测试套件：≤ 30 秒
   - 集成测试套件：≤ 3 分钟
   - E2E 测试套件：≤ 10 分钟
```

#### 风险降低指标

| 风险类型 | 改进前 | 改进后 | 降低幅度 |
|---------|--------|--------|---------|
| **Explorer 后端缺陷** | 高风险 | 中等风险 | ⬇️ 60% |
| **DFS 跨系统缺陷** | 高风险 | 低风险 | ⬇️ 70% |
| **前端回归缺陷** | 高风险 | 中等风险 | ⬇️ 50% |
| **边界条件缺陷** | 中等风险 | 低风险 | ⬇️ 40% |

### 6.2 最终验收清单

**项目完成时必须满足以下所有条件：**

```
✅ 测试覆盖率
   [ ] 后端整体覆盖率 ≥ 75%
   [ ] 前端整体覆盖率 ≥ 60%
   [ ] 关键模块覆盖率达标（见目标表）

✅ 测试质量
   [ ] 新增测试用例总数 ≥ 75 个
   [ ] 测试金字塔比例合理
   [ ] E2E 测试成功率 ≥ 95%

✅ CI/CD
   [ ] 覆盖率监控正常运行
   [ ] PR 中显示覆盖率变化
   [ ] 覆盖率徽章显示在 README

✅ 文档
   [ ] 测试编写指南完成
   [ ] 测试最佳实践文档完成
   [ ] 覆盖率报告解读指南完成
```

---

## 7. 风险和缓解措施

### 7.1 潜在风险

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|---------|
| 时间估算过于乐观 | 中 | 中 | 预留 20% 缓冲时间，可调整范围 |
| 前端测试难度大 | 高 | 中 | 优先测试关键路径，降低组件测试要求 |
| Mock 环境不稳定 | 中 | 低 | 改进 test-common 模块，增加重试机制 |
| 团队测试经验不足 | 低 | 中 | 提供培训和结对编程 |

### 7.2 应急预案

**如果迭代 1 未按时完成**：
- 调整迭代 2 范围，降低 S3 测试优先级
- 延长迭代 1 时间 1 周

**如果前端覆盖率目标难以达成**：
- 降低组件测试要求（从 50% 降至 30%）
- 重点保证 API 客户端和工具函数的覆盖

**如果 CI 集成遇到问题**：
- 先手动运行覆盖率报告
- 后续再优化 CI 流程

---

## 8. 后续维护

### 8.1 持续改进机制

**每周覆盖率检查**：
- CI 自动生成覆盖率报告
- 低于阈值时发出警告
- PR 中强制显示覆盖率变化

**月度测试审查**：
- 审查新增代码的测试覆盖
- 识别测试盲区
- 更新测试优先级

**季度测试优化**：
- 优化慢测试
- 清理冗余测试
- 更新测试工具和框架

### 8.2 测试文化推广

**测试编写指南**：
- 提供测试模板和示例
- 最佳实践文档
- 常见问题解答

**代码审查要求**：
- 新代码必须包含测试
- PR 中显示覆盖率变化
- 测试质量作为审查标准之一

---

## 9. 参考资料

### 9.1 相关文档

- [CLAUDE.md](../../CLAUDE.md) - 项目概述和开发指南
- [Test Monitoring Guide](../testing/test-monitoring-guide.md) - 测试监控指南
- [Architecture](../architecture/architecture.md) - 系统架构文档

### 9.2 测试工具

- **后端**: cargo-llvm-cov, tokio-test, mockall
- **前端**: vitest, @vitest/coverage-v8, @playwright/test
- **Mock**: test-common 模块（agent_mock, s3_mock）

### 9.3 最佳实践

- Rust 测试最佳实践: https://doc.rust-lang.org/book/ch11-00-testing.html
- Vitest 文档: https://vitest.dev/
- Playwright 最佳实践: https://playwright.dev/docs/best-practices

---

## 附录 A: 测试用例清单

### 迭代 1 测试用例（25 个）

#### Explorer 集成测试（10 个）

1. test_list_local_directory_with_files
2. test_list_local_empty_directory
3. test_list_local_with_permission_denied
4. test_list_agent_files_success
5. test_list_agent_with_offline_agent
6. test_list_agent_with_network_error
7. test_navigate_tar_archive
8. test_navigate_tar_gz_archive
9. test_navigate_nested_archive
10. test_download_local_file

#### DFS 集成测试（5 个）

11. test_s3_archive_tar_read
12. test_s3_archive_zip_read
13. test_s3_archive_nested
14. test_agent_archive_tar_read
15. test_agent_archive_with_mock

#### 前端 API 测试（10 个）

16. test_build_search_request
17. test_parse_search_response
18. test_handle_search_error
19. test_build_list_request
20. test_parse_list_response
21. test_build_download_url
22. test_parse_orl_with_archive
23. test_build_orl_for_s3
24. test_build_orl_for_agent
25. test_orl_error_handling

### 迭代 2 测试用例（35 个）

#### Explorer 完整测试（12 个）

26. test_list_s3_buckets
27. test_list_s3_bucket_contents
28. test_list_s3_with_empty_bucket
29. test_list_s3_with_invalid_credentials
30. test_handle_malformed_orl
31. test_handle_nonexistent_path
32. test_handle_permission_denied
33. test_handle_network_timeout
34. test_list_large_directory_performance
35. test_concurrent_list_operations
36. test_download_agent_file
37. test_download_archive_entry

#### DFS 完整测试（10 个）

38. test_local_archive_with_multiple_entries
39. test_local_archive_with_large_file
40. test_local_archive_corrupted
41. test_orl_with_special_characters
42. test_orl_with_unicode
43. test_orl_with_very_long_path
44. test_orl_with_invalid_encoding
45. test_concurrent_read_operations
46. test_concurrent_archive_access
47. test_dfs_error_recovery

#### 前端工具测试（13 个）

48. test_highlight_with_chinese_characters
49. test_highlight_with_regex_pattern
50. test_highlight_performance_large_text
51. test_build_simple_query
52. test_build_query_with_filters
53. test_build_query_with_date_range
54. test_query_error_handling
55. test_format_file_size
56. test_parse_file_type
57. test_build_file_path
58. test_path_utils_edge_cases
59. test_date_parsing
60. test_string_utils

### 迭代 3 测试用例（15 个）

#### 边界测试补全（10 个）

61. test_mixed_encoding_search_implementation
62. test_utf8_gbk_mixed_files
63. test_bom_handling
64. test_malicious_orl_validation
65. test_orl_rejection_patterns
66. test_concurrent_search_execution
67. test_resource_contention
68. test_memory_management
69. test_permission_denied_scenarios
70. test_large_file_search_100mb

#### E2E 优化（5 个）

71. test_e2e_parallel_execution
72. test_e2e_independent_data_setup
73. test_e2e_smart_waits
74. test_e2e_enhanced_diagnostics
75. test_e2e_retry_mechanism

---

## 变更历史

| 版本 | 日期 | 作者 | 变更说明 |
|------|------|------|---------|
| 1.0 | 2026-02-26 | Claude Code | 初始版本 |

---

**文档结束**
