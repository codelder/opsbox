---
phase: 04-error-handling-tests
plan: 01
subsystem: e2e-tests
tags: [playwright, error-handling, search, e2e]
dependency_graph:
  requires: [search.spec.ts, settings.spec.ts]
  provides: [error_handling.spec.ts]
  affects: []
tech-stack:
  added: []
  patterns: [page.route mocking, waitForFunction, getByPlaceholder]
key-files:
  created:
    - web/tests/e2e/error_handling.spec.ts
  modified: []
decisions:
  - "Used Array.from(document.querySelectorAll('h3')) instead of document.querySelector('h3') because page has multiple h3 elements (筛选 header before error section)"
  - "Used .first() on error message locator because 'Internal Server Error' appears in both the summary paragraph and the details panel"
metrics:
  duration: ~15 minutes
  completed: "2026-03-14T13:28:00Z"
  tasks_completed: 1
  tasks_total: 1
  tests_added: 4
  tests_passing: 4
  lines_added: 190
---

# Phase 04 Plan 01: Error Handling E2E Tests Summary

## One-Liner

Created 4 Playwright E2E tests verifying search page error feedback for API failures, network timeouts, error details interaction, and search cancellation.

## What Was Built

New test file `web/tests/e2e/error_handling.spec.ts` with 4 independent tests covering error handling scenarios:

| Test | Requirement | Behavior Verified |
|------|-------------|-------------------|
| ERROR-01 | API 500 error | Shows "搜索出错" title, "Internal Server Error" message, "重新搜索" button |
| ERROR-02 | Network timeout | Shows error state on route abort with retry option |
| ERROR-03 | Error details | Expand/collapse `<details>` panel, retry button triggers new search |
| ERROR-04 | Cancellation | Loading spinner clears, clear button works, new search can start |

## Test Patterns Used

- **API Mocking**: `page.route('**/api/v1/logseek/search.ndjson', ...)` to intercept search requests
- **Error Simulation**: `route.fulfill({ status: 500, ... })` for HTTP errors, `route.abort('timedout')` for network failures
- **Async State**: `page.waitForFunction()` polling DOM for error state elements
- **Selectors**: `getByPlaceholder('搜索...')`, `locator('h3', { hasText: '搜索出错' })`, `getByRole('button', { name: '重新搜索' })`

## Verification Results

```
Running 4 tests using 4 workers

  ✓ ERROR-01: should display error message on API 500 (1.6s)
  ✓ ERROR-02: should display error on network timeout (1.6s)
  ✓ ERROR-03: should expand/collapse error details and retry (2.2s)
  ✓ ERROR-04: should clear loading spinner on search cancellation (1.7s)

4 passed (8.3s)
```

## Deviations from Plan

None - plan executed exactly as written. Minor fix during development: `waitForFunction` selector needed to iterate all h3 elements instead of just `document.querySelector('h3')` because the page has multiple h3 headings (filter sidebar header "筛选" appears before the error section).

## Commit

- `4cc33f6`: test(04-01): add error handling E2E tests

## Self-Check: PASSED

- [x] File `web/tests/e2e/error_handling.spec.ts` exists (190 lines, >120 minimum)
- [x] ERROR-01 verifies h3 "搜索出错", error message, retry button
- [x] ERROR-02 verifies timeout error state with retry option
- [x] ERROR-03 verifies details expand/collapse and retry functionality
- [x] ERROR-04 verifies loading spinner clears, new search can initiate
- [x] All tests use `page.route()` mocking (no real backend)
- [x] Commit `4cc33f6` exists in git history
