# Phase 6: Edge Cases and Accessibility Tests - Research

**Researched:** 2026-03-14
**Domain:** Playwright E2E testing for edge cases and accessibility
**Confidence:** HIGH

## Summary

This phase creates two new E2E test files covering edge cases (empty search results, very long queries, XSS protection, empty directory browsing) and accessibility (keyboard navigation, ARIA attributes, focus management). The project already has 18 existing E2E spec files with well-established patterns for mocking API responses, waiting for search completion, and asserting on specific DOM elements. The search page (`+page.svelte`) and explorer page (`+page.svelte`) expose known ARIA labels, data-testid attributes, and semantic selectors that these tests can target directly.

**Primary recommendation:** Use `page.route()` to mock API responses for controlled edge case testing, and leverage existing `aria-label`, `data-testid`, and semantic selectors (`h3`, `mark.highlight`) from established test patterns.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `@playwright/test` | (from project) | E2E test framework | Already used in all 18 existing spec files |
| Node.js `fs`/`path` | built-in | Temp directory/file creation for explorer tests | Used in `explorer_interaction.spec.ts` |
| Node.js `url` | built-in | ESM `__dirname` resolution | Used in `playwright.config.ts` and test files |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| Playwright `page.route()` | built-in | Mock API responses | EDGE-01 (empty results), EDGE-03 (XSS payload) |
| Playwright `page.keyboard` | built-in | Keyboard interaction simulation | A11Y-01 (Tab/Enter navigation) |
| Playwright `page.getByRole()` | built-in | Accessible element selection | A11Y-02 (ARIA verification) |
| Playwright `page.getByPlaceholder()` | built-in | Input element selection | Search input targeting |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `page.route()` mocking | Real backend with seeded data | Mocking is deterministic and fast; real backend is slower and requires setup |
| `axe-core` integration | Manual ARIA checks | axe-core is deferred per CONTEXT.md; manual checks sufficient for current scope |
| `page.locator(':focus')` | `page.evaluate(document.activeElement)` | Both work; `:focus` is more idiomatic Playwright |

## Architecture Patterns

### Test File Organization
```
web/tests/e2e/
├── edge_cases.spec.ts          # NEW: EDGE-01 to EDGE-04
├── accessibility.spec.ts       # NEW: A11Y-01 to A11Y-03
├── error_handling.spec.ts      # EXISTING: pattern reference
├── loading_states.spec.ts      # EXISTING: pattern reference
├── search.spec.ts              # EXISTING: search flow patterns
├── search_ux.spec.ts           # EXISTING: UX assertion patterns
└── explorer_interaction.spec.ts # EXISTING: explorer patterns
```

### Pattern 1: Search Completion Wait
**What:** Wait for search to finish by polling for result count text that matches `/\d+\s*个结果/`
**When to use:** Any test that triggers a search via Enter key
**Example:**
```typescript
// Source: search.spec.ts lines 25-32
await page.waitForFunction(
  () => {
    const el = document.querySelector('.text-lg.font-semibold');
    const text = el?.textContent || '';
    return /\d+\s*个结果/.test(text) && !text.includes('搜索结果');
  },
  { timeout: 60000 }
);
```

### Pattern 2: API Mocking with page.route()
**What:** Intercept backend API calls and return controlled responses
**When to use:** Error states, empty results, XSS payload injection
**Example:**
```typescript
// Source: error_handling.spec.ts lines 21-27
await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
  await route.fulfill({
    status: 200,
    headers: { 'Content-Type': 'application/x-ndjson' },
    body: '' // empty results
  });
});
```

### Pattern 3: Temporary Directory for Explorer Tests
**What:** Create temp dirs/files in `beforeAll`, clean up in `afterAll`
**When to use:** Explorer tests requiring real filesystem
**Example:**
```typescript
// Source: explorer_interaction.spec.ts lines 24-50
const RUN_ID = Date.now();
const TEST_DIR = path.join(__dirname, `temp_explorer_interaction_${RUN_ID}`);
// ...
test.afterAll(async () => {
  try {
    if (fs.existsSync(TEST_DIR)) {
      fs.rmSync(TEST_DIR, { recursive: true, force: true });
    }
  } catch (e) { /* ignore */ }
});
```

