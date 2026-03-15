# Milestones

## v1.0 — E2E 测试断言收紧

**Shipped:** 2026-03-14
**Phases:** 6 | **Plans:** 6 | **Requirements:** 18

### Delivered

系统性地收紧 OpsBox 前端 E2E 测试断言，从"能通过"升级为"能发现问题"。同时补充错误处理、加载状态、边界情况和无障碍访问的新测试用例。

### Key Accomplishments

1. **Tightened 42 E2E test assertions** — Replaced all loose `body` visibility checks, `\d+` regex patterns, and conditional `if (count > 0)` wrappers with specific element assertions
2. **Created 4 error handling tests** — API 500 errors, network timeouts, error details expand/collapse, search cancellation
3. **Created 3 loading state tests** — Search spinner, spinner-to-content transitions, Explorer loading states
4. **Created 7 edge case and accessibility tests** — Empty results, long queries, XSS protection, empty directories, keyboard navigation, ARIA attributes, focus management
5. **Established test patterns** — NDJSON mock format, `waitForFunction` over `waitForTimeout`, `page.route()` with delays

### Stats

- **Files modified:** 31 files (+3,907 / -165 lines)
- **Test files:** 4 tightened, 4 new files created
- **Timeline:** 1 day (2026-03-14)
- **Git range:** `f92c0f8..f0a18f1`
- **Test LOC:** 6,330 lines in E2E spec files

### Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| 先收紧后添加 | 避免在松散基础上叠加新测试 | ✓ Applied across phases 1-6 |
| NDJSON mock 格式 | useStreamReader 期望 { type: 'result', data: {...} } | ✓ Applied in EDGE-03 |
| XSS 断言分开检查 | highlight() 将关键词包在 <mark> 标签中 | ✓ Applied in EDGE-03 |
| A11Y-03 分两次导航 | 避免 mock route 冲突 | ✓ Applied in accessibility.spec.ts |
| 用 waitForFunction 替代 waitForTimeout | 事件驱动而非固定延迟，避免 flaky tests | ✓ Applied in loading_states.spec.ts |

### Known Gaps

None — all 18 requirements completed.

---

_Archive details: `.planning/milestones/v1.0-ROADMAP.md`, `.planning/milestones/v1.0-REQUIREMENTS.md`_
