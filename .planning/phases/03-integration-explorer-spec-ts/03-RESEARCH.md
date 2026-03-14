# Phase 3: Tighten `integration_explorer.spec.ts` Assertions - Research

**Researched:** 2026-03-14
**Domain:** Playwright E2E test assertion tightening for Explorer page
**Confidence:** HIGH

## Summary

The `integration_explorer.spec.ts` file contains 5 weak `body.toContainText()` assertions and 1 incomplete download test. The weak assertions check for generic error patterns (`/error|错误/i`, `/500|Internal Server Error/i`, etc.) on the entire page body, which could pass vacuously or fail on unrelated content. The download test only verifies the file is visible but never triggers or validates the actual download.

The Explorer Svelte page (`/explorer/+page.svelte`) has specific DOM elements for error display and download functionality that tests should target instead.

**Primary recommendation:** Replace all 5 `body.toContainText()` calls with specific element checks against the Explorer error display container (heading + error details `<details>` element), and implement actual download verification using Playwright's `page.waitForEvent('download')`.

## User Constraints (from CONTEXT.md)

### Locked Decisions
- 5 `body.toContainText(/error|.../i)` calls replaced with specific error element verification
- Download test enhanced to verify download event, filename, and file size > 0
- All conditional assertions removed; use `waitForSelector` for dynamic elements

### Claude's Discretion
- Specific error element selectors (depends on error display implementation)
- Download event wait timeout
- Whether to verify specific response body field values

### Deferred Ideas (OUT OF SCOPE)
- None

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| @playwright/test | Latest | E2E testing framework | Project standard for all E2E tests |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| bits-ui | ^2.14.4 | Context menu component | Explorer uses ContextMenu for download |

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| ASSERT-04 | Tighten `integration_explorer.spec.ts` — complete download tests, verify response body fields, remove conditional skips | Error display DOM structure identified, download mechanism analyzed, 5 weak assertions catalogued |

## Architecture Patterns

### Error Display DOM Structure (from +page.svelte lines 833-884)

When `error` state is non-null, the page renders:

```svelte
{#if error}
  <div class="pointer-events-auto mx-auto w-full max-w-5xl py-12">
    <div class="rounded-lg border border-border bg-card p-10 md:p-14">
      <h3 class="text-2xl font-normal text-foreground">资源列举失败</h3>
      <details open>
        <summary><span>错误详情</span></summary>
        <p class="rounded bg-muted p-3 font-mono text-xs leading-relaxed break-all">
          {error}
        </p>
      </details>
    </div>
  </div>
{/if}
```

**Key selectors for error verification:**
- Error title: `page.getByText('资源列举失败')`
- Error details section: `page.locator('details').filter({ has: page.locator('summary', { hasText: '错误详情' }) })`
- Error message text: The `<p>` element containing the error message

### Download Mechanism (from +page.svelte lines 235-244)

The download is triggered via context menu:
1. Right-click on a file item
2. Click "下载" menu item
3. `handleDownload()` creates an anchor with href `/api/v1/explorer/download?orl=...` and clicks it

```typescript
function handleDownload(item: ResourceItem) {
  const url = `/api/v1/explorer/download?orl=${encodeURIComponent(item.path)}`;
  const a = document.createElement('a');
  a.href = url;
  a.download = '';
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
}
```

**Playwright download approach:**
```typescript
const downloadPromise = page.waitForEvent('download');
await page.getByText('下载').click();
const download = await downloadPromise;
expect(download.suggestedFilename()).toBe('test.txt');
const path = await download.path();
// Verify file exists and has content
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Download verification | Manual HTTP request | `page.waitForEvent('download')` | Captures actual browser download event, verifies filename and content |
| Element waiting | `setTimeout` + assertion | `waitForSelector` / `locator.waitFor()` | Proper Playwright waiting with built-in retry |

## Common Pitfalls

### Pitfall 1: Context Menu in Playwright
**What goes wrong:** Right-click context menu from `bits-ui` ContextMenu may not trigger reliably in headless mode
**Why it happens:** Context menus require specific event handling
**How to avoid:** Use `{ button: 'right' }` option with `click()`, wait for menu to appear before clicking menu item
**Warning signs:** Menu item not found error after right-click

### Pitfall 2: Download Event Timeout
**What goes wrong:** `waitForEvent('download')` times out if download doesn't start
**Why it happens:** Download is triggered programmatically via anchor click, may need network time
**How to avoid:** Set appropriate timeout (e.g., 10000ms), ensure the download URL is correct
**Warning signs:** Timeout error on `waitForEvent('download')`

## Code Examples

### Replacing body assertion (error case)
```typescript
// BEFORE (weak):
await expect(page.locator('body')).toContainText(/Access denied|Not Found|404|错误/i);

