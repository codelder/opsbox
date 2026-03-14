# Phase 1: Tighten `settings.spec.ts` Assertions - Research

**Researched:** 2026-03-14
**Domain:** Playwright E2E test assertion tightening
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Body Replacement Strategy
- Locate specific elements by section: each describe block verifies its corresponding specific UI elements
- Page Layout: verify `heading('系统设置')` + tabs exist
- Planner Management: verify planner section heading/form elements
- LLM Management: verify LLM Card, heading, form elements
- S3 Profile: verify Profile Card, heading, form elements
- Agent Management: verify Agent section specific elements
- Server Log Settings: verify Log section specific elements (e.g. log level label)
- Error Handling: verify page structure still exists (heading + tabs)

#### Mock Data Verification Depth
- Verify mock data name rendered: LLM shows 'ollama-local', S3 shows 'minio-local'
- Verify mock data count: list item count matches mock data count
- Structure always verified: even in error handling scenarios, verify basic UI structure (cards, headings, forms)

#### Theme Toggle Assertions
- Check html class exact value changes: default state -> toggle -> 'dark' -> toggle -> back to original state
- Verify CSS variable value changes: check `--background` or equivalent CSS variable value changes after toggle
- Bidirectional toggle verification: toggle twice to verify return to original state

#### Conditional Assertion Handling
- All strict failures: remove all `if (count > 0)` conditional assertion wrapping
- Remove + wait for element: remove conditional wrapping while adding `waitForSelector` to give UI enough load time
- Affected tests: Settings navigation test (settings button), Theme toggle test (theme button)

### Claude's Discretion
- Specific CSS variable name selection (which CSS variable to check)
- waitForSelector timeout duration
- Error handling test "page still displays" specific assertion element selection

### Deferred Ideas (OUT OF SCOPE)
- None -- discussion stayed within phase scope

</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| ASSERT-01 | Tighten `settings.spec.ts` -- replace 10 `body` visibility checks with specific UI elements | All findings in this document enable implementation |
</phase_requirements>

## Summary

This research covers how to replace loose `body` visibility assertions with specific, meaningful Playwright assertions in `settings.spec.ts`. The file currently has 10 instances of `await expect(page.locator('body')).toBeVisible()` and multiple conditional `if (count > 0)` patterns that pass even when the actual feature is broken.

**Primary recommendation:** Replace each `body` assertion with section-specific element verification using Playwright's built-in locators (`getByRole`, `getByText`, `getByLabel`), verify mock data content (not just existence), and use CSS variable checks for theme toggle verification.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| @playwright/test | (from project) | E2E testing framework | Project standard, already in use |

### Key Playwright APIs for This Phase
| API | Purpose | When to Use |
|-----|---------|-------------|
| `getByRole('heading', { name })` | Locate headings | Page layout, section titles |
| `getByRole('button', { name })` | Locate buttons | Theme toggle, settings navigation |
| `getByRole('tab', { name })` | Locate tabs | Verify all 5 tabs exist |
| `getByText('text')` | Locate by text | Mock data name verification |
| `locator().count()` | Count elements | List item count matching mock data |
| `locator('html').getAttribute('class')` | Check theme class | Theme toggle verification |
| `locator('html').evaluate()` | Check CSS variables | Theme toggle color verification |
| `waitForSelector()` | Wait for element | Replace conditional patterns |
| `expect(locator).toContainText()` | Text content assertion | Mock data name rendering |

## Architecture Patterns

### Settings Page Structure (verified from source)

```
/settings page
├── Header
│   ├── LogSeekLogo (link to /)
│   ├── h1 "系统设置"
│   └── ThemeToggle (button, aria-label="Toggle theme")
├── Tabs (5 tabs from bits-ui)
│   ├── 对象存储 (profiles)
│   ├── Agent (agents)
│   ├── 规划脚本 (planners)
│   ├── 大模型 (llm)
│   └── Server 日志 (server-log)
└── TabContent (shows one component at a time)
    ├── ProfileManagement (Card with "S3 Profile 配置" title)
    ├── AgentManagement (Card with "已注册 Agent" title)
    ├── PlannerManagement (Card with "规划脚本" title)
    ├── LlmManagement (Card with "大模型配置" title)
    └── ServerLogSettings (Card with "Server 日志设置" title)
```

### Pattern: Replace `body` Assertion with Section-Specific Elements

**Before (loose):**
```typescript
await expect(page.locator('body')).toBeVisible();
```

**After (tight) -- depends on test context:**

For Page Layout:
```typescript
// Verify heading + all 5 tabs
await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
await expect(page.getByRole('tab', { name: '对象存储' })).toBeVisible();
await expect(page.getByRole('tab', { name: 'Agent' })).toBeVisible();
await expect(page.getByRole('tab', { name: '规划脚本' })).toBeVisible();
await expect(page.getByRole('tab', { name: '大模型' })).toBeVisible();
await expect(page.getByRole('tab', { name: 'Server 日志' })).toBeVisible();
```

