# OpsBox E2E 测试断言收紧

## What This Is

系统性地收紧 OpsBox 前端 E2E 测试断言，从"能通过"升级为"能发现问题"。同时补充错误处理、加载状态、边界情况和无障碍访问的新测试用例。

## Core Value

E2E 测试断言必须在功能真正损坏时失败，而不是仅仅检查页面渲染了。

## Requirements

### Validated

- ✓ E2E 测试框架搭建 (Playwright) — existing
- ✓ 基础搜索功能测试 — existing
- ✓ 集成测试（本地/混合/Agent）— existing
- ✓ Explorer 测试 — existing
- ✓ 设置页面测试 — existing

### Active

- [ ] 收紧 `search.spec.ts` — 修复 `\d+` 正则匹配，验证空状态为 `0 个结果`
- [ ] 收紧 `search_ux.spec.ts` — 移除嵌套条件，检查具体高亮文本和文件路径
- [ ] 收紧 `settings.spec.ts` — 替换 `body` 可见性检查为具体 UI 元素验证
- [ ] 收紧 `integration_explorer.spec.ts` — 完善下载测试，验证响应体字段
- [ ] 收紧其余集成测试 — 响应体验证、具体值匹配
- [ ] 添加错误处理测试 — 500 错误提示、API 失败、网络超时
- [ ] 添加加载状态测试 — skeleton、spinner、进度指示
- [ ] 添加边界情况测试 — 空数据、超长输入、特殊字符
- [ ] 添加无障碍测试 — 键盘导航、ARIA 属性、焦点管理

### Out of Scope

- 后端 Rust 单元测试 — 本次只关注前端 E2E
- 性能基准测试 — 已有 `integration_performance.spec.ts`
- 视觉回归测试 — 需要额外工具（如 Percy）

## Context

- **技术栈**: Playwright 1.57, SvelteKit, TypeScript
- **测试文件位置**: `web/tests/e2e/`
- **运行命令**: `pnpm --dir web test:e2e`
- **现有测试数量**: 18 个 spec 文件
- **问题**: 多个测试使用 `toBeTruthy()`, `\d+` 正则, `body` 可见性等松散断言

## Constraints

- **测试稳定性**: 收紧断言不能引入 flaky tests
- **执行时间**: 新测试不应显著增加总执行时间
- **测试数据**: 依赖后端启动和测试数据准备

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| 先收紧后添加 | 避免在松散基础上叠加新测试 | — Pending |
| 全部文件一起处理 | 确保一致性，避免遗漏 | — Pending |
| 全面收紧 | 松散断言是当前主要问题 | — Pending |

---
*Last updated: 2026-03-14 after initialization*