// AFTER (specific):
await expect(page.getByText('资源列举失败')).toBeVisible({ timeout: 5000 });
await expect(page.getByText('错误详情')).toBeVisible();
```

### Replacing body assertion (success case)
```typescript
// BEFORE (weak negative check):
await expect(page.locator('body')).not.toContainText(/error|错误/i);

// AFTER (positive check on actual elements):
await expect(page.getByText('test.txt')).toBeVisible({ timeout: 5000 });
await expect(page.getByText('test.log')).toBeVisible();
```

### Download test implementation
```typescript
test('should download local file by clicking', async ({ page }) => {
  await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(`orl://local${TEST_FILES_DIR}`)}`);
  await page.waitForLoadState('networkidle');

  // Right-click to open context menu
  await page.getByText('test.txt').click({ button: 'right' });

  // Wait for download event before clicking menu item
  const downloadPromise = page.waitForEvent('download', { timeout: 10000 });
  await page.getByText('下载').click();

  const download = await downloadPromise;

  // Verify filename
  expect(download.suggestedFilename()).toBe('test.txt');

  // Verify file size > 0
  const downloadPath = await download.path();
  expect(downloadPath).toBeTruthy();
  const stats = fs.statSync(downloadPath!);
  expect(stats.size).toBeGreaterThan(0);
});
```

## State of the Art

| Old Approach | Current Approach | Impact |
|--------------|------------------|--------|
| `body.toContainText()` | Specific element selectors | Tests fail only when the actual feature breaks |
| Conditional assertions (`if (count > 0)`) | Direct assertions with `waitForSelector` | Eliminates vacuous passes |
| Incomplete download tests | Full download verification with `waitForEvent('download')` | Tests actual download functionality |

## Open Questions

1. **Context menu reliability in CI**
   - What we know: `bits-ui` ContextMenu uses portal rendering
   - What's unclear: Whether it renders reliably in Playwright headless mode
   - Recommendation: Test locally first, may need to add `await page.waitForTimeout(300)` after right-click

2. **Download URL accessibility**
   - What we know: Download endpoint is `/api/v1/explorer/download?orl=...`
   - What's unclear: Whether the test agent serves files correctly for download
   - Recommendation: The test already works for file listing, download should work with same agent

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Playwright (via `@playwright/test`) |
| Config file | `web/playwright.config.ts` |
| Quick run command | `cd web && npx playwright test tests/e2e/integration_explorer.spec.ts --project=server` |
| Full suite command | `cd web && npx playwright test tests/e2e/integration_explorer.spec.ts` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ASSERT-04 | Tighten explorer assertions | E2E | `cd web && npx playwright test tests/e2e/integration_explorer.spec.ts --project=server` | Yes |

### Sampling Rate
- Per task commit: Run the test file
- Phase gate: All 11 tests pass with tightened assertions

## Sources

### Primary (HIGH confidence)
- `/web/src/routes/explorer/+page.svelte` - Error display HTML structure (lines 833-884), download handler (lines 235-244)
- `/web/tests/e2e/integration_explorer.spec.ts` - Current test file with 5 weak assertions (lines 121, 149, 173, 500, 509) and incomplete download test (lines 196-210)

### Secondary (MEDIUM confidence)
- Playwright docs: `page.waitForEvent('download')` pattern for download verification
- Phase 1 plan (01-PLAN.md): Pattern for replacing body assertions with specific element checks

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - Playwright is the established project standard
- Architecture: HIGH - Error display DOM structure directly observable in source
- Pitfalls: MEDIUM - Context menu reliability in headless mode needs validation

**Research date:** 2026-03-14
**Valid until:** 2026-04-14 (stable project, no expected changes)
