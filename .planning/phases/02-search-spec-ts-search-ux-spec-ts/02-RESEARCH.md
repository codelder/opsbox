# Phase 2: Tighten search.spec.ts and search_ux.spec.ts Assertions - Research

**Researched:** 2026-03-14
**Domain:** Playwright E2E test assertion tightening for search functionality
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Regex Replacement Strategy
- Extract and verify specific numbers: parse the number from result text, use `expect(count).toBeGreaterThanOrEqual(0)` or `expect(count).toBeGreaterThan(0)` to verify
- Tighten both ends:
  - `waitForFunction`: use more specific conditions (wait for `.text-lg.font-semibold` to have specific text content, not just format matching)
  - Final `expect`: verify specific numeric values, not just `\d+` format matching
- Empty state test: verify exact `toContainText('0 个结果')` with unique keyword ensuring 0 results

#### Conditional Assertion Handling
- Separate search completion verification from result element verification:
  1. First verify search completion (waitForFunction waits for result text to appear)
  2. Extract specific number
  3. If number > 0, verify result elements exist
- Remove all nested conditionals: `if (count > 0) { if (highlightCount > 0) { expect(...) } }` becomes direct `expect(highlightCount).toBeGreaterThan(0)`
- Remove `if (buttonCount > 0)` wrapping, use direct `expect(buttonCount).toBeGreaterThan(0)` or use `waitFor`

#### Highlight Verification Depth
- Existence + exact text matching: verify highlight elements exist, and text matches search keyword exactly (case-insensitive)
- Use `expect(highlightText?.toUpperCase()).toMatch(/CRITICAL|ERROR/)` to verify

#### File Path Verification
- Card existence + content length verification: verify result cards exist, and first card text length exceeds a threshold (has substantive content)
- `expect(cardText?.length).toBeGreaterThan(50)` or another reasonable threshold

#### Search Data Dependencies
- Universal keywords + graceful degradation: use common keywords (error, info, CRITICAL, etc.) that will likely return results
- No mock search API (keep real integration test nature)
- If 0 results returned, use `.toBeGreaterThanOrEqual(0)` not `.toBeGreaterThan(0)`
- Empty state test uses unique keyword `NONEXISTENT_KEYWORD_XYZ123_UNLIKELY_TO_MATCH`, can verify exact `0 个结果`

### Claude's Discretion
- Specific numeric extraction regex expressions
- `waitForFunction` specific waiting conditions
- Card content length threshold determination
- Whether `test.skip()` or `test.fail()` markers are needed when search environment has no data

</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| ASSERT-02 | Tighten `search.spec.ts` -- replace `\d+` regex, verify specific result count or empty state | All findings in this document enable implementation |
| ASSERT-03 | Tighten `search_ux.spec.ts` -- remove nested conditionals, check highlight text and file path | All findings in this document enable implementation |
</phase_requirements>

## Summary

This research covers how to replace loose `/\d+\s*个结果/` regex assertions and conditional `if (count > 0)` wrappers with specific, meaningful Playwright assertions in `search.spec.ts` and `search_ux.spec.ts`. Both files have significant problems where assertions pass even when features are broken because they only verify format patterns or skip assertions entirely when conditions are not met.

**Primary recommendation:** Replace each `/regex/` assertion with numeric extraction and concrete value verification using `expect(count).toBeGreaterThan(0)`, remove all conditional wrapping to make assertions fail-fast, and add concrete text content verification for highlights and file paths.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| @playwright/test | (from project) | E2E testing framework | Project standard, already in use |

### Key Playwright APIs for This Phase
| API | Purpose | When to Use |
|-----|---------|-------------|
| `waitForFunction()` | Wait for DOM state | Wait for search result text to appear |
| `locator.textContent()` | Get element text | Extract result count text |
| `String.match()` | Extract numeric value | Parse number from "X 个结果" text |
| `expect(locator).toContainText()` | Text content assertion | Verify "0 个结果" exactly |
| `locator.count()` | Count elements | Verify buttons/highlights exist |
| `expect(count).toBeGreaterThan(0)` | Numeric assertion | Fail if zero elements |
| `expect(count).toBeGreaterThanOrEqual(0)` | Numeric assertion | Accept zero results gracefully |

