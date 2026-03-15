---
phase: 06-edge-a11y-tests
plan: 01
subsystem: frontend
tags: [e2e, edge-cases, accessibility, playwright]
dependency_graph:
  requires:
    - web/src/lib/modules/logseek/utils/highlight.ts (escapeHtml function)
    - web/src/routes/search/SearchEmptyState.svelte (empty state UI)
    - web/src/routes/explorer/+page.svelte (empty directory message)
    - web/src/routes/search/+page.svelte (ARIA labels)
  provides:
    - web/tests/e2e/edge_cases.spec.ts (4 edge case tests)
    - web/tests/e2e/accessibility.spec.ts (3 accessibility tests)
  affects:
    - test coverage for boundary conditions and a11y flows
tech_stack:
  added:
    - Playwright E2E test patterns for mock NDJSON responses
    - Serial test mode for file system operations
  patterns:
    - NDJSON mock with { type: 'result', data: {...} } wrapper format
    - Temp directory creation/cleanup with RUN_ID for test isolation
    - Tab navigation loop for keyboard accessibility testing
key_files:
  created:
    - web/tests/e2e/edge_cases.spec.ts
    - web/tests/e2e/accessibility.spec.ts
  referenced:
    - web/tests/e2e/error_handling.spec.ts (mock patterns)
    - web/tests/e2e/explorer_interaction.spec.ts (temp dir patterns)
    - web/tests/e2e/search.spec.ts (search completion patterns)
    - web/src/lib/modules/logseek/composables/useStreamReader.svelte.ts (NDJSON format)
decisions:
  - Used NDJSON format { type: 'result', data: {...} } based on useStreamReader parsing
  - XSS test checks for escaped entities (&lt;, &gt;) rather than continuous &lt;script&gt; due to keyword highlighting
  - A11Y-03 split into two page navigations to avoid mock route conflicts
metrics:
  duration_seconds: 600
  completed_date: "2026-03-14"
  tests_added: 7
  tests_passing: 7
---

# Phase 6 Plan 1: Edge Cases and Accessibility Tests Summary

Created two new Playwright E2E test files covering edge cases and accessibility flows that were previously untested.

## What Was Built

### edge_cases.spec.ts (4 tests)

1. **EDGE-01: Empty search results** -- Mocks search API with empty NDJSON body, verifies the empty state message "您的搜索没有匹配到任何日志" appears after a search with zero results.

2. **EDGE-02: Long query (10000 chars)** -- Generates a 10000-character query string, submits it, and verifies the page remains responsive (search input and body are visible after 5 seconds).

3. **EDGE-03: XSS protection** -- Mocks search API to return NDJSON with `<script>alert("XSS")</script>` in the result line. The `highlight()` function calls `escapeHtml()` which converts `<` to `&lt;` and `>` to `&gt;`. Verifies escaped entities appear in innerHTML and no executable `<script>` tags exist in the DOM.

4. **EDGE-04: Empty directory** -- Creates a real temp directory with `fs.mkdirSync`, navigates to explorer with ORL path, verifies "This directory is empty." message appears. Uses serial mode and proper cleanup in try/finally.

### accessibility.spec.ts (3 tests)

1. **A11Y-01: Keyboard navigation** -- Tabs through up to 10 elements to find search input by `placeholder === '搜索...'`, types "error" via keyboard, presses Enter, and waits for search to initiate (spinner or results text).

2. **A11Y-02: ARIA attributes** -- Fills search input to make clear button appear, verifies `aria-label="清除搜索内容"` on clear button and `aria-label="调整侧边栏宽度"` on resize handle.

3. **A11Y-03: Focus management** -- Part 1: Mocks empty results, verifies focus returns to search input after successful search. Part 2: Navigates fresh to avoid mock conflicts, mocks 500 error, verifies retry button "重新搜索" is visible and can receive focus.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed NDJSON mock format**
- **Found during:** EDGE-03 test execution
- **Issue:** Mock returned raw `SearchJsonResult` but `useStreamReader` expects `{ type: 'result', data: {...} }` wrapper
- **Fix:** Wrapped mock data in proper event format matching `parseAndDispatch()` in `useStreamReader.svelte.ts`
- **Files modified:** `web/tests/e2e/edge_cases.spec.ts`
- **Commit:** 648e617

**2. [Rule 1 - Bug] Fixed XSS assertion for highlighted content**
- **Found during:** EDGE-03 test execution
- **Issue:** `toContain('&lt;script&gt;')` failed because `highlight()` wraps keyword "script" in `<mark>` tags, producing `&lt;<mark>script</mark>&gt;` not `&lt;script&gt;`
- **Fix:** Changed assertion to check for `&lt;` and `&gt;` separately, plus verify no executable `<script>` tags exist
- **Files modified:** `web/tests/e2e/edge_cases.spec.ts`
- **Commit:** 648e617

**3. [Rule 1 - Bug] Fixed A11Y-03 focus test failure**
- **Found during:** A11Y-03 test execution
- **Issue:** `page.locator(':focus').getAttribute('placeholder')` failed after first search due to focus state being ambiguous
- **Fix:** Added `await searchInput.click()` to explicitly set focus before checking, and split Part 2 into a fresh page navigation to avoid mock route conflicts
- **Files modified:** `web/tests/e2e/accessibility.spec.ts`
- **Commit:** 648e617

## Auth Gates

None -- no authentication required for these E2E tests.

## Test Results

All 7 tests passing in isolation and full suite:
```
7 passed (14.2s)
```

## Self-Check: PASSED

- `web/tests/e2e/edge_cases.spec.ts` exists with 4 tests (EDGE-01 through EDGE-04)
- `web/tests/e2e/accessibility.spec.ts` exists with 3 tests (A11Y-01 through A11Y-03)
- Commit `648e617` exists in git log
- All 7 tests pass
