# Phase 5: 添加加载状态测试 - Research

**Researched:** 2026-03-14
**Domain:** Playwright E2E testing for loading state UI transitions
**Confidence:** HIGH

## Summary

Phase 5 creates E2E tests for loading state indicators across three pages: Search, Explorer, and View. The codebase uses a consistent `.animate-spin` CSS class pattern with lucide-svelte icons (`LoaderCircle` and `RefreshCw`) for loading feedback. Tests must mock API delays to ensure loading states are observable, as real backend responses may be too fast to catch spinner visibility.

**Primary recommendation:** Use `page.route()` with delayed `route.fulfill()` to create observable loading windows, combined with `page.waitForFunction()` to detect spinner appearance and disappearance.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **测试范围调整**: 代码库中无骨架屏实现，所有页面使用 spinner（`.animate-spin`）。将 LOAD-02 改为测试 "spinner 到内容过渡"
- **Loading Spinner 选择器**: 通用 spinner: `.animate-spin` CSS 类；搜索页: `LoaderCircle` 组件，文本 "搜索中..."（首次）或 "加载更多..."（后续）；Explorer: `RefreshCw` 图标带 `animate-spin` 类（条件渲染）；View 页: `LoaderCircle` + 文本 "加载中..."
- **状态转换验证**: 验证 loading 开始时 spinner 可见；验证 loading 结束后 spinner 消失；验证内容在 loading 完成后出现；使用 `page.waitForFunction()` 等待状态变化
- **搜索加载状态细节**: 输入框在 loading 期间 `disabled`；结果计数：loading 中显示 "搜索结果"，完成后显示 "X 个结果"；Load More 按钮：loading 时显示 spinner + "加载更多..."
- **Explorer 加载状态细节**: 刷新按钮的 `RefreshCw` 图标在 loading 时带 `animate-spin`；返回按钮在 loading 期间 `disabled`；空目录消息仅在 `!loading` 时显示

### Claude's Discretion
- 具体的等待超时时间
- 是否需要 mock API 延迟来确保观察到 loading 状态
- 哪些边缘情况值得测试

### Deferred Ideas (OUT OF SCOPE)
- 骨架屏实现 — 当前不存在，如需添加应为独立 phase
- 乐观更新/占位符 — 超出当前 scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| LOAD-01 | 搜索加载 spinner 验证 | Search page uses `LoaderCircle` with `.animate-spin` class inside the search input area. `searchStore.loading` controls visibility. Spinner text: "搜索中..." |
| LOAD-02 | Spinner 到内容过渡（调整后） | All pages transition from spinner to content. Search: "搜索结果" → "X 个结果"; Explorer: RefreshCw spin → stop; View: "加载中..." → content lines |
| LOAD-03 | Explorer 目录加载状态 | Explorer uses `RefreshCw` with conditional `animate-spin` class. Back button `disabled={loading}`. Refresh button `disabled={loading}` |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Playwright | 1.x | E2E browser testing | Project standard for all E2E tests |
| @playwright/test | latest | Test runner and assertions | Built-in expect, page fixtures |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| page.route() | built-in | Mock API responses | Intercepting API calls for delayed responses |
| page.waitForFunction() | built-in | Wait for DOM conditions | Detecting spinner visibility changes |
| page.locator('.animate-spin') | built-in | CSS class selection | Finding loading spinners |

### Test File Convention
- File: `web/tests/e2e/loading_states.spec.ts` (new file)
- Pattern: Follow `error_handling.spec.ts` structure

**Installation:** No new dependencies required -- Playwright already installed.

## Architecture Patterns

### Recommended Test Structure
```
loading_states.spec.ts
├── test.describe('Search Loading States')
│   ├── LOAD-01: spinner appears during search
│   ├── LOAD-02: spinner disappears after search completes
│   └── LOAD-01b: input disabled during loading
├── test.describe('Explorer Loading States')
│   ├── LOAD-03: RefreshCw spins during directory load
│   ├── LOAD-03b: back button disabled during loading
│   └── LOAD-03c: refresh triggers spin animation
└── test.describe('View Page Loading States')
    └── LOAD-02b: LoaderCircle shows during file load
```