## Architecture Patterns

### Search Page DOM Structure (verified from source)

From `web/src/routes/search/+page.svelte`:
```
h2.text-lg.font-semibold
  -- When count > 0: "{filteredCount} 个结果"
  -- When 0 results (not loading, has query): "0 个结果"
  -- Default: "搜索结果"

aside
  └── button (sidebar filter buttons, one per endpoint type/node)

main
  └── SearchResultCard (repeated per result)
      ├── [data-result-card="{index}"] attribute
      ├── header with file path link + action buttons
      │   └── Button[title="在新窗口打开"]
      └── table rows with highlighted text
          └── <mark class="highlight">keyword</mark>
```

### Pattern: Extract and Verify Specific Number from Result Count

**Before (loose):**
```typescript
expect(resultsText).toMatch(/\d+\s*个结果/);
```

**After (tight):**
```typescript
const match = resultsText?.match(/(\d+)\s*个结果/);
const count = match ? parseInt(match[1], 10) : 0;
expect(count).toBeGreaterThanOrEqual(0); // or toBeGreaterThan(0) when expecting results
```

### Pattern: Tighten waitForFunction for Search Completion

**Before (loose -- only checks format):**
```typescript
await page.waitForFunction(
  () => {
    const el = document.querySelector('.text-lg.font-semibold');
    return el && /\d+\s*个结果/.test(el.textContent || '');
  },
  { timeout: 60000 }
);
```

**After (tight -- also excludes placeholder "搜索结果" text):**
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

Note: The `!text.includes('搜索结果')` check is already present in some tests but missing in others. The key improvement is ensuring all waitForFunction calls include this filter.

### Pattern: Remove Conditional Wrapping for Result Cards

**Before (conditional -- hides failures):**
```typescript
if (count > 0) {
  const cards = page.locator('[data-result-card], .rounded.border');
  await expect(cards.first()).toBeVisible({ timeout: 5000 });
}
```

**After (strict -- fails fast if no results when expecting them):**
```typescript
// Wait for result cards to render (with timeout for race condition)
await page.waitForSelector('[data-result-card]', { timeout: 10000 });
const cards = page.locator('[data-result-card]');
await expect(cards.first()).toBeVisible();
```

When results are not guaranteed (integration test without mock), use:
```typescript
const cardCount = await cards.count();
if (cardCount > 0) {
  await expect(cards.first()).toBeVisible();
}
```
But document this as a data-dependency limitation, not a real assertion.

### Pattern: Verify Highlight Text Content

**Before (no text verification):**
```typescript
const highlights = page.locator('mark, .highlight, [style*="background-color"]');
const highlightCount = await highlights.count();
if (highlightCount > 0) {
  // ... some checks but no text verification
}
```

**After (with text verification):**
```typescript
// Use the correct selector based on source: mark.highlight
const highlights = page.locator('mark.highlight');
await page.waitForSelector('mark.highlight', { timeout: 5000 }).catch(() => {});
const highlightCount = await highlights.count();
expect(highlightCount).toBeGreaterThan(0);

if (highlightCount > 0) {
  const highlightText = await highlights.first().textContent();
  expect(highlightText?.toUpperCase()).toMatch(/CRITICAL|ERROR/);
}
```

Note: The actual highlight selector is `mark.highlight` (verified from `highlight.ts` line 86: `<mark class="highlight">`). The test's current selector `mark, .highlight, [style*="background-color"]` is overly broad.

### Pattern: Verify File Path in Result Cards

**Before (weak -- only checks length > 0):**
```typescript
if (cardCount > 0) {
  const cardText = await cards.first().textContent();
  expect(cardText?.length).toBeGreaterThan(0);
}
```

**After (meaningful -- checks for substantive content):**
```typescript
if (cardCount > 0) {
  const cardText = await cards.first().textContent();
  expect(cardText?.length).toBeGreaterThan(50); // Card with file path + content should be substantial
}
```

### Pattern: Empty State Exact Verification

**Before (loose -- same regex as positive results):**
```typescript
expect(resultsText).toMatch(/\d+\s*个结果/);
```

