# Playwright E2E Assertion Best Practices - OpsBox

> Research document: improving E2E test assertion quality for OpsBox
> Date: 2026-03-14
> **Status: v1.0 milestone completed (2026-03-15) — Sections 1 & 5 document historical anti-patterns that have been fixed. Sections 2-4, 6-7 remain useful as ongoing Playwright reference.**

## Executive Summary

After auditing 20+ E2E test files in `/web/tests/e2e/`, the following categories of weak assertions were identified. This document catalogs the anti-patterns found (now fixed), recommended replacements, and Playwright-specific best practices.

---

## 1. Anti-Patterns Found in Current Tests

### 1.1 `await expect(page.locator('body')).toBeVisible()` (Always Passes)

**Found in:** `settings.spec.ts` (lines 24, 59, 87, 113, 123, 137, 147, 156, 192, 200)

The `body` element is always visible if the page loads at all. This assertion never fails, providing zero signal about whether the feature actually rendered.

```typescript
// BAD: No-op assertion
await expect(page.locator('body')).toBeVisible();

// GOOD: Assert specific content exists
await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();

// GOOD: Assert specific sections rendered
await expect(page.getByText('Planner Scripts')).toBeVisible();
```

### 1.2 `expect(await x).toBeTruthy()` (Not Web-First)

**Found in:** `integration_local.spec.ts` (lines 138, 146, 154, 162), `integration_query_syntax.spec.ts` (line 120)

Using `await` before `expect()` breaks Playwright's auto-retrying web-first assertion model. The value is captured once; if the UI updates slightly after, the assertion does not retry.

```typescript
// BAD: Captures response.ok() once, no retry
expect(response.ok()).toBeTruthy();

// GOOD: Web-first assertion on status
expect(response).toBeOK();
// Or explicitly check the status:
expect(response.status()).toBe(200);
```

### 1.3 Regex Matching Any Value (`/\d+\s*个结果/`)

**Found in:** `search.spec.ts` (lines 35, 64, 96, 115, 135, 160), `search_ux.spec.ts` (lines 70, 114, 134, 173)

The regex `/\d+\s*个结果/` matches "0 个结果" just as well as "42 个结果". Tests that rely on this regex pass even when the search returns nothing, defeating the purpose.

```typescript
// BAD: Matches "0 个结果" - no real validation
expect(resultsText).toMatch(/\d+\s*个结果/);

// GOOD: Assert at least 1 result (if that is the expectation)
await expect(page.locator('.text-lg.font-semibold')).toContainText(/[1-9]\d* 个结果/);

// BETTER: Use toContainText with an exact substring when the count is deterministic
await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果');

// BEST: Assert specific result content is visible
await expect(page.getByText('Database connection failed')).toBeVisible();
```

### 1.4 Conditional Assertions (Silent Skip on Empty Data)

**Found in:** `search.spec.ts` (lines 89-93), `search_ux.spec.ts` (lines 40-56, 94-110, 157-169), `settings.spec.ts` (lines 36-38, 165-174, 213-215)

Tests that check `if (count > 0)` before asserting will pass silently when the data source is empty. This masks regressions where the search feature returns zero results due to a bug.

```typescript
// BAD: If count is 0, the test does nothing and passes
if (count > 0) {
  await expect(cards.first()).toBeVisible();
}

// GOOD: Test data setup guarantees count > 0, then assert unconditionally
// In beforeAll: create known test data with specific content
// In test: assert that specific content appears
await expect(page.getByText('Expected log line')).toBeVisible();

// GOOD (for truly optional features): Use soft assertions with explicit logging
test.describe.configure({ mode: 'serial' });
// ... then assert known content from controlled test data
```

**Root cause:** Many search tests use generic queries ("error", "exception", "timeout") that may or may not return results depending on the backend state. Tests should use controlled test data with deterministic queries.

### 1.5 Checking Existence Without Validating Value

**Found in:** `settings.spec.ts` (line 173)

```typescript
// BAD: Only checks the attribute exists, not its value
const newClass = (await page.locator('html').getAttribute('class')) || '';
expect(newClass).toBeDefined(); // Always passes since we default to ''

// GOOD: Assert the class changed to the expected value
await expect(page.locator('html')).toHaveClass(/dark/); // or /light/
```

### 1.6 Body Content Length Check

**Found in:** `settings.spec.ts` (lines 29, 50, 128)

```typescript
// BAD: Any page with any content passes
const pageContent = (await page.locator('body').textContent()) || '';
expect(pageContent.length).toBeGreaterThan(0);

// GOOD: Check for specific expected text
await expect(page.getByText('LLM Backends')).toBeVisible();
await expect(page.getByRole('table')).toBeVisible();
```

---

## 2. Recommended Assertion Patterns

### 2.1 Text Assertions: `toHaveText()` vs `toContainText()`

| Method | Use When | Example |
|--------|----------|---------|
| `toHaveText()` | Exact full text match needed | `await expect(el).toHaveText('1 个结果')` |
| `toContainText()` | Substring match is sufficient | `await expect(el).toContainText('个结果')` |
| `toContainText(regex)` | Pattern match with retry | `await expect(el).toContainText(/[1-9]\d* 个结果/)` |

