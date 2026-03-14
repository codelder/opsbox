# Phase 4: Error Handling Tests - Research

**Researched:** 2026-03-14
**Domain:** Playwright E2E error scenario testing with route mocking
**Confidence:** HIGH

## Summary

Phase 4 creates `error_handling.spec.ts` with 4 tests covering error scenarios: API 500 errors, network timeouts, error display interaction, and search cancellation cleanup. All required components exist in the codebase, and established mocking patterns from `settings.spec.ts` and `integration_performance.spec.ts` provide clear templates.

The project uses RFC 7807 Problem Details format for backend errors (see `backend/opsbox-core/src/error.rs`), which the frontend `search.ts` API client parses to extract `detail` field. The search page displays errors via `SearchEmptyState.svelte` with type="error", and the explorer page has its own error display with h3 "资源列举失败".

**Primary recommendation:** Follow the route mocking patterns from `settings.spec.ts` and `integration_performance.spec.ts`. Use `page.route()` with `route.fulfill()` for error responses and `route.abort('timedout')` for timeout simulation. Test error UI visibility, retry button presence, and state cleanup after cancellation.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| @playwright/test | (project) | E2E test framework | Already used for all E2E tests |
| SvelteKit 2.22 | 2.22 | Frontend framework | Project standard |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| AbortController | browser API | Search cancellation | UseSearch composable manages it internally |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| route.fulfill() | route.abort() | abort() for network errors only, fulfill() for HTTP status codes |

**Installation:** No additional packages needed - all dependencies already installed.

## Architecture Patterns

### Recommended Project Structure
```
web/tests/e2e/
├── error_handling.spec.ts  # NEW - Phase 4
├── search.spec.ts          # Existing patterns reference
├── settings.spec.ts        # Mock patterns reference
├── integration_performance.spec.ts  # NDJSON mock patterns
└── utils/
    └── agent.ts            # Shared test utilities
```

### Pattern 1: Route Mocking for API Errors
**What:** Use `page.route()` to intercept API calls and return controlled error responses
**When to use:** Testing error scenarios without requiring a real backend failure
**Example:**
```typescript
// Source: settings.spec.ts lines 61-78
await page.route('**/settings/llm/backends**', async (route) => {
  await route.fulfill({
    status: 200,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ backends: [], default: null })
  });
});
```

For 500 errors (from settings.spec.ts lines 192-197):
```typescript
await page.route('**/log/config', (route) =>
  route.fulfill({
    status: 500,
    body: JSON.stringify({ error: 'Internal error' })
  })
);
```

For NDJSON search mocking (from integration_performance.spec.ts lines 199-229):
```typescript
await page.route('**/search.ndjson', async (route) => {
  // Build NDJSON response
  const ndjson = results.map((r) => JSON.stringify(r)).join('\n');
  await route.fulfill({
    status: 200,
    headers: {
      'Content-Type': 'application/x-ndjson',
      'X-Logseek-SID': 'test-session-id'
    },
    body: ndjson
  });
});
```

### Pattern 2: Error Display Selectors
**What:** Specific DOM selectors for error UI elements
**Search page error (SearchEmptyState.svelte):**
- Title: `h3` containing "搜索出错" (line 50)
- Error message: `p` element with errorMessage prop (line 51)
- Error details: `details` section with "错误详情" summary (line 57-68)
- Retry button: button text "重新搜索" (lines 98-106)

**Explorer page error (+page.svelte lines 833-903):**
- Title: `h3` containing "资源列举失败" (line 846)
- Error details: `details` section with "错误详情" summary (lines 852-866)
- Retry button: button text "重试" (line 896)
- Retry calls `loadResources(currentOrlStr)` on click

### Pattern 3: Search State Management
**What:** How search state is managed including error and loading states
**Key state properties (useSearch.svelte.ts):**
- `loading: boolean` - true during search
- `error: string | null` - error message when search fails
- `hasMore: boolean` - whether more results available
- `controller: AbortController | null` - for cancellation

**Error handling flow:**
1. API call fails → error caught in try/catch (lines 56-61)
2. `error` state set to error message
3. `hasMore` set to false
4. `loading` set to false
5. UI shows SearchEmptyState with type="error"