### Pattern 1: Mock API Delay for Observable Loading States
**What:** Intercept API calls and delay response to ensure loading state is visible
**When to use:** When testing spinner visibility, as real responses may be too fast
**Example:**
```typescript
// Source: Derived from error_handling.spec.ts pattern
await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
  // Delay 500ms to ensure spinner is observable
  await new Promise(resolve => setTimeout(resolve, 500));
  await route.fulfill({
    status: 200,
    headers: { 'Content-Type': 'application/x-ndjson' },
    body: ''  // Empty response for loading test
  });
});
```

### Pattern 2: Detect Spinner Appearance with waitForFunction
**What:** Use `page.waitForFunction()` to wait for `.animate-spin` element to appear
**When to use:** After triggering an action that shows a loading spinner
**Example:**
```typescript
// Source: Adapted from search.spec.ts waitForFunction pattern
await page.waitForFunction(
  () => document.querySelector('.animate-spin') !== null,
  { timeout: 5000 }
);
```

### Pattern 3: Verify Spinner Disappearance
**What:** Wait for `.animate-spin` to be removed from DOM
**When to use:** After API response completes
**Example:**
```typescript
await page.waitForFunction(
  () => document.querySelector('.animate-spin') === null,
  { timeout: 10000 }
);
```

### Anti-Patterns to Avoid
- **Relying on real API speed:** Backend may respond too fast; always mock for loading tests
- **Using `page.waitForTimeout()`:** Race conditions in CI; use `waitForFunction` instead
- **Checking visibility of `.animate-spin` with `toBeVisible()`:** Element may be removed from DOM, not just hidden; use `waitForFunction` with null check

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| API delay mocking | Custom fetch interceptors | `page.route()` | Built-in, reliable, no side effects |
| Spinner detection | Custom polling loops | `page.waitForFunction()` | Optimized, handles race conditions |
| State assertions | Manual DOM queries | Playwright locators | Auto-wait, better error messages |

**Key insight:** Playwright's `page.route()` can simulate any network condition (delay, error, slow response) without modifying application code.

## Common Pitfalls

### Pitfall 1: Spinner Too Fast to Detect
**What goes wrong:** Real backend responds in <50ms, spinner appears and disappears too fast
**Why it happens:** Local backend is fast, no network latency
**How to avoid:** Always mock API with deliberate delay (500-1000ms)
**Warning signs:** Test passes without mock, fails intermittently in CI

### Pitfall 2: waitForSelector Timeout on Removed Elements
**What goes wrong:** `waitForSelector('.animate-spin', { state: 'hidden' })` times out when element is removed from DOM
**Why it happens:** Playwright `hidden` state means "not visible" but element must exist
**How to avoid:** Use `waitForFunction` checking `querySelector(...) === null` for DOM removal
**Warning signs:** Timeout errors on spinner disappearance checks

### Pitfall 3: Multiple Spinners on Page
**What goes wrong:** Page has multiple `.animate-spin` elements (sidebar loading, main content loading)
**Why it happens:** Explorer has sidebar `animate-pulse` and main content spinner
**How to avoid:** Use more specific selectors or `waitForFunction` with `querySelectorAll` and count checks
**Warning signs:** Wrong spinner detected, test assertions on unexpected elements

## Code Examples

Verified patterns from official sources and existing codebase:

### Search Page Loading State (from +page.svelte lines 400-408, 576-584)
```typescript
// Source: /web/src/routes/search/+page.svelte
// Search input is disabled during loading:
<Input disabled={searchStore.loading} ... />

// Spinner appears inside the input:
{#if searchStore.loading}
  <LoaderCircle class="h-3.5 w-3.5 animate-spin text-primary" />
{/if}

// Load More button shows spinner with text:
{#if searchStore.loading}
  <LoaderCircle class="mr-2 h-4 w-4 animate-spin" />
  {searchStore.results.length === 0 ? '搜索中...' : '加载更多...'}
{/if}
```

### Explorer Loading State (from +page.svelte lines 757-761)
```typescript
// Source: /web/src/routes/explorer/+page.svelte
// Back button disabled during loading:
<Button variant="ghost" size="icon" onclick={goUp} disabled={loading} title="后退">
  <ArrowLeft class="h-4 w-4" />
</Button>

// Refresh button with conditional spin:
<Button variant="ghost" size="icon" onclick={() => loadResources(currentOrlStr)} disabled={loading} title="刷新">
  <RefreshCw class="h-4 w-4 {loading ? 'animate-spin' : ''}" />
</Button>
```