```typescript
// Exact match for deterministic counts
await expect(page.locator('.result-count')).toHaveText('1 个结果');

// Substring match for presence check
await expect(page.getByRole('status')).toContainText('完成');

// Regex match for pattern validation (web-first, auto-retries)
await expect(page.locator('.result-count')).toContainText(/共 \d+ 条/);
```

### 2.2 Visibility Assertions

| Pattern | Meaning | When to Use |
|---------|---------|-------------|
| `toBeVisible()` | Element is visible in viewport | Positive assertion for UI elements |
| `not.toBeVisible()` | Element is hidden or does not exist | Negative assertion |
| `toBeHidden()` | Element explicitly hidden (display:none, visibility:hidden) | Element exists but must be hidden |
| `not.toBeAttached()` | Element removed from DOM | Element should be removed entirely |

```typescript
// GOOD: Positive assertion for element that should appear
await expect(page.getByText('搜索完成')).toBeVisible();

// GOOD: Negative assertion for element that should NOT appear
await expect(page.getByText('deprecated API')).not.toBeVisible();

// GOOD: Element removed from DOM (stronger than hidden)
await expect(page.getByText('loading...')).not.toBeAttached();
```

**Critical note:** Do NOT use `page.locator('body')` for visibility checks. Assert on specific elements instead.

### 2.3 Attribute Assertions

```typescript
// Validate href contains expected URL pattern
await expect(link).toHaveAttribute('href', /file=orl%3A%2F%2Flocal/);

// Validate aria attributes for accessibility
await expect(button).toHaveAttribute('aria-expanded', 'true');

// Validate data attributes
await expect(card).toHaveAttribute('data-status', 'complete');
```

### 2.4 Count Assertions

```typescript
// Exact count
await expect(page.locator('.result-card')).toHaveCount(3);

// Greater than zero (use sparingly - prefer exact counts when possible)
const count = await page.locator('.result-card').count();
expect(count).toBeGreaterThan(0);

// Empty state
await expect(page.locator('.result-card')).toHaveCount(0);
```

### 2.5 URL Assertions

```typescript
// Exact URL match
await expect(page).toHaveURL('http://localhost:5173/search');

// URL contains substring
await expect(page).toHaveURL(/\/search\?q=test/);

// URL with encoded parameters
await expect(page).toHaveURL(/orl=orl%3A%2F%2Flocal/);
```

---

## 3. Web-First Assertions vs Manual `expect()`

### 3.1 The Rule

Playwright assertions are **auto-retrying**: they wait for the condition to become true (up to the timeout). Manual `expect()` on captured values does NOT retry.

```typescript
// WEB-FIRST (retries automatically): Use locators directly
await expect(page.getByText('Done')).toBeVisible(); // Retries until visible or timeout

// NON-RETRYING (evaluates once): Avoid
const text = await page.getByText('Done').textContent(); // Captures once
expect(text).toBe('Done'); // Fails immediately if timing is off
```

### 3.2 When Non-Retrying Is Acceptable