### Pattern 4: Search Cancellation
**What:** How search is cancelled and state cleaned up
**Methods:**
- `searchStore.cancel()` - aborts controller, sets loading=false, hasMore=false (lines 109-117)
- `searchStore.cleanup()` - calls cancel() + deletes backend session + resets state (lines 122-130)
- `searchStore.search()` - creates new AbortController, resets state (lines 27-61)

**After cancellation, UI should:**
- Loading spinner gone (loading = false)
- Error state null (error = null in new search)
- Can initiate new search

### Anti-Patterns to Avoid
- **Don't rely on real backend errors:** Backend might not fail reliably in CI; always use route mocking
- **Don't use fixed sleep delays:** Use Playwright's built-in waiting (waitForSelector, toBeVisible with timeout)
- **Don't verify retry result success:** Only verify state reset, not that retry succeeds (that's integration test scope)

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| API error mocking | Custom server error injection | Playwright `page.route()` | Built-in, reliable, no server restart needed |
| Network timeout simulation | Custom proxy/fault injection | Playwright `route.abort('timedout')` | Native browser-level simulation |
| Error response format | Custom JSON structure | RFC 7807 Problem Details `{detail: string}` | Matches actual backend format |

**Key insight:** The backend error response format is standardized (RFC 7807). Mock responses should include `detail` field to match real API behavior.

## Common Pitfalls

### Pitfall 1: Wrong Search API Endpoint
**What goes wrong:** Using wrong URL pattern for route interception
**Why it happens:** Search uses `/api/v1/logseek/search.ndjson`, not `/search.ndjson`
**How to avoid:** Use `'**/search.ndjson'` pattern to match regardless of base path
**Warning signs:** Route never matches, search hits real backend

### Pitfall 2: NDJSON Content-Type Header Missing
**What goes wrong:** Frontend fails to parse search results from mock
**Why it happens:** Frontend expects `application/x-ndjson` content type
**How to avoid:** Always include `'Content-Type': 'application/x-ndjson'` in mock response headers
**Warning signs:** Stream reader errors, no results displayed

### Pitfall 3: Missing X-Logseek-SID Header
**What goes wrong:** Search session ID not extracted, cleanup fails
**Why it happens:** Frontend extracts session from `X-Logseek-SID` response header
**How to avoid:** Include `'X-Logseek-SID': 'mock-session-id'` header in mock
**Warning signs:** Session cleanup errors in console

### Pitfall 4: AbortError Not Showing Error UI
**What goes wrong:** Search cancellation doesn't display error state
**Why it happens:** AbortController abort throws DOMException with name "AbortError", which the catch block handles but may not set error state (see useSearch.svelte.ts lines 56-61)
**How to avoid:** Cancel should clear state, not set error. Test cancellation verifies state cleanup, not error display.
**Warning signs:** Test expects error UI after cancel, but only loading state clears

## Code Examples

### Verified patterns from official sources:

### Mock 500 Error Response for Search
```typescript
// Pattern from settings.spec.ts adapted for search API
await page.route('**/search.ndjson', (route) =>
  route.fulfill({
    status: 500,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ detail: 'Internal Server Error' })
  })
);
```

### Mock Network Timeout
```typescript
// Pattern from Playwright docs
await page.route('**/search.ndjson', (route) => route.abort('timedout'));
```

### Test Error Display in Search Page
```typescript
// Based on SearchEmptyState.svelte selectors
// h3 = "搜索出错", retry button = "重新搜索"
const errorTitle = page.locator('h3', { hasText: '搜索出错' });
await expect(errorTitle).toBeVisible();

const errorMessage = page.locator('p', { hasText: /Internal Server Error/ });
await expect(errorMessage).toBeVisible();

const retryButton = page.getByRole('button', { name: '重新搜索' });
await expect(retryButton).toBeVisible();
```

### Test Error Display in Explorer Page
```typescript
// Based on explorer/+page.svelte selectors
// h3 = "资源列举失败", retry button = "重试"
const errorTitle = page.locator('h3', { hasText: '资源列举失败' });
await expect(errorTitle).toBeVisible();

const errorDetails = page.locator('summary', { hasText: '错误详情' });
await expect(errorDetails).toBeVisible();

const retryButton = page.getByRole('button', { name: '重试' });
await expect(retryButton).toBeVisible();
```