For LLM Management (after mock):
```typescript
// Click on the LLM tab first
await page.getByRole('tab', { name: '大模型' }).click();
// Verify mock data rendered
await expect(page.getByText('ollama-local')).toBeVisible();
await expect(page.getByText('qwen3:8b')).toBeVisible();
// Count backend items (1 item in mock)
const backendItems = page.locator('div').filter({ has: page.getByText('ollama-local') });
await expect(backendItems).toHaveCount(1);
```

For S3 Profile Management (after mock):
```typescript
// Default tab is profiles, verify mock data
await expect(page.getByText('minio-local')).toBeVisible();
await expect(page.getByText('http://127.0.0.1:9000')).toBeVisible();
```

### Pattern: Remove Conditional Assertions with waitForSelector

**Before (conditional -- hides failures):**
```typescript
const themeButton = page.getByRole('button', { name: /theme|主题|toggle/i });
const themeCount = await themeButton.count();
if (themeCount > 0) {
  await expect(themeButton.first()).toBeVisible();
}
```

**After (strict -- fails fast if broken):**
```typescript
const themeButton = page.getByRole('button', { name: /toggle/i });
await themeButton.waitFor({ state: 'visible', timeout: 5000 });
await expect(themeButton).toBeVisible();
```

Note: The ThemeToggle button has `aria-label="Toggle theme"` (verified from ThemeToggle.svelte line 11), so use `/toggle/i` regex.

### Pattern: Theme Toggle Bidirectional Verification

```typescript
const html = page.locator('html');
const themeButton = page.getByRole('button', { name: /toggle/i });

// Record initial state
const initialClass = (await html.getAttribute('class')) || '';
const initialBg = await html.evaluate(
  () => getComputedStyle(document.documentElement).getPropertyValue('--background').trim()
);

// Toggle to dark
await themeButton.click();
await page.waitForTimeout(300);
const darkClass = (await html.getAttribute('class')) || '';
const darkBg = await html.evaluate(
  () => getComputedStyle(document.documentElement).getPropertyValue('--background').trim()
);
expect(darkClass).toContain('dark');
expect(darkBg).not.toBe(initialBg);

// Toggle back to light
await themeButton.click();
await page.waitForTimeout(300);
const finalClass = (await html.getAttribute('class')) || '';
const finalBg = await html.evaluate(
  () => getComputedStyle(document.documentElement).getPropertyValue('--background').trim()
);
expect(finalClass).not.toContain('dark');
expect(finalBg).toBe(initialBg);
```

CSS variable values from `app.css`:
- Light mode: `--background: oklch(1 0 0)` (line 84)
- Dark mode: `--background: oklch(0.145 0 0)` (line 108)

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Wait for element existence | Custom polling loops | `locator.waitFor()` | Built-in timeout handling, better error messages |
| Check CSS computed values | Manual DOM manipulation | `locator.evaluate()` with `getComputedStyle()` | Reliable cross-browser access |
| Count matching elements | Manual array filtering | `locator.count()` + `expect().toHaveCount()` | Playwright auto-retries |

## Common Pitfalls

### Pitfall 1: Tab Content Not Visible Until Clicked
**What goes wrong:** Asserting elements in tab content without clicking the tab first
**Why it happens:** bits-ui Tabs only render active tab content; other tabs are hidden
**How to avoid:** Always click the target tab before asserting its content elements
**Warning signs:** `waitForSelector` timeout or element not found

### Pitfall 2: Mock Route Pattern Too Broad
**What goes wrong:** `**/profiles**` intercepts unintended requests
**Why it happens:** Glob pattern matches more than intended
**How to avoid:** Use more specific patterns like `**/logseek/profiles**` or verify exact URL in route handler
**Warning signs:** Tests pass but API mock data not appearing, or unexpected API failures

### Pitfall 3: Theme Toggle Timing
**What goes wrong:** CSS transition not complete when asserting class change
**Why it happens:** `mode-watcher` toggle has animation/transition delay
**How to avoid:** Use `page.waitForTimeout(300)` after click (already in existing test)
**Warning signs:** Intermittent test failures, class assertion succeeds sometimes

### Pitfall 4: `page.goto('/settings')` Already in beforeEach
**What goes wrong:** Tests that call `page.goto('/settings')` again cause redundant navigation
**Why it happens:** `beforeEach` already navigates and verifies heading
**How to avoid:** Remove redundant `page.goto('/settings')` calls; use `page.reload()` if re-fetch needed
**Warning signs:** Slower test execution, potential race conditions

## Code Examples

### Verified Patterns from Svelte Source