### Anti-Patterns to Avoid
- **Waiting for `networkidle` after triggering search:** Search uses NDJSON streaming, so `networkidle` may never fire during active search. Use `waitForFunction` with result count regex instead.
- **Using `body` visibility as assertion:** Too weak. Always assert on specific UI elements (h3 text, aria-labels, result counts).
- **Conditional test logic with `if`:** Tests should assert deterministic outcomes. Use mocking to control state.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Keyboard navigation testing | Custom event dispatch | `page.keyboard.press('Tab')` + `page.locator(':focus')` | Playwright handles real browser focus, not synthetic events |
| XSS verification | DOM inspection of raw HTML | Check that `<script>` becomes `&lt;script&gt;` in text content | `escapeHtml()` already in `highlight.ts`; test its output, not implementation |
| ARIA attribute checks | `page.evaluate` with `getAttribute` | `page.getByRole()` or `expect(locator).toHaveAttribute('aria-label', ...)` | More readable and leverages accessibility tree |

## Common Pitfalls

### Pitfall 1: Search Input Selector
**What goes wrong:** Using `input#search` or other CSS selectors that may not exist
**Why it happens:** The input uses Svelte `Input` component; ID is set as `id="search"`
**How to avoid:** Use `page.getByPlaceholder('搜索...')` -- this is the established pattern across all search tests (search.spec.ts, error_handling.spec.ts, loading_states.spec.ts)
**Warning signs:** Test fails with "locator not found"

### Pitfall 2: Empty Results State
**What goes wrong:** Looking for `h3` with text "0 个 results" instead of the actual no-results component
**Why it happens:** The `SearchEmptyState` component with `type="no-results"` shows `h3` "您的搜索没有匹配到任何日志" only after search returns 0 results AND the UI transitions to the empty state
**How to avoid:** Use `page.route()` to mock empty NDJSON response, then wait for `h3` with text "您的搜索没有匹配到任何日志"
**Warning signs:** Test finds "0 个结果" text but not the dedicated empty state component

### Pitfall 3: Keyboard Focus After Search
**What goes wrong:** Assuming focus stays on input after search completes
**Why it happens:** Svelte reactivity may cause DOM updates that shift focus
**How to avoid:** After search completion, use `page.locator(':focus')` to get the focused element and assert its attributes
**Warning signs:** Focus-related assertions pass inconsistently

## Code Examples

### EDGE-01: Empty Search Results
```typescript
// Mock empty NDJSON response
await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
  await route.fulfill({
    status: 200,
    headers: { 'Content-Type': 'application/x-ndjson' },
    body: ''
  });
});

const searchInput = page.getByPlaceholder('搜索...');
await searchInput.fill('NONEXISTENT_KEYWORD_XYZ123');
await searchInput.press('Enter');

// Wait for "0 个结果" in the results header
await page.waitForFunction(
  () => {
    const el = document.querySelector('.text-lg.font-semibold');
    return el?.textContent?.includes('0 个结果');
  },
  { timeout: 10000 }
);

// Verify the no-results h3 appears
await expect(page.locator('h3', { hasText: '您的搜索没有匹配到任何日志' })).toBeVisible();
```

### EDGE-02: Very Long Query
```typescript
// Generate 10000+ character query
const longQuery = 'a'.repeat(10000);
const searchInput = page.getByPlaceholder('搜索...');
await searchInput.fill(longQuery);
await searchInput.press('Enter');

// Page should not crash - verify page is still responsive
await expect(searchInput).toBeVisible();
// After search completes or errors, page should still be usable
await expect(page.locator('body')).toBeVisible();
```

### EDGE-03: XSS Protection
```typescript
// Mock API to echo back the search query in results
await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
  const xssPayload = '<script>alert("XSS")</script>';
  const ndjsonLine = JSON.stringify({
    type: 'match',
    path: '/test.log',
    line_number: 1,
    line: `Found: ${xssPayload}`,
    keywords: [{ text: xssPayload, type: 'literal' }]
  });
  await route.fulfill({
    status: 200,
    headers: { 'Content-Type': 'application/x-ndjson' },
    body: ndjsonLine + '\n'
  });
});

const searchInput = page.getByPlaceholder('搜索...');
await searchInput.fill('<script>alert("XSS")</script>');
await searchInput.press('Enter');

// Wait for results
await page.waitForFunction(
  () => document.querySelector('[data-result-card]') !== null,
  { timeout: 10000 }
);

// Verify script tags are escaped in rendered output
const cardText = await page.locator('[data-result-card]').first().textContent();
expect(cardText).not.toContain('<script>');
expect(cardText).toContain('&lt;script&gt;');
```

