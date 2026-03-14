# E2E Testing Pitfalls and Prevention Guide

Research document for OpsBox E2E testing (Playwright + SvelteKit + Rust backend).

## Table of Contents

1. [Assertion Pitfalls](#1-assertion-pitfalls)
2. [Test Design Pitfalls](#2-test-design-pitfalls)
3. [Coverage Pitfalls](#3-coverage-pitfalls)
4. [Maintenance Pitfalls](#4-maintenance-pitfalls)
5. [OpsBox-Specific Observations](#5-opsbox-specific-observations)
6. [Detection Strategies](#6-detection-strategies)
7. [Prevention Approaches](#7-prevention-approaches)
8. [Remediation Priorities](#8-remediation-priorities)

---

## 1. Assertion Pitfalls

### 1.1 False Positives (Tests Pass When They Should Not)

**Problem**: Tests that appear to pass but do not actually verify the intended behavior.

**Observed pattern in OpsBox** (`web/tests/e2e/search.spec.ts`):
```typescript
// VULNERABLE: Only checks regex matches "digits + result", never validates actual count
const resultsText = await page.locator('.text-lg.font-semibold').textContent();
expect(resultsText).toMatch(/\d+\s*个结果/);
```

This assertion passes when results text is "0 个结果", "1 个结果", or "999 个结果". The test cannot distinguish between successful results and empty results.

**Other false positive patterns found**:

1. **Tautological assertions** (`settings.spec.ts` line 172):
```typescript
const newClass = (await page.locator('html').getAttribute('class')) || '';
expect(newClass).toBeDefined();  // Always passes - string || '' is never undefined
```

2. **Body visibility as success indicator** (multiple files):
```typescript
// Appears in settings.spec.ts, image_viewer.spec.ts, explorer_interaction.spec.ts
await expect(page.locator('body')).toBeVisible();
// Body is ALWAYS visible on any rendered page - even error pages
```

3. **Guard clause tests** (`search_ux.spec.ts` lines 40-56):
```typescript
if (count > 0) {
  // assertions here only run when results exist
}
// Test passes even with 0 results - the "real" assertion is just regex match
```

4. **Catch-all fallback** (`search.spec.ts` line 159):
```typescript
const resultsText = await page
  .locator('.text-lg.font-semibold')
  .textContent()
  .catch(() => '0 个结果');  // Fallback makes assertion always pass
expect(resultsText).toMatch(/\d+\s*个结果/);
```

**Detection**: Mark any assertion that would pass on a blank page, error page, or 0-result state.

**Prevention**: Use explicit value assertions instead of format assertions:
```typescript
// GOOD: Verify actual result count matches expectation
await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果', {
  timeout: 10000
});
// OR use numeric extraction with explicit bounds
const count = await getResultCount(page);
expect(count).toBeGreaterThan(0);
```

---

### 1.2 Overly Specific Assertions (Brittle Tests)

**Problem**: Assertions that break with minor UI changes unrelated to the feature being tested.

**Patterns to avoid**:

1. **CSS class selectors** (`search.spec.ts` line 27):
```typescript
const el = document.querySelector('.text-lg.font-semibold');
// Breaks if Tailwind classes change or component refactors
```

2. **Icon-based element selection** (`explorer_interaction.spec.ts` line 127):
```typescript
const listBtn = page.locator('button').filter({ has: page.locator('.lucide-layout-list') });
// Breaks if icon library changes or icon name changes
```

3. **Hardcoded URL patterns** (`integration_explorer.spec.ts` lines 224, 240):
```typescript
await expect(gzLink).toHaveAttribute('href', /file=orl%3A%2F%2Flocal%2F.*archive\.tar/);
// URL encoding assumptions can break across browsers
```

**Prevention**: Use semantic locators (`getByRole`, `getByLabel`, `getByText` with exact: true) and data-testid attributes.

---

### 1.3 Timing Issues (Race Conditions)

**Problem**: Tests rely on timing assumptions rather than deterministic state transitions.

**Observed anti-patterns in OpsBox**:

1. **Arbitrary `waitForTimeout`** (found in 8 test files):
```typescript
// search_ux.spec.ts line 42, 65, 96, 159
await page.waitForTimeout(1000);  // "wait for results to render"
// explorer_interaction.spec.ts line 113, 131, 169
await page.waitForTimeout(500);   // "wait for theme toggle"
// image_viewer.spec.ts line 213, 260, 287, 310, etc.
await page.waitForTimeout(500);   // "wait after keyboard press"
```

2. **Inconsistent timeout values across similar operations**:
- 200ms in `image_viewer.spec.ts` line 213
- 300ms in `explorer_interaction.spec.ts` line 131
- 500ms in `search_ux.spec.ts` line 65
- 1000ms in `search_ux.spec.ts` line 42

3. **Fallback to fixed delays when smart waiting fails** (`integration_explorer.spec.ts` lines 83-84):
```typescript
} catch (error) {
  console.log(`Falling back to fixed 10-second wait...`);
  await new Promise((resolve) => setTimeout(resolve, 10000));
}
```

**Prevention**: Use Playwright's built-in waiting mechanisms:
```typescript
// GOOD: Wait for specific state
await expect(element).toBeVisible({ timeout: 5000 });
await page.waitForLoadState('networkidle');
await page.waitForURL(/expected-pattern/);

// GOOD: Wait for API response
const responsePromise = page.waitForResponse('**/api/v1/logseek/search**');
await searchInput.press('Enter');
await responsePromise;
```

---

### 1.4 Element Selection Fragility

**Problem**: Locators that are too broad or ambiguous.

**Observed patterns**:

1. **Generic selectors that match multiple elements**:
```typescript
// search.spec.ts line 53
const sidebarButtons = page.locator('aside button');
// Could match any button in any aside element

// settings.spec.ts line 162
const themeButton = page.getByRole('button', { name: /theme|主题|toggle/i });
// Regex is broad - could match unexpected buttons
```

2. **Text-based selectors with partial matching risk**:
```typescript
// integration_explorer.spec.ts line 222
const agentItem = page.locator(`text=${AGENT_ID}`).first();
// Could match partial text in unexpected elements
```

3. **Using `.first()` to resolve ambiguity** (masking potential issues):
```typescript
// home.spec.ts line 43-44
await expect(orButton.first()).toBeVisible();
await expect(andButton.first()).toBeVisible();
// If multiple matches exist, which one is "correct"?
```

**Prevention**: Use specific roles, labels, or test IDs:
```typescript
// GOOD
page.getByRole('button', { name: 'Local Machine', exact: true })
page.getByTestId('sidebar-filter-local')
page.locator('[data-result-card]').filter({ hasText: expectedContent })
```

---

## 2. Test Design Pitfalls

### 2.1 Testing Implementation vs Behavior

**Problem**: Tests verify how the app works rather than what it accomplishes.

**Observed patterns**:

1. **Testing URL format rather than navigation outcome** (`integration_explorer.spec.ts`):
```typescript
// Tests URL encoding rather than "user can see files"
await expect(page).toHaveURL(new RegExp(encodeURIComponent(agentRootOrl)));
```

2. **Testing request body fields** (`integration_explorer.spec.ts` lines 128-130):
```typescript
expect(requests[0]).toHaveProperty('orl');
expect(requests[0]).not.toHaveProperty('odfi');
// This is testing API contract at E2E level - better as API integration test
```

3. **Checking for absence of error text rather than presence of success** (`explorer_interaction.spec.ts` line 121):
```typescript
await expect(page.locator('body')).not.toContainText(/error|错误/i);
// Absence of error text does not confirm correct behavior
```

**Prevention**: Focus on user-observable outcomes:
```typescript
// GOOD: User can see the files they expect
await expect(page.getByText('test.txt')).toBeVisible();
await expect(page.getByText('test.log')).toBeVisible();
```

---

### 2.2 Excessive Mocking

**Problem**: Mocking undermines the "real integration" value of E2E tests.

**Observed patterns** (`settings.spec.ts`):

1. **Route interception that bypasses real backend** (lines 67-81):
```typescript
await page.route('**/settings/llm/backends**', async (route) => {
  await route.fulfill({
    status: 200,
    body: JSON.stringify([{ name: 'ollama-local', ... }])
  });
});
// This tests the UI rendering mock data, not real LLM configuration
```

2. **Partial mocking creates inconsistent state** (lines 181-186):
```typescript
// Mocks only /log/config, leaving other API calls real
await page.route('**/log/config', (route) =>
  route.fulfill({ status: 500, body: JSON.stringify({ error: 'Internal error' }) })
);
```

**Prevention**: Reserve E2E tests for real integration. Use component tests for mock-based scenarios:
- E2E: Test with real backend services
- Component tests: Test UI with mocked API responses
- API tests: Test backend behavior directly

---

### 2.3 Test Interdependencies

**Problem**: Tests that depend on state from previous tests.

**Observed mitigation** (`integration_local.spec.ts`, `integration_explorer.spec.ts`):
```typescript
test.describe.configure({ mode: 'serial' });
```

**Issues with serial mode**:
- Slower execution (no parallelism)
- One failure cascades to all subsequent tests
- State leaks if cleanup fails

**Observed pattern** (`integration_explorer.spec.ts` line 73):
```typescript
const RUN_ID = Date.now();
const TEST_ROOT_DIR = path.join(__dirname, `temp_explorer_${RUN_ID}`);
```

This timestamp-based isolation helps, but tests within the same file still share state.

**Prevention**:
1. Each test creates its own isolated data
2. Use `test.beforeEach` for setup, not just `test.beforeAll`
3. Consider Playwright's `storageState` for session isolation
4. Use unique identifiers per test, not per file

---

### 2.4 Flaky Test Causes and Fixes

**Common causes identified in OpsBox tests**:

| Cause | Example | Fix |
|-------|---------|-----|
| Race conditions | `waitForTimeout(500)` after click | Use `waitForResponse` or `waitForLoadState` |
| Network timing | Backend compilation delays | Increase timeout, use health check polling |
| Parallel resource contention | Multiple agents on same port | Use `getFreePort()` utility |
| DOM state assumptions | "body visible" means page works | Assert specific content |
| Floating timestamps | `Date.now()` in test data | Use `RUN_ID` pattern consistently |

**Observed resilience patterns** (good practices):

1. **Health check polling** (`utils/agent.ts`):
```typescript
export async function waitForAgentReady(request, agentId, maxWait, interval) {
  while (Date.now() - start < maxWait) {
    const response = await request.get(`http://127.0.0.1:4001/api/v1/agents/${agentId}`);
    if (response.ok()) return;
    await new Promise((r) => setTimeout(r, interval));
  }
}
```

2. **Global setup cleanup** (`global-setup.ts`):
```typescript
performCleanup(true, false);  // Clean leftover resources from interrupted runs
```

---

## 3. Coverage Pitfalls

### 3.1 Illusion of Coverage

**Problem**: Tests exist and pass, but do not meaningfully verify behavior.

**Analysis of OpsBox tests by coverage quality**:

| Test File | Total Tests | Trivial Assertions | Real Verification |
|-----------|-------------|-------------------|-------------------|
| `settings.spec.ts` | 14 | 12 (86%) | 2 (14%) |
| `home.spec.ts` | 9 | 6 (67%) | 3 (33%) |
| `search.spec.ts` | 6 | 4 (67%) | 2 (33%) |
| `image_viewer.spec.ts` | 15 | 8 (53%) | 7 (47%) |
| `integration_local.spec.ts` | 4 | 0 (0%) | 4 (100%) |
| `integration_query_syntax.spec.ts` | 11 | 0 (0%) | 11 (100%) |
| `integration_explorer.spec.ts` | 12 | 1 (8%) | 11 (92%) |

**Trivial assertion patterns**:
- `expect(page.locator('body')).toBeVisible()` (15 occurrences)
- `expect(something).toBeDefined()` (3 occurrences)
- `expect(content.length).toBeGreaterThan(0)` (4 occurrences)
- Conditional test bodies with no else clause (7 occurrences)

**Prevention**: Require meaningful assertions:
```typescript
// BAD: Illusion of coverage
test('should display search results', async ({ page }) => {
  await page.goto('/search');
  await expect(page.locator('body')).toBeVisible();
});

// GOOD: Verifies actual behavior
test('should display search results', async ({ page }) => {
  await page.goto('/search');
  await searchInput.fill('test query');
  await searchInput.press('Enter');
  await expect(page.locator('[data-result-card]').first()).toBeVisible({ timeout: 10000 });
  await expect(page.getByText('1 个结果')).toBeVisible();
});
```

---

### 3.2 Missing Negative Cases

**Problem**: Tests only verify happy paths.

**Gaps identified in OpsBox**:

1. **Search module**: No tests for:
   - Invalid query syntax (malformed regex, unmatched quotes)
   - Query injection attempts
   - Very long queries (>10000 chars)
   - Special Unicode in queries
   - Empty query submission

2. **Explorer module**: No tests for:
   - Symlink following behavior
   - Permission denied on file system
   - Binary file handling
   - Extremely deep directory nesting
   - Circular archive entries

3. **Image viewer**: No tests for:
   - Corrupted image files
   - Extremely large images (>100MB)
   - SVG/animated GIF handling
   - Image with malicious EXIF data

**Prevention**: Implement test matrix:
```
For each feature:
  - Happy path (normal usage)
  - Boundary values (min/max/empty)
  - Error conditions (invalid input, missing data)
  - Security (injection, traversal)
  - Performance (large inputs)
```

---

### 3.3 Boundary Condition Gaps

**Problem**: Tests use only "typical" data, missing edge cases.

**Observed in OpsBox**:

1. **Timestamp collision risk** (`integration_local.spec.ts` line 72):
```typescript
const RUN_ID = Date.now();
// If tests run within same millisecond, collision is possible
```

2. **No empty directory tests**: All directory tests have files
3. **No single-file directory tests**: Always use multiple files
4. **No special character tests except Chinese** (`integration_explorer.spec.ts`):
   - Missing: Arabic RTL, emoji filenames, spaces, quotes, newlines in names

**Prevention**: Add boundary test cases:
```typescript
test.describe('Boundary conditions', () => {
  test('empty directory shows empty state');
  test('file with spaces in name');
  test('file with special chars: !@#$%^&()');
  test('extremely long filename (255 chars)');
  test('directory with 10000+ files');
});
```

---

### 3.4 Error Path Neglect

**Problem**: Error handling is verified with weak assertions.

**Observed pattern** (`explorer_interaction.spec.ts` lines 217-223):
```typescript
test('should handle non-existent directory gracefully', async ({ page }) => {
  await page.goto(`/explorer?orl=${encodeURIComponent('orl://local/non/existent/path/12345')}`);
  await page.waitForLoadState('networkidle');
  await expect(page.locator('body')).toBeVisible();  // Just checks page renders
});
```

**What should be verified**:
- Error message is user-friendly
- Error does not expose internal paths or stack traces
- User can navigate away from error state
- Error is logged for debugging

**Prevention**:
```typescript
test('should handle non-existent directory gracefully', async ({ page }) => {
  await page.goto('/explorer?orl=orl://local/nonexistent');
  await page.waitForLoadState('networkidle');

  // Verify error state
  await expect(page.getByText(/not found|does not exist/i)).toBeVisible();
  await expect(page.getByText(/stack trace|panic/i)).not.toBeVisible();

  // Verify recovery path
  await expect(page.getByRole('button', { name: /back|home/i })).toBeVisible();
});
```

---

## 4. Maintenance Pitfalls

### 4.1 Test Rot (Outdated Assertions)

**Problem**: Tests pass but verify obsolete behavior.

**Early warning signs**:
- Assertions that always pass (see false positives)
- Tests with commented-out assertions (`integration_query_syntax.spec.ts` line 267):
```typescript
// await expect(page.getByText('File not found')).not.toBeVisible();
```
- Tests that never fail in CI but "sometimes fail locally"

**Prevention**:
1. Periodically inject deliberate failures to verify tests catch regressions
2. Track test execution time - tests that become faster may be skipping work
3. Review tests when features change

---

### 4.2 Copy-Paste Test Duplication

**Problem**: Tests with nearly identical structure and duplicated logic.

**Observed patterns**:

1. **Repeated wait-for-search-complete block** (appears 15+ times):
```typescript
await page.waitForFunction(
  () => {
    const el = document.querySelector('.text-lg.font-semibold');
    const text = el?.textContent || '';
    return /\d+\s*个结果/.test(text) && !text.includes('搜索结果');
  },
  { timeout: 60000 }
);
```

2. **Repeated cleanup pattern** (appears 10+ times):
```typescript
try {
  await request.delete(`http://127.0.0.1:4001/api/v1/agents/${AGENT_ID}`);
} catch {
  // ignore
}
```

3. **Repeated agent spawn boilerplate** (`integration_explorer.spec.ts`):
   - Lines 46-70: Main agent spawn
   - Lines 300-329: Multi-root agent spawn
   - Lines 375-400: Escape test agent spawn
   - Lines 452-477: Restricted agent spawn
   - Lines 530-555: Chinese path agent spawn

**Prevention**: Extract common patterns to utility functions:
```typescript
// utils/search.ts
export async function waitForSearchComplete(page: Page, timeout = 60000) {
  await page.waitForFunction(
    () => {
      const el = document.querySelector('.text-lg.font-semibold');
      const text = el?.textContent || '';
      return /\d+\s*个结果/.test(text) && !text.includes('搜索结果');
    },
    { timeout }
  );
}

// utils/agent.ts (already partially done)
export async function cleanupAgent(request: APIRequestContext, agentId: string) {
  try {
    await request.delete(`http://127.0.0.1:4001/api/v1/agents/${agentId}`);
  } catch { /* ignore */ }
}
```

---

### 4.3 Unclear Test Intent

**Problem**: Test names and comments do not explain what behavior is being verified.

**Examples**:

1. **Vague test name** (`settings.spec.ts` line 22):
```typescript
test('should display settings page with navigation tabs', async ({ page }) => {
  // Actually just checks body is visible and has content
});
```

2. **Misleading comments** (`search.spec.ts` lines 20-21):
```typescript
// 使用通用搜索词，搜索可能返回结果（从配置的 S3 源）
await searchInput.fill('error');
// Comment says "may return results from S3" but test doesn't verify source
```

3. **Tests that say one thing and verify another** (`image_viewer.spec.ts` line 179):
```typescript
test('should have zoom controls', async ({ page }) => {
  // Actually just counts buttons - doesn't verify they are zoom controls
  const buttonCount = await buttons.count();
  expect(buttonCount).toBeGreaterThan(0);
});
```

**Prevention**: Follow naming convention:
```
should [action] [expected outcome] [when/with condition]

Examples:
- should display 1 result when searching for unique ID
- should hide sidebar when clicking collapse button
- should show error message when API returns 500
```

---

### 4.4 Missing Test Documentation

**Problem**: Complex test setups lack explanation.

**Well-documented examples** (`integration_explorer.spec.ts` lines 133-137):
```typescript
// This test case captures the bug we just fixed:
// - OrlManager was using effective_id() which mapped empty ID to "localhost"
// - This caused key to be "agent.localhost" instead of "agent.root"
// - AgentDiscoveryFileSystem was registered as "agent.root" but couldn't be found
```

**Undocumented examples** (most other tests):
- No explanation of why `Date.now()` is used for RUN_ID
- No explanation of the tar file creation helper's purpose
- No explanation of why some tests are serial while others are parallel

**Prevention**: Document:
1. **Why**: What bug/requirement motivated this test?
2. **Setup**: Why is this specific configuration needed?
3. **Assertions**: What exactly is being verified and why?
4. **Known limitations**: What edge cases are NOT covered?

---

## 5. OpsBox-Specific Observations

### 5.1 Architecture-Specific Pitfalls

1. **Backend compilation time**: Agent tests require `cargo run --release` which compiles on first use
   - **Mitigation**: Global setup pre-compiles agent binary
   - **Remaining risk**: Parallel tests may still contend for Cargo lock

2. **Port conflicts**: Tests spawn agents on random ports
   - **Good practice**: `getFreePort()` utility exists
   - **Risk**: Port may be taken between allocation and binding

3. **Database isolation**: E2E tests use separate database (`opsbox-e2e.db`)
   - **Good practice**: Database artifacts are cleaned in global setup/teardown
   - **Risk**: WAL/SHM files may persist if cleanup fails

4. **File system race conditions**: Tests create/delete temp directories
   - **Mitigation**: Serial mode for file-modifying tests
   - **Risk**: Cleanup errors are silently ignored

### 5.2 Chinese Localization Considerations

Tests use Chinese text in assertions:
```typescript
await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果');
await expect(page.getByRole('button', { name: '本地文件' })).toBeVisible();
```

**Pitfall**: These tests will fail if localization changes or if tested with different locale settings.

**Mitigation**: Use data-testid attributes or test with explicit locale configuration.

### 5.3 ORL Protocol Complexity

The ORL protocol (`orl://id@type/path?entry=...`) adds test complexity:
- URL encoding varies by browser
- Deep archive navigation (`?entry=path`) is tested but edge cases are limited
- Invalid ORL formats are not tested

---

## 6. Detection Strategies

### 6.1 Automated Detection

1. **Assertion strength analysis**:
   - Flag tests with only `toBeVisible()` on `body`
   - Flag tests with only `toBeDefined()` assertions
   - Flag tests with only regex format assertions

2. **Coverage quality metrics**:
   - Track "trivial vs meaningful" assertion ratio
   - Require minimum 2 meaningful assertions per test
   - Flag tests that pass with deliberately broken features

3. **Flakiness detection**:
   - Track test retry rates in CI
   - Flag tests with `waitForTimeout` > 200ms
   - Monitor test duration variance

### 6.2 Manual Review Checklist

Before approving E2E tests:

- [ ] Test name describes the behavior being verified
- [ ] At least one assertion verifies specific content/state
- [ ] No `waitForTimeout` without documented reason
- [ ] Error cases are tested with specific error messages
- [ ] Test can fail if the feature is broken
- [ ] Setup/teardown is isolated from other tests

---

## 7. Prevention Approaches

### 7.1 Test Writing Guidelines

1. **Every test must have a negative case**: If test `should show results`, also test `should show no-results message`

2. **Prefer explicit over implicit**:
   ```typescript
   // BAD: Implicit success
   await expect(page.locator('body')).toBeVisible();

   // GOOD: Explicit verification
   await expect(page.getByText('Search Results')).toBeVisible();
   await expect(page.getByTestId('result-count')).toHaveText('5 results');
   ```

3. **Use semantic locators**:
   ```typescript
   // BAD: CSS selector
   page.locator('.text-lg.font-semibold')

   // GOOD: Semantic locator
   page.getByRole('heading', { level: 2, name: /results/i })
   // OR: data-testid
   page.getByTestId('search-result-count')
   ```

4. **Test behavior, not implementation**:
   ```typescript
   // BAD: Testing implementation detail
   expect(requests[0]).toHaveProperty('orl');

   // GOOD: Testing user-visible behavior
   await expect(page.getByText('test.txt')).toBeVisible();
   ```

### 7.2 Code Review Standards

1. **Reject tests with**:
   - Only `body` visibility assertions
   - Conditional test bodies (if/else) without corresponding `else` test
   - `waitForTimeout` without explanation
   - Assertions that pass on error pages

2. **Require**:
   - At least one assertion that would fail if feature is broken
   - Cleanup that handles errors without silent failure
   - Documentation of what regression is being prevented

### 7.3 CI/CD Integration

1. **Mutation testing**: Periodically break features and verify tests fail
2. **Retry budget**: Track and limit test retries (current: 2 retries in CI)
3. **Duration monitoring**: Flag tests that become significantly faster or slower

---

## 8. Remediation Priorities

### Priority 1: High Impact, Low Effort

| Issue | Files Affected | Fix |
|-------|---------------|-----|
| `body` visibility assertions | 5 files, ~15 occurrences | Replace with content-specific assertions |
| Catch-all `.catch()` fallback | `search.spec.ts` | Remove fallback, let test fail |
| `tobeDefined()` tautologies | `settings.spec.ts` | Replace with specific value checks |

### Priority 2: High Impact, Medium Effort

| Issue | Files Affected | Fix |
|-------|---------------|-----|
| `waitForTimeout` replacements | 8 files, ~20 occurrences | Replace with `waitForResponse`/`waitForLoadState` |
| Duplicated search-wait logic | 6 files | Extract `waitForSearchComplete()` utility |
| Duplicated agent spawn/cleanup | 4 files | Use existing `spawnAgent()` utility consistently |

### Priority 3: Medium Impact, Medium Effort

| Issue | Files Affected | Fix |
|-------|---------------|-----|
| Add negative test cases | All test files | Create test matrix per feature |
| Add boundary conditions | `integration_local.spec.ts`, `explorer_interaction.spec.ts` | Add empty dir, special chars, etc. |
| Improve test documentation | All files | Add "motivated by" and "verifies" comments |

### Priority 4: Low Impact, High Effort

| Issue | Files Affected | Fix |
|-------|---------------|-----|
| Replace CSS selectors with semantic locators | Most files | Gradual refactor during feature work |
| Add mutation testing | N/A | Infrastructure setup required |
| Locale-independent assertions | Files with Chinese text | Add data-testid attributes |

---

## Appendix A: Assertion Quality Rubric

Rate each assertion from 1-5:

| Score | Description | Example |
|-------|-------------|---------|
| 1 | Always passes (trivial) | `expect(body).toBeVisible()` |
| 2 | Passes on any rendered page | `expect(text.length).toBeGreaterThan(0)` |
| 3 | Verifies format but not content | `expect(text).toMatch(/\d+ results/)` |
| 4 | Verifies specific content | `expect(text).toContain('3 results')` |
| 5 | Verifies content + context + failure mode | `expect(card).toContainText('error.log')` with retry logic |

**Target**: Every test should have at least one assertion scoring 4 or 5.

## Appendix B: Anti-Pattern Quick Reference

| Anti-Pattern | Example | Replacement |
|-------------|---------|-------------|
| Body visibility | `expect(body).toBeVisible()` | `expect(page.getByText('Expected')).toBeVisible()` |
| Blind regex | `.toMatch(/\d+/)` | `.toContainText('3')` |
| Arbitrary sleep | `waitForTimeout(1000)` | `waitForResponse()` |
| Conditional skip | `if (count > 0) { test }` | Separate tests for 0 and >0 |
| Catch fallback | `.catch(() => default)` | Let assertion fail |
| CSS selector | `.text-lg.font-semibold` | `getByRole('heading')` |
| Implementation test | `expect(req.body).toHaveProperty('x')` | `expect(page.getByText('result')).toBeVisible()` |

---

*Document generated: 2026-03-14*
*Source: Analysis of 17 E2E test files in `web/tests/e2e/`*
*Total tests analyzed: ~120 test cases*