**ThemeToggle button selector** (from `ThemeToggle.svelte` line 11):
```typescript
// aria-label="Toggle theme"
page.getByRole('button', { name: /toggle/i })
```

**Tab trigger selectors** (from `+page.svelte` lines 52-56):
```typescript
page.getByRole('tab', { name: '对象存储' })
page.getByRole('tab', { name: 'Agent' })
page.getByRole('tab', { name: '规划脚本' })
page.getByRole('tab', { name: '大模型' })
page.getByRole('tab', { name: 'Server 日志' })
```

**Card title selectors** (from each management component):
```typescript
// ProfileManagement.svelte line 112
page.getByText('S3 Profile 配置')
// LlmManagement.svelte line 193
page.getByText('大模型配置')
// PlannerManagement.svelte line 210
page.getByText('规划脚本')
// ServerLogSettings.svelte line 80
page.getByText('Server 日志设置')
// AgentManagement.svelte line 193
page.getByText('已注册 Agent')
```

**List item counting for mock data:**
```typescript
// LLM mock: 1 backend with name 'ollama-local'
const llmItems = page.locator('[class*="rounded-lg border"]').filter({ hasText: 'ollama-local' });
await expect(llmItems).toHaveCount(1);

// S3 mock: 1 profile with name 'minio-local'
const s3Items = page.locator('[class*="rounded-lg border"]').filter({ hasText: 'minio-local' });
await expect(s3Items).toHaveCount(1);
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `locator('body').toBeVisible()` | Section-specific element assertions | This phase | Tests fail when feature breaks |
| `if (count > 0) { expect(...)` | `waitForSelector` + direct expect | This phase | Failures no longer silently skipped |
| Theme: check class only | Class + CSS variable + bidirectional toggle | This phase | Verifies actual theme change effect |

## Open Questions

1. **Should tab clicking before content assertion be in beforeEach or per-test?**
   - What we know: `beforeEach` already navigates to `/settings` and verifies heading
   - What's unclear: Whether clicking a specific tab should be repeated in each test
   - Recommendation: Click tab within each test that needs specific tab content. The default tab is 'profiles' (对象存储).

2. **Exact CSS variable to verify for theme toggle**
   - What we know: `--background` changes between `oklch(1 0 0)` and `oklch(0.145 0 0)`
   - What's unclear: Whether `--foreground` or other variables might be more reliable
   - Recommendation: Use `--background` as primary check (biggest contrast between light/dark)

## Validation Architecture

> Enabled (workflow.nyquist_validation not set to false in config.json)

### Test Framework
| Property | Value |
|----------|-------|
| Framework | @playwright/test |
| Config file | web/playwright.config.ts |
| Quick run command | `cd web && npx playwright test tests/e2e/settings.spec.ts` |
| Full suite command | `cd web && npx playwright test` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ASSERT-01 | Settings page assertions are tight and meaningful | E2E | `npx playwright test tests/e2e/settings.spec.ts` | YES |

### Sampling Rate
- Per task commit: `npx playwright test tests/e2e/settings.spec.ts`
- Phase gate: All 12 tests pass before `/gsd:verify-work`

### Wave 0 Gaps
- None -- existing test file `web/tests/e2e/settings.spec.ts` covers this phase

## Sources

### Primary (HIGH confidence)
- `web/src/routes/settings/+page.svelte` -- Settings page structure, 5 tabs, heading
- `web/src/lib/components/ThemeToggle.svelte` -- Button with `aria-label="Toggle theme"`, uses `mode-watcher`
- `web/src/routes/settings/LlmManagement.svelte` -- Card title "大模型配置", backend list rendering
- `web/src/routes/settings/ProfileManagement.svelte` -- Card title "S3 Profile 配置", profile list rendering
- `web/src/routes/settings/PlannerManagement.svelte` -- Card title "规划脚本", planner list rendering
- `web/src/routes/settings/ServerLogSettings.svelte` -- Card title "Server 日志设置", log config form
- `web/src/routes/settings/AgentManagement.svelte` -- Card title "已注册 Agent", agent list rendering
- `web/src/app.css` -- CSS variables: `--background` light=`oklch(1 0 0)`, dark=`oklch(0.145 0 0)`
- `web/tests/e2e/settings.spec.ts` -- Current test file with 12 tests, 10 `body` assertions to replace
- Playwright docs: https://playwright.dev/docs/locators
- Playwright docs: https://playwright.dev/docs/assertions

### Secondary (MEDIUM confidence)
- `mode-watcher` library for Svelte theme management (used by ThemeToggle)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- @playwright/test already in use, APIs verified in project
- Architecture: HIGH -- All Svelte components read directly, selectors verified
- Pitfalls: HIGH -- Common Playwright issues well-documented, tab behavior verified from bits-ui usage

**Research date:** 2026-03-14
**Valid until:** 2026-04-14 (30 days -- stable Playwright patterns)
