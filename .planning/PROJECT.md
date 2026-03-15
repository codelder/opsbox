# OpsBox E2E 测试补充

## What This Is

在 v1.0 收紧断言的基础上，全面补充 E2E 测试覆盖缺口。目标：Settings CRUD 操作、View/Prompt 页面直接测试、Image Viewer 鼠标交互、Search/Explorer 交互细节、AI 模式流程、主题持久化。

## Core Value

E2E 测试必须覆盖最终用户的所有关键操作路径，确保功能在回归时被发现。

## Current Milestone: v1.1 全面补充测试覆盖

**Goal:** 补充所有已识别的 E2E 测试缺口，从 ~60% 综合覆盖提升到 ~95%

**Target features:**
- Settings CRUD 操作（S3 Profile、LLM 后端、规划脚本的新建/编辑/删除）
- View 页面直接测试（字体调节、下载、键盘快捷键、虚拟滚动）
- Prompt 页面 Markdown 渲染测试
- Image Viewer 鼠标交互（拖拽平移、滚轮缩放、缩略图）
- Search/Explorer 交互细节（复制路径、展开/折叠、悬停、拖拽调整）
- AI 模式完整流程测试
- 主题持久化跨页面测试

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

- [ ] Settings CRUD 操作测试覆盖
- [ ] View 页面直接测试
- [ ] Prompt 页面测试
- [ ] Image Viewer 鼠标交互测试
- [ ] Search 交互细节测试
- [ ] Explorer 交互细节测试
- [ ] AI 模式完整流程测试
- [ ] 主题持久化测试

### Out of Scope

- 后端 Rust 单元测试 — 本次只关注前端 E2E
- 性能基准测试 — 已有 `integration_performance.spec.ts`
- 视觉回归测试 — 需要额外工具（如 Percy）
- 移动端响应式布局测试 — v1.0 已排除

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
*Last updated: 2026-03-15 after v1.1 milestone start*