### Test Search Cancellation State Cleanup
```typescript
// Based on useSearch.svelte.ts cancel() method
// After cancel: loading=false, hasMore=false
// User can start new search
await page.goto('/search');
await page.waitForLoadState('networkidle');

const searchInput = page.getByPlaceholder('搜索...');
await searchInput.fill('test');
await searchInput.press('Enter');

// Wait for loading to start
await expect(page.locator('.animate-spin')).toBeVisible();

// Trigger cancellation (e.g., clear input, navigate away, or use X button)
const clearButton = page.locator('button[aria-label="清除搜索内容"]');
await clearButton.click();

// Verify loading spinner is gone
await expect(page.locator('.animate-spin')).not.toBeVisible();

// Verify can start new search
await searchInput.fill('new search');
await searchInput.press('Enter');
// Should work without errors
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Test real backend errors | Mock with `page.route()` | Established pattern | Reliable, CI-safe tests |
| Check `body` visibility | Check specific error elements | Phase 1-3 tightening | More meaningful assertions |

**Deprecated/outdated:**
- Testing error by waiting for backend to fail: Use route mocking instead
- Checking generic container visibility: Use specific error element selectors

## Open Questions

1. **Should ERROR-04 test the AbortController directly or via UI?**
   - What we know: Cancel button is in the header (X icon), clears search state
   - What's unclear: Whether clicking X triggers cancel() or just cleanup()
   - Recommendation: Test via UI interaction (click X button) to match user behavior

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Playwright (configured in playwright.config.ts) |
| Config file | web/playwright.config.ts |
| Quick run command | `pnpm --dir web test:unit --run --project=server` |
| Full suite command | `npx playwright test tests/e2e/error_handling.spec.ts` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ERROR-01 | API 500 error display | e2e | `npx playwright test tests/e2e/error_handling.spec.ts -g "API 500"` | Wave 0 |
| ERROR-02 | Network timeout handling | e2e | `npx playwright test tests/e2e/error_handling.spec.ts -g "timeout"` | Wave 0 |
| ERROR-03 | Error display interaction | e2e | `npx playwright test tests/e2e/error_handling.spec.ts -g "error display"` | Wave 0 |
| ERROR-04 | Search cancellation cleanup | e2e | `npx playwright test tests/e2e/error_handling.spec.ts -g "cancellation"` | Wave 0 |

### Wave 0 Gaps
- [ ] `web/tests/e2e/error_handling.spec.ts` — new file for all 4 error tests

## Sources

### Primary (HIGH confidence)
- `web/src/routes/search/SearchEmptyState.svelte` — Error display selectors: h3 "搜索出错", retry button "重新搜索"
- `web/src/routes/search/+page.svelte` — Search page integration, error display at lines 554-561
- `web/src/routes/explorer/+page.svelte` — Explorer error display: h3 "资源列举失败", retry "重试" at lines 833-903
- `web/src/lib/modules/logseek/composables/useSearch.svelte.ts` — Search state management, cancel() and cleanup() methods
- `web/src/lib/modules/logseek/api/search.ts` — API client, error handling with RFC 7807 parsing
- `backend/opsbox-core/src/error.rs` — RFC 7807 Problem Details response format with `detail` field

### Secondary (MEDIUM confidence)
- `web/tests/e2e/settings.spec.ts` — Route mocking patterns for error responses (lines 61-78, 192-197)
- `web/tests/e2e/integration_performance.spec.ts` — NDJSON route mocking patterns (lines 199-289)
- `web/tests/e2e/search.spec.ts` — Search test setup patterns (beforeEach, waitForLoadState)
- `web/playwright.config.ts` — Test configuration, baseURL, webServer setup

### Tertiary (LOW confidence)
- None — all findings verified with source code

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — No new dependencies needed, all patterns established
- Architecture: HIGH — All selectors and state management verified in source
- Pitfalls: HIGH — All potential issues identified from existing test patterns

**Research date:** 2026-03-14
**Valid until:** 2026-04-14 (30 days - stable codebase)