**After (tight -- verifies exact "0 个结果"):**
```typescript
await expect(page.locator('.text-lg.font-semibold')).toContainText('0 个结果');
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Wait for search completion | Custom polling loops | `waitForFunction()` with specific condition | Built-in timeout, better error messages |
| Extract number from text | Complex string manipulation | `String.match(/(\d+)/)` + `parseInt()` | Simple, reliable |
| Wait for element existence | `if (count > 0)` checks | `waitForSelector()` with timeout | Fail-fast, no silent skips |
| Verify highlight exists | Checking multiple selectors | `locator('mark.highlight')` | Matches actual source code |

## Common Pitfalls

### Pitfall 1: Search Returns 0 Results (No Data Environment)
**What goes wrong:** Tests fail because the search environment has no data to search
**Why it happens:** E2E tests run against real backend without seeded data
**How to avoid:** Use `toBeGreaterThanOrEqual(0)` when results are data-dependent; use `toBeGreaterThan(0)` only when you can guarantee data exists
**Warning signs:** All search tests consistently returning 0 results

### Pitfall 2: Race Condition Between Search Completion and DOM Rendering
**What goes wrong:** `waitForFunction` detects "X 个结果" but cards haven't rendered yet
**Why it happens:** NDJSON streaming completes count update before card rendering
**How to avoid:** After waitForFunction, add `await page.waitForTimeout(500-1000)` or `waitForSelector('[data-result-card]')`
**Warning signs:** Intermittent "element not found" errors after search completion

### Pitfall 3: Highlight Selector Mismatch
**What goes wrong:** Tests look for `mark, .highlight, [style*="background-color"]` but actual highlights are only `<mark class="highlight">`
**Why it happens:** Test was written without checking source code
**How to avoid:** Use `mark.highlight` as primary selector (verified from `highlight.ts` line 86)
**Warning signs:** Tests pass even when highlight feature is broken

### Pitfall 4: "搜索结果" Placeholder vs Real Results
**What goes wrong:** `waitForFunction` matches "搜索结果" placeholder text before search completes
**Why it happens:** Initial page shows "搜索结果" in the heading before any search
**How to avoid:** Add `!text.includes('搜索结果')` filter in waitForFunction
**Warning signs:** Tests complete too fast, no actual search performed

### Pitfall 5: Sidebar Buttons May Not Exist Without Results
**What goes wrong:** `aside button` count is 0 when no results exist (tree nodes with count=0 are hidden)
**Why it happens:** Sidebar only shows nodes with `count > 0` (line 479 of +page.svelte: `{#if depth === 0 || node.count > 0}`)
**How to avoid:** Check sidebar button assertions only when results exist; use graceful degradation
**Warning signs:** Button count is 0 even after successful search

## Code Examples

### Verified Patterns from Source Code

**Highlight selector** (from `highlight.ts` line 86):
```typescript
// Source: web/src/lib/modules/logseek/utils/highlight.ts
// Generates: <mark class="highlight">keyword</mark>
page.locator('mark.highlight')
```

**Result count text** (from `+page.svelte` lines 528-537):
```typescript
// Source: web/src/routes/search/+page.svelte
// When results: "{count} 个结果"
// When zero: "0 个结果"
// Initial: "搜索结果"
page.locator('.text-lg.font-semibold')
```

**Result card selector** (from `SearchResultCard.svelte` line 171):
```typescript
// Source: web/src/routes/search/SearchResultCard.svelte
// Card has: data-result-card={index}
page.locator('[data-result-card]')
```

**Open in new window button** (from `SearchResultCard.svelte` line 366):
```typescript
// Source: web/src/routes/search/SearchResultCard.svelte
// Button has: title="在新窗口打开"
page.getByTitle('在新窗口打开')
```

**Sidebar buttons** (from `+page.svelte` line 480-486):
```typescript
// Source: web/src/routes/search/+page.svelte
// Buttons are: aside > div > button
// Only visible when node.count > 0 (line 479)
page.locator('aside button')
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `expect(text).toMatch(/\d+\s*个结果/)` | Extract number + `expect(count).toBeGreaterThan(0)` | This phase | Fails when count is 0 |
| `if (count > 0) { expect(...)` | `waitForSelector` + direct expect | This phase | Failures no longer silently skipped |
| `mark, .highlight, [style*="background-color"]` | `mark.highlight` | This phase | Matches actual DOM structure |
| `expect(cardText?.length).toBeGreaterThan(0)` | `expect(cardText?.length).toBeGreaterThan(50)` | This phase | Verifies substantive content |

**Deprecated/outdated:**
- Multiple CSS selector approach for highlights: use `mark.highlight` which matches the actual `<mark class="highlight">` from `highlight.ts`
- Conditional assertion wrapping: removed entirely; tests should fail fast when features break

## Open Questions

1. **Should we use `toBeGreaterThan(0)` or `toBeGreaterThanOrEqual(0)` for result counts?**
   - What we know: Search environment may or may not have data
   - What's unclear: Whether the E2E backend has seeded test data
   - Recommendation: Use `toBeGreaterThanOrEqual(0)` for generic keyword searches; reserve `toBeGreaterThan(0)` for the empty state test (which uses a unique keyword to guarantee 0 results)

2. **What threshold for card content length?**
   - What we know: SearchResultCard displays file path (~20-50 chars) + matching lines
   - What's unclear: Minimum expected content length
   - Recommendation: Use `toBeGreaterThan(50)` -- a card with just a file path is ~20-30 chars, with actual content should be 50+

3. **Should we add `waitForSelector` for cards before assertions?**
   - What we know: Race condition exists between count display and card rendering
   - What's unclear: Whether 500ms timeout is sufficient
   - Recommendation: Use `waitForSelector('[data-result-card]', { timeout: 10000 }).catch(() => {})` to handle gracefully when 0 results

## Validation Architecture

> Enabled (workflow.nyquist_validation not set to false in config.json)

### Test Framework
| Property | Value |
|----------|-------|
| Framework | @playwright/test |
| Config file | web/playwright.config.ts |
| Quick run command | `cd web && npx playwright test tests/e2e/search.spec.ts tests/e2e/search_ux.spec.ts` |
| Full suite command | `cd web && npx playwright test` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ASSERT-02 | search.spec.ts assertions are tight and meaningful | E2E | `npx playwright test tests/e2e/search.spec.ts` | YES |
| ASSERT-03 | search_ux.spec.ts assertions are tight and meaningful | E2E | `npx playwright test tests/e2e/search_ux.spec.ts` | YES |

### Sampling Rate
- Per task commit: `npx playwright test tests/e2e/search.spec.ts tests/e2e/search_ux.spec.ts`
- Phase gate: All 10 tests pass before `/gsd:verify-work`

### Wave 0 Gaps
- None -- existing test files `web/tests/e2e/search.spec.ts` (5 tests) and `web/tests/e2e/search_ux.spec.ts` (5 tests) cover this phase

## Sources

### Primary (HIGH confidence)
- `web/src/routes/search/+page.svelte` -- Result count display (lines 527-538), sidebar tree rendering (lines 454-518), filter logic (lines 220-264)
- `web/src/routes/search/SearchResultCard.svelte` -- Card structure with `data-result-card` (line 171), open button `title="在新窗口打开"` (line 366), highlight rendering
- `web/src/lib/modules/logseek/utils/highlight.ts` -- Generates `<mark class="highlight">` (line 86), case-insensitive literal matching
- `web/tests/e2e/search.spec.ts` -- Current test file with 5 tests, 10 regex instances to tighten, 2 conditional wrappers
- `web/tests/e2e/search_ux.spec.ts` -- Current test file with 5 tests, 10+ regex instances, 4 nested conditional blocks
- Playwright docs: https://playwright.dev/docs/api/class-page#page-wait-for-function
- Playwright docs: https://playwright.dev/docs/assertions

### Secondary (MEDIUM confidence)
- Phase 1 research and implementation patterns (settings.spec.ts tightening)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- @playwright/test already in use, APIs verified in project
- Architecture: HIGH -- All Svelte components read directly, selectors verified against source code
- Pitfalls: HIGH -- Race conditions and data dependency issues well-understood from examining actual code

**Research date:** 2026-03-14
**Valid until:** 2026-04-14 (30 days -- stable Playwright patterns)
