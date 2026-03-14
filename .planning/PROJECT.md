# OpsBox E2E 测试断言收紧

## What This Is

系统性地收紧 OpsBox 前端 E2E 测试断言，从"能通过"升级为"能发现问题"。补充错误处理、加载状态、边界情况和无障碍访问的新测试用例。v1.0 已完成所有 18 个需求。

## Core Value

E2E 测试断言必须在功能真正损坏时失败，而不是仅仅检查页面渲染了。

## Requirements

### Validated

- ✓ E2E 测试框架搭建 (Playwright) — existing
- ✓ 基础搜索功能测试 — existing
- ✓ 集成测试（本地/混合/Agent）— existing
- ✓ Explorer 测试 — existing
- ✓ 设置页面测试 — existing
- ✓ 收紧 `search.spec.ts` — v1.0 (修复 `\d+` 正则匹配，验证空状态为 `0 个结果`)
- ✓ 收紧 `search_ux.spec.ts` — v1.0 (移除嵌套条件，检查具体高亮文本和文件路径)
- ✓ 收紧 `settings.spec.ts` — v1.0 (替换 `body` 可见性检查为具体 UI 元素验证)
- ✓ 收紧 `integration_explorer.spec.ts` — v1.0 (完善下载测试，验证响应体字段)
- ✓ 添加错误处理测试 — v1.0 (500 错误提示、API 失败、网络超时)
- ✓ 添加加载状态测试 — v1.0 (spinner、spinner-to-content 过渡)
- ✓ 添加边界情况测试 — v1.0 (空数据、超长输入、特殊字符 XSS)
- ✓ 添加无障碍测试 — v1.0 (键盘导航、ARIA 属性、焦点管理)

### Active

(All requirements shipped in v1.0. Next milestone requirements will be defined via `/gsd:new-milestone`)

### Out of Scope

- 后端 Rust 单元测试 — 本次只关注前端 E2E
- 性能基准测试 — 已有 `integration_performance.spec.ts`
- 视觉回归测试 — 需要额外工具（如 Percy）

## Context

- **技术栈**: Playwright 1.57, SvelteKit, TypeScript
- **测试文件位置**: `web/tests/e2e/`
- **运行命令**: `pnpm --dir web test:e2e`
- **测试文件数量**: 22 个 spec 文件 (was 18)
- **新测试**: 14 个新测试用例 (error_handling: 4, loading_states: 3, edge_cases: 4, accessibility: 3)
- **收紧测试**: 42 个断言在 4 个文件中被收紧
- **代码规模**: 6,330 行 E2E 测试代码
- **已解决**: 所有松散断言已替换为具体元素检查

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| 先收紧后添加 | 避免在松散基础上叠加新测试 | ✓ Good — 有效防止测试质量问题 |
| NDJSON mock 格式 | useStreamReader 期望 { type: 'result', data: {...} } | ✓ Good — 确保 mock 与真实 API 行为一致 |
| XSS 断言分开检查 | highlight() 将关键词包在 <mark> 标签中 | ✓ Good — 适应实际 DOM 渲染行为 |
| A11Y-03 分两次导航 | 避免 mock route 冲突 | ✓ Good — 确保测试隔离性 |
| 用 waitForFunction 替代 waitForTimeout | 事件驱动而非固定延迟 | ✓ Good — 避免 flaky tests |
| 全部文件一起处理 | 确保一致性，避免遗漏 | ✓ Good — 一次性完成所有收紧工作 |

## Constraints

- **测试稳定性**: 收紧断言不能引入 flaky tests ✓ 遵守
- **执行时间**: 新测试不应显著增加总执行时间 ✓ 遵守
- **测试数据**: 依赖后端启动和测试数据准备 — 使用 page.route() mock 避免后端依赖

---
*Last updated: 2026-03-14 after v1.0 milestone completion*