### View Page Loading State (from +page.svelte lines 679-683)
```typescript
// Source: /web/src/routes/view/+page.svelte
{#if loading}
  <div class="flex flex-col items-center gap-2 text-muted-foreground">
    <LoaderCircle class="h-8 w-8 animate-spin" />
    <span class="text-sm">加载中...</span>
  </div>
{/if}
```

### Result Count Transition (from +page.svelte lines 527-538)
```typescript
// Source: /web/src/routes/search/+page.svelte
<h2 class="text-lg font-semibold">
  {#if filteredCount > 0}
    {filteredCount} 个结果
  {:else if !searchStore.loading && q}
    0 个结果
  {:else}
    搜索结果  // Default text when no search or loading
  {/if}
</h2>
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Fixed `waitForTimeout(1000)` | `waitForFunction` with condition checks | Throughout project | Eliminates race conditions |
| Direct API calls in E2E | `page.route()` mocking | error_handling.spec.ts | Reliable, controllable |

**Deprecated/outdated:**
- Skeleton screens: Not implemented in codebase, all loading uses spinners

## Open Questions

1. **Should we test the View page loading via Explorer navigation?**
   - What we know: View page is opened from Explorer double-click in a popup
   - What's unclear: Whether testing View loading independently or via Explorer flow is better
   - Recommendation: Test independently first, then add integration test

2. **What timeout values to use for spinner detection?**
   - What we know: Mock delay will be 500ms, Playwright default timeout is 30s
   - What's unclear: Whether 5s is enough for CI environments
   - Recommendation: Use 10s timeout for spinner appearance, 15s for disappearance

3. **Should we test the "加载更多..." button loading state?**
   - What we know: Load More shows different text based on results.length === 0
   - What's unclear: Whether this is worth a separate test case
   - Recommendation: Include as assertion within LOAD-01 test

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Playwright (chromium project) |
| Config file | `/web/playwright.config.ts` |
| Quick run command | `pnpm --dir web test:unit --run --project=chromium` |
| Full suite command | `pnpm --dir web test:unit --run` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| LOAD-01 | Search spinner visible during loading | e2e | `pnpm --dir web test:unit --run -g "LOAD-01"` | No (Wave 0) |
| LOAD-02 | Spinner transitions to content | e2e | `pnpm --dir web test:unit --run -g "LOAD-02"` | No (Wave 0) |
| LOAD-03 | Explorer loading indicators | e2e | `pnpm --dir web test:unit --run -g "LOAD-03"` | No (Wave 0) |

### Sampling Rate
- Per task commit: `pnpm --dir web test:unit --run -g "loading_states"`
- Per wave merge: Full E2E suite
- Phase gate: All loading_states tests green

### Wave 0 Gaps
- [ ] `web/tests/e2e/loading_states.spec.ts` — new test file for all loading state tests
- [ ] Framework: Playwright already configured, no additional setup needed

## Sources

### Primary (HIGH confidence)
- `/web/src/routes/search/+page.svelte` — Search page loading state implementation (lines 400-408, 527-538, 576-584)
- `/web/src/routes/explorer/+page.svelte` — Explorer loading state implementation (lines 54, 757-761)
- `/web/src/routes/view/+page.svelte` — View page loading state implementation (lines 33, 679-683)
- `/web/tests/e2e/error_handling.spec.ts` — Existing test patterns for `page.route()` mocking and `waitForFunction`

### Secondary (MEDIUM confidence)
- `/web/playwright.config.ts` — Test configuration and webServer setup
- `/web/src/lib/modules/logseek/composables/useSearch.svelte.ts` — Search state management (loading transitions)

### Tertiary (LOW confidence)
- None — all findings verified against actual codebase

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — Playwright is the established testing framework, no alternatives needed
- Architecture: HIGH — Patterns directly observed from existing tests and source code
- Pitfalls: HIGH — Based on direct code analysis of spinner implementation patterns

**Research date:** 2026-03-14
**Valid until:** 2026-04-14 (30 days — loading UI patterns are stable)