Non-retyring assertions are appropriate for:
- API response status codes (they don't change after receipt)
- Input values after user interaction (stable state)
- Configuration values that should be deterministic

```typescript
// OK: API responses are final
const response = await request.post('/api/endpoint', { data: payload });
expect(response.status()).toBe(201);

// OK: Input value after explicit action
const inputValue = await searchInput.inputValue();
expect(inputValue).toContain('OR');
```

### 3.3 The `response.ok()` Trap

```typescript
// BAD: response.ok() returns a boolean - no retry mechanism
expect(response.ok()).toBeTruthy();

// GOOD: Use toBeOK() matcher
await expect(response).toBeOK();

// GOOD: Explicit status check
expect(response.status()).toBe(200);
```

---

## 4. Playwright-Specific Best Practices

### 4.1 Prefer `getByRole()` Over CSS Selectors

```typescript
// Fragile: relies on CSS class names that may change
await expect(page.locator('.text-lg.font-semibold')).toBeVisible();

// Robust: uses accessible role and name
await expect(page.getByRole('heading', { name: /results/i })).toBeVisible();
```

### 4.2 Use `exact: true` for Ambiguous Text

```typescript
// May match "logs" inside "travelogs" or other substrings
await expect(page.getByText('logs')).toBeVisible();

// Exact match only
await expect(page.getByText('logs', { exact: true })).toBeVisible();
```

### 4.3 Timeout Configuration

```typescript
// Per-assertion timeout for slow operations
await expect(result).toBeVisible({ timeout: 30000 });

// Avoid page.waitForTimeout() for synchronization - it's brittle
// BAD:
await page.waitForTimeout(1000);

// GOOD: Wait for specific condition
await expect(page.getByText('Loading...')).not.toBeVisible();
```

### 4.4 API Response Validation in Tests

```typescript
// Intercept and validate response bodies
const responsePromise = page.waitForResponse('**/api/v1/logseek/search**');
await searchInput.press('Enter');
const response = await responsePromise;

// Validate response body structure
const body = await response.json();
expect(body.results).toHaveLength(3);
expect(body.results[0].file).toContain('access.log');
```

### 4.5 Using `waitForFunction` Carefully

The current codebase uses `waitForFunction` with regex checks. This is acceptable for waiting on dynamic content, but the subsequent assertion should be stricter:

```typescript
// Acceptable: wait for dynamic content to appear
await page.waitForFunction(
  () => /\d+ 个结果/.test(document.querySelector('.result-count')?.textContent || ''),
  { timeout: 10000 }
);

// Then: assert specific expected value (not just "any number")
await expect(page.locator('.result-count')).toContainText('3 个结果');
```

---

## 5. OpsBox-Specific Recommendations

### 5.1 Controlled Test Data Pattern (Eliminate Conditional Assertions)

The root cause of most conditional assertions is that tests search against uncontrolled backend data. The `integration_local.spec.ts` and `integration_query_syntax.spec.ts` files demonstrate the correct pattern: create test data with unique identifiers in `beforeAll`, then search for those identifiers.

```typescript
// GOOD PATTERN (already used in integration_local.spec.ts):
test.beforeAll(async ({ request }) => {
  // Create test data with unique identifier
  const UNIQUE_ID = `TEST_${Date.now()}`;
  fs.writeFileSync(testFile, `[INFO] Log entry ${UNIQUE_ID}\n`);

  // Configure planner to point at test data
  await request.post('/api/v1/logseek/settings/planners/scripts', {
    data: { app: testApp, script: `SOURCES = ["orl://local${testDir}"]` }
  });
});

test('should find test data', async ({ page }) => {
  await searchInput.fill(`app:${testApp} "${UNIQUE_ID}"`);
  await searchInput.press('Enter');

  // Deterministic assertion - always exactly 1 result
  await expect(page.locator('.result-count')).toContainText('1 个结果');
  await expect(page.getByText(UNIQUE_ID)).toBeVisible();
});
```

### 5.2 Fix List for High-Impact Anti-Patterns

Priority order for fixing (by number of occurrences and severity):

1. **`page.locator('body')).toBeVisible()`** in `settings.spec.ts` - 10 occurrences of no-op assertions
2. **`/\d+\s*个结果/` regex** in `search.spec.ts`, `search_ux.spec.ts` - passes on "0 results"
3. **Conditional `if (count > 0)` assertions** in `search_ux.spec.ts` - silent skips
4. **`expect(response.ok()).toBeTruthy()`** in integration tests - not web-first
5. **`expect(newClass).toBeDefined()`** in `settings.spec.ts` - no value check
6. **Body content length checks** in `settings.spec.ts` - meaningless

### 5.3 Test Stability Improvements

- Replace `page.waitForTimeout(300/500/1000)` with condition-based waits where possible
- Use `await expect(locator).toBeVisible({ timeout: ... })` instead of `waitForSelector` followed by `expect`
- Add trace-on-first-retry (already configured in `playwright.config.ts`)

---

## 6. Testing Library Comparison

Playwright's assertion model differs from Testing Library (often used with Jest/Vitest):

| Aspect | Playwright | Testing Library |
|--------|------------|-----------------|
| Auto-retry | Built-in via web-first assertions | Manual via `waitFor` |
| Selectors | `getByRole`, `getByText`, `locator` | `getByRole`, `getByText`, `queryBy*` |
| Negative | `not.toBeVisible()` | `waitFor(() => expect(el).not.toBeVisible())` |
| API testing | `request` fixture + `expect(response)` | Separate supertest or axios |

Playwright's advantage is the auto-retrying assertions, which eliminate most flakiness from timing issues. The key is to always use locators in `expect()` rather than captured values.

---

## 7. Quick Reference Cheat Sheet

```typescript
// TEXT
await expect(locator).toHaveText('exact text')
await expect(locator).toContainText('substring')
await expect(locator).toContainText(/regex/)

// VISIBILITY
await expect(locator).toBeVisible()
await expect(locator).not.toBeVisible()
await expect(locator).toBeHidden()
await expect(locator).not.toBeAttached()

// ATTRIBUTES
await expect(locator).toHaveAttribute('href', /pattern/)
await expect(locator).toHaveClass(/dark/)
await expect(locator).toHaveValue('input value')

// COUNT
await expect(locator).toHaveCount(3)

// URL
await expect(page).toHaveURL(/pattern/)

// API RESPONSE
await expect(response).toBeOK()
expect(response.status()).toBe(201)

// INPUT VALUE (non-retrying is OK here)
const value = await locator.inputValue()
expect(value).toBe('expected')
```

---

## Sources

- Playwright Assertions docs: https://playwright.dev/docs/test-assertions
- Playwright Auto-waiting: https://playwright.dev/docs/actionability
- Web-first assertions: https://playwright.dev/docs/best-practices#assertions
- OpsBox test files: `/Users/wangyue/workspace/codelder/opsboard/web/tests/e2e/`