### EDGE-04: Empty Directory
```typescript
import * as fs from 'fs';
import * as path from 'path';

const EMPTY_DIR = path.join(__dirname, `temp_empty_dir_${Date.now()}`);
fs.mkdirSync(EMPTY_DIR, { recursive: true });

// Navigate to empty directory via ORL
await page.goto(`/explorer?orl=${encodeURIComponent(`orl://local${EMPTY_DIR}`)}`);
await page.waitForLoadState('networkidle');

// Verify empty directory message
await expect(page.getByText('This directory is empty.')).toBeVisible({ timeout: 5000 });

// Cleanup
fs.rmSync(EMPTY_DIR, { recursive: true });
```

### A11Y-01: Keyboard Navigation
```typescript
const searchInput = page.getByPlaceholder('搜索...');

// Tab from body to search input
await page.keyboard.press('Tab');
// Continue tabbing to find the search input
let focusedElement = page.locator(':focus');
for (let i = 0; i < 10; i++) {
  const placeholder = await focusedElement.getAttribute('placeholder');
  if (placeholder === '搜索...') break;
  await page.keyboard.press('Tab');
  focusedElement = page.locator(':focus');
}

// Verify we found the search input
await expect(focusedElement).toHaveAttribute('placeholder', '搜索...');

// Enter should trigger search
await page.keyboard.type('error');
await page.keyboard.press('Enter');

// Verify search initiated (spinner or results appear)
await page.waitForFunction(
  () => {
    const spinner = document.querySelector('.animate-spin');
    const results = document.querySelector('.text-lg.font-semibold');
    return spinner !== null || (results?.textContent || '').match(/\d+\s*个结果/);
  },
  { timeout: 60000 }
);
```

### A11Y-02: ARIA Attributes
```typescript
// Navigate to search page
await page.goto('/search');
await page.waitForLoadState('networkidle');

// Type text to make clear button appear
const searchInput = page.getByPlaceholder('搜索...');
await searchInput.fill('test');

// Verify clear button has aria-label
const clearButton = page.getByRole('button', { name: '清除搜索内容' });
await expect(clearButton).toBeVisible();

// Verify sidebar resize handle has aria-label
const resizeHandle = page.locator('[aria-label="调整侧边栏宽度"]');
await expect(resizeHandle).toBeAttached();
```

### A11Y-03: Focus Management
```typescript
// Mock error response
await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
  await route.fulfill({
    status: 500,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ detail: 'Test error' })
  });
});

const searchInput = page.getByPlaceholder('搜索...');
await searchInput.fill('test query');
await searchInput.press('Enter');

// Wait for error state
await page.waitForFunction(
  () => {
    const h3s = document.querySelectorAll('h3');
    return Array.from(h3s).some(h3 => h3.textContent?.includes('搜索出错'));
  },
  { timeout: 10000 }
);

// Verify retry button exists and is focusable
const retryButton = page.getByRole('button', { name: '重新搜索' });
await expect(retryButton).toBeVisible();

// Tab to retry button and verify it receives focus
await retryButton.focus();
const focused = page.locator(':focus');
await expect(focused).toHaveText('重新搜索');
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `page.locator('body')` assertions | Specific element selectors (h3, aria-label, data-testid) | Phase 1 (ASSERT-01 to 04) | Much stronger test reliability |
| Real backend for error tests | `page.route()` API mocking | Phase 2 (ERROR tests) | Deterministic, fast error state testing |
| CSS class-based selectors | `getByRole()` / `getByPlaceholder()` | Ongoing | More resilient to UI refactors |

**Deprecated/outdated:**
- `page.locator('body').toBeVisible()` as primary assertion: replaced by targeted element checks
- Using `page.waitForTimeout()` for timing: replaced by `waitForFunction` for deterministic waits

## Open Questions

1. **XSS payload echoing in results**
   - What we know: `escapeHtml()` in `highlight.ts` handles `<`, `>`, `&`, `"`, `'` characters
   - What's unclear: Whether the mocked NDJSON response structure correctly triggers the highlight/render path to test escaping
   - Recommendation: Test that rendered DOM text content contains `&lt;` and `&gt;` entities rather than raw `<` and `>`

