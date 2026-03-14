---
phase: 03-integration-explorer-spec-ts
plan: 01
subsystem: frontend
tags: [e2e, playwright, assertions, explorer]
dependency_graph:
  requires: []
  provides:
    - Tightened E2E assertions for integration_explorer.spec.ts
  affects:
    - web/tests/e2e/integration_explorer.spec.ts
tech_stack:
  added: []
  patterns:
    - Playwright download event verification
    - Specific element selectors over body regex
key_files:
  modified:
    - web/tests/e2e/integration_explorer.spec.ts
decisions:
  - Used '下载' text selector for context menu item (matches +page.svelte)
  - Used '资源列举失败' and '错误详情' selectors for error display
  - Set 10s timeout for download event capture
metrics:
  duration: ~12 minutes
  completed: "2026-03-14"
  tests_modified: 4
  assertions_tightened: 5
---

# Phase 03 Plan 01: Tighten integration_explorer.spec.ts Assertions Summary

## One-liner

Replaced 5 weak `body.toContainText()` assertions with specific element checks and completed the download test with `page.waitForEvent('download')`.

## Changes Made

### Task 1: Remove 3 redundant negative body assertions

Removed 3 `body.not.toContainText()` calls that were redundant because positive element checks already verified the same behavior:

| Line (original) | Removed assertion | Already covered by |
|---|---|---|
| 121 | `body.not.toContainText(/error\|错误/i)` | `getByText('test.txt').toBeVisible()` + `getByText('test.log').toBeVisible()` |
| 149 | `body.not.toContainText(/500\|Internal Server Error/i)` | `getByText(AGENT_ID).toBeVisible()` |
| 173 | `body.not.toContainText(/404\|Not Found\|错误/i)` | `getByText(new RegExp(namePrefix)).first().toBeVisible()` |

### Task 2: Replace 2 error body assertions with specific element checks

Replaced the "should prohibit access" test assertions:

**Error case (forbidden path):**
- BEFORE: `await expect(page.locator('body')).toContainText(/Access denied|Not Found|404|错误/i);`
- AFTER: `await expect(page.getByText('资源列举失败')).toBeVisible({ timeout: 5000 });` + `await expect(page.getByText('错误详情')).toBeVisible();`

**Success case (allowed path):**
- BEFORE: `await expect(page.locator('body')).not.toContainText(/Access denied|Not Found|404|错误/i);`
- AFTER: `await expect(page.getByText('资源列举失败')).not.toBeVisible();`

### Task 3: Complete download test with waitForEvent

Replaced the incomplete download test (previously only verified file visibility):

- BEFORE: Just clicked right-click and verified `test.txt` was visible
- AFTER: Uses `page.waitForEvent('download')` to capture the actual browser download, verifies `suggestedFilename()` is `'test.txt'`, and confirms downloaded file size is greater than 0 bytes

## Verification

All tests pass:

```
13 passed (34.3s)
```

No `body.toContainText` or `body.not.toContainText` assertions remain in the file.

## Success Criteria

- [x] Zero `body.toContainText` or `body.not.toContainText` assertions in the file
- [x] 5 assertions replaced: 3 removed (redundant), 2 replaced with specific elements
- [x] Download test uses `page.waitForEvent('download')` and verifies filename + size
- [x] All 13 tests pass with `--project=chromium`
- [x] No new test cases added (only modifications to existing tests)

## Deviations from Plan

None - plan executed exactly as written.

## Commit

- `8e33a45`: `test(03-01): tighten integration_explorer.spec.ts assertions`