2. **Explorer empty directory API response**
   - What we know: The explorer checks `displayedItems.length === 0 && !loading` to show "This directory is empty."
   - What's unclear: Whether navigating to a real empty dir via ORL will trigger the same code path
   - Recommendation: Use real filesystem (like explorer_interaction.spec.ts) rather than mocking, since explorer list API may behave differently with mock

3. **Long query backend behavior**
   - What we know: The input accepts any text; backend may reject very long queries
   - What's unclear: What the exact upper limit is or if there is one
   - Recommendation: Test 10000 chars (as specified in CONTEXT.md) and verify page remains responsive regardless of backend response

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Playwright (from `@playwright/test`) |
| Config file | `web/playwright.config.ts` |
| Quick run command | `npx playwright test tests/e2e/edge_cases.spec.ts --project=chromium` |
| Full suite command | `npx playwright test --project=chromium` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| EDGE-01 | Empty search results state | e2e | `npx playwright test tests/e2e/edge_cases.spec.ts -g "EDGE-01"` | ❌ Wave 0 |
| EDGE-02 | Very long query handling | e2e | `npx playwright test tests/e2e/edge_cases.spec.ts -g "EDGE-02"` | ❌ Wave 0 |
| EDGE-03 | XSS protection | e2e | `npx playwright test tests/e2e/edge_cases.spec.ts -g "EDGE-03"` | ❌ Wave 0 |
| EDGE-04 | Empty directory browsing | e2e | `npx playwright test tests/e2e/edge_cases.spec.ts -g "EDGE-04"` | ❌ Wave 0 |
| A11Y-01 | Keyboard navigation | e2e | `npx playwright test tests/e2e/accessibility.spec.ts -g "A11Y-01"` | ❌ Wave 0 |
| A11Y-02 | ARIA attributes | e2e | `npx playwright test tests/e2e/accessibility.spec.ts -g "A11Y-02"` | ❌ Wave 0 |
| A11Y-03 | Focus management | e2e | `npx playwright test tests/e2e/accessibility.spec.ts -g "A11Y-03"` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `npx playwright test tests/e2e/edge_cases.spec.ts tests/e2e/accessibility.spec.ts --project=chromium`
- **Per wave merge:** `npx playwright test --project=chromium`
- **Phase gate:** All 7 new tests green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `web/tests/e2e/edge_cases.spec.ts` — covers EDGE-01 through EDGE-04
- [ ] `web/tests/e2e/accessibility.spec.ts` — covers A11Y-01 through A11Y-03

*(No framework gaps — Playwright is already configured and all dependencies installed)*

## Sources

### Primary (HIGH confidence)
- Search page source: `web/src/routes/search/+page.svelte` (lines 390-450) — search input, clear button aria-label, sidebar resize aria-label
- SearchEmptyState component: `web/src/routes/search/SearchEmptyState.svelte` — h3 "您的搜索没有匹配到任何日志", error state h3 "搜索出错"
- Explorer source: `web/src/routes/explorer/+page.svelte` (lines 928, 998) — "This directory is empty." in both table and grid views
- SearchResultCard: `web/src/routes/search/SearchResultCard.svelte` (line 171) — `data-result-card={index}` attribute
- highlight.ts: `web/src/lib/modules/logseek/utils/highlight.ts` — `escapeHtml()` function

### Secondary (MEDIUM confidence)
- error_handling.spec.ts — Pattern reference for `page.route()` mocking, `waitForFunction` with h3 text checks, retry button assertions
- loading_states.spec.ts — Pattern reference for spinner detection, transition verification
- explorer_interaction.spec.ts — Pattern reference for temp directory creation, ORL navigation, empty directory handling
- search.spec.ts — Pattern reference for search completion wait pattern (`/\d+\s*个结果/` regex)

### Tertiary (LOW confidence)
- Focus management after Svelte reactivity updates — behavior may vary; needs validation in test runs

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — All libraries already used in existing test files
- Architecture: HIGH — Established patterns from 18 existing spec files
- Pitfalls: MEDIUM — Focus management edge cases need runtime validation

**Research date:** 2026-03-14
**Valid until:** 2026-04-14 (30 days - stable E2E patterns)
