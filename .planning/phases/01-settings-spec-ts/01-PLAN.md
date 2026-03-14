---
phase: 01-settings-spec-ts
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - web/tests/e2e/settings.spec.ts
autonomous: true
requirements:
  - ASSERT-01
must_haves:
  truths:
    - "Zero `body` visibility checks remain in settings.spec.ts"
    - "All 5 tab triggers (对象存储, Agent, 规划脚本, 大模型, Server 日志) verified in Page Layout test"
    - "LLM mock data 'ollama-local' renders and is verified with count"
    - "S3 mock data 'minio-local' renders and is verified with count"
    - "Theme toggle verifies HTML class change (contains/does not contain 'dark')"
    - "Theme toggle verifies CSS variable --background value changes"
    - "Theme toggle verifies bidirectional toggle (back to original state)"
    - "All `if (count > 0)` conditional assertions removed"
    - "waitForSelector added before strict assertions where needed"
  artifacts:
    - path: "web/tests/e2e/settings.spec.ts"
      provides: "Tightened E2E assertions for settings page"
      min_lines: 200
  key_links:
    - from: "web/tests/e2e/settings.spec.ts"
      to: "web/src/routes/settings/+page.svelte"
      via: "tab trigger selectors (getByRole('tab', { name }))"
      pattern: "getByRole\\('tab', \\{ name: '"
    - from: "web/tests/e2e/settings.spec.ts"
      to: "web/src/lib/components/ThemeToggle.svelte"
      via: "button selector (getByRole('button', { name: /toggle/i }))"
      pattern: "getByRole\\('button', \\{ name: /toggle/i \\}\\)"
    - from: "web/tests/e2e/settings.spec.ts"
      to: "web/src/app.css"
      via: "CSS variable --background check"
      pattern: "getPropertyValue\\('--background'\\)"
---

<objective>
Tighten all 12 E2E tests in `settings.spec.ts` by replacing 10 weak `body` visibility assertions with section-specific element checks, adding mock data content verification, implementing bidirectional theme toggle verification with CSS variable checks, and removing all conditional assertion patterns.

Purpose: Tests must fail when the actual feature is broken, not pass vacuously with `body` checks and conditional skips.
Output: Single modified file `web/tests/e2e/settings.spec.ts` with tightened assertions across all 12 tests.
</objective>

<execution_context>
@./.claude/get-shit-done/workflows/execute-plan.md
@./.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/phases/01-settings-spec-ts/01-CONTEXT.md
@.planning/phases/01-settings-spec-ts/01-RESEARCH.md
@.planning/phases/01-settings-spec-ts/01-VALIDATION.md
@web/tests/e2e/settings.spec.ts

<interfaces>
Settings page structure (from +page.svelte):
- Header: h1 "系统设置", ThemeToggle button (aria-label="Toggle theme")
- 5 tabs: 对象存储, Agent, 规划脚本, 大模型, Server 日志
- Tab content renders only active tab (bits-ui behavior)

Tab content components and their Card titles:
- ProfileManagement: "S3 Profile 配置"
- AgentManagement: "已注册 Agent"
- PlannerManagement: "规划脚本"
- LlmManagement: "大模型配置"
- ServerLogSettings: "Server 日志设置"

CSS variables (from app.css):
- Light mode --background: oklch(1 0 0)
- Dark mode --background: oklch(0.145 0 0)

Mock data:
- LLM: [{ name: 'ollama-local', provider: 'ollama', base_url: 'http://127.0.0.1:11434', model: 'qwen3:8b', timeout_secs: 60 }]
- S3: [{ profile_name: 'minio-local', endpoint: 'http://127.0.0.1:9000', access_key: 'minioadmin' }]
</interfaces>
</context>

<tasks>

<task type="auto">
  <name>Task 1: Replace body assertions and tighten mock data verification across all sections</name>
  <files>web/tests/e2e/settings.spec.ts</files>
  <action>
Replace all 10 `body` visibility assertions with section-specific element checks. Also tighten mock data verification for LLM and S3 sections. Per section:

**Page Layout tests (2 tests):**
- Replace `body` assertion with: heading('系统设置') visible + all 5 tab triggers visible (getByRole('tab', { name: '...' }))
- Replace `body.textContent().length > 0` with: heading visible + at least one tab visible

**Planner Management tests (2 tests):**
- Replace `body` assertions with: heading('系统设置') visible + PlannerManagement card text visible (use getByText('规划脚本'))

**LLM Management test (1 test):**
- After mock + reload, click the '大模型' tab first
- Replace `body` assertion with: getByText('ollama-local') visible + getByText('qwen3:8b') visible
- Add count assertion: locator filtering on 'ollama-local' should have toHaveCount(1)

**S3 Profile Management test (1 test):**
- Default tab is 对象存储 (profiles), no tab click needed
- Replace `body` assertion with: getByText('minio-local') visible + getByText('http://127.0.0.1:9000') visible
- Add count assertion: locator filtering on 'minio-local' should have toHaveCount(1)

**Agent Management tests (2 tests):**
- Replace `body` assertions with: heading('系统设置') visible + AgentManagement card text visible (use getByText('已注册 Agent'))
- Remove `body.textContent().length > 0` pattern, replace with specific element check

**Server Log Settings tests (2 tests):**
- Replace `body` assertions with: heading('系统设置') visible + ServerLogSettings card text visible (use getByText('Server 日志设置'))

**Error Handling - API errors test (1 test):**
- Replace `body` assertion with: heading('系统设置') visible + tab structure still exists (verify at least one tab visible)

**Error Handling - loading state test (1 test):**
- Remove redundant page.goto('/settings') (already in beforeEach)
- Replace `body` assertion with: heading('系统设置') visible

Key selectors from research:
- Heading: page.getByRole('heading', { name: '系统设置' })
- Tabs: page.getByRole('tab', { name: '对象存储' }), 'Agent', '规划脚本', '大模型', 'Server 日志'
- Card titles via getByText: 'S3 Profile 配置', '已注册 Agent', '规划脚本', '大模型配置', 'Server 日志设置'

For list item counting, use:
```typescript
const items = page.locator('div').filter({ hasText: 'mock-name' });
await expect(items).toHaveCount(1);
```

IMPORTANT: For LLM and S3 tests, click the correct tab before asserting content (bits-ui only renders active tab content).
  </action>
  <verify>
    <automated>cd web && npx playwright test tests/e2e/settings.spec.ts --project=server 2>&1 | tail -20</automated>
  </verify>
  <done>
    - All 10 `body` locator assertions replaced with section-specific elements
    - LLM test verifies 'ollama-local' text and count after clicking '大模型' tab
    - S3 test verifies 'minio-local' text and count on default tab
    - Zero `body` visibility checks remain
    - All 12 tests still pass
  </done>
</task>

<task type="auto">
  <name>Task 2: Implement bidirectional theme toggle verification and remove conditional assertions</name>
  <files>web/tests/e2e/settings.spec.ts</files>
  <action>
Two changes in this task:

**A) Theme Toggle - bidirectional verification with CSS variables:**
Rewrite the "should toggle between light and dark theme" test:

1. Use exact selector: page.getByRole('button', { name: /toggle/i }) (ThemeToggle has aria-label="Toggle theme")
2. Wait for button: await themeButton.waitFor({ state: 'visible', timeout: 5000 })
3. Record initial state:
   - const initialClass = (await page.locator('html').getAttribute('class')) || ''
   - const initialBg = await page.locator('html').evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--background').trim())
4. Toggle to dark:
   - await themeButton.click()
   - await page.waitForTimeout(300)
   - Verify: html class contains 'dark'
   - Verify: --background changed from initial
5. Toggle back to light:
   - await themeButton.click()
   - await page.waitForTimeout(300)
   - Verify: html class does NOT contain 'dark'
   - Verify: --background equals initial value

Remove the `if (themeCount > 0)` conditional wrapping.

**B) Settings Navigation - remove conditional assertions:**
For both "should have settings button in header" and "should navigate to settings page" tests:

1. Remove `if (count > 0)` conditional wrapping
2. Add waitForSelector before assertion: await settingsButton.waitFor({ state: 'visible', timeout: 5000 })
3. Assert directly: await expect(settingsButton).toBeVisible()

Use exact selector: page.getByRole('button', { name: /打开设置|settings/i })

Note: The settings button is on the home page ('/'), not on the settings page itself. The beforeEach navigates to '/settings', so these tests navigate to '/' explicitly.
  </action>
  <verify>
    <automated>cd web && npx playwright test tests/e2e/settings.spec.ts --project=server 2>&1 | tail -20</automated>
  </verify>
  <done>
    - Theme toggle test verifies bidirectional toggle (light -> dark -> light)
    - Theme toggle test verifies CSS variable --background value changes
    - Theme toggle test verifies html class 'dark' presence/absence
    - No `if (count > 0)` conditional assertions remain
    - Settings navigation tests use waitForSelector + direct assertion
    - All 12 tests pass
  </done>
</task>

</tasks>

<verification>
Run the full test suite to confirm all 12 tests pass:
```bash
cd web && npx playwright test tests/e2e/settings.spec.ts
```

Validation checklist from VALIDATION.md:
- [ ] Zero `body` visibility checks remain
- [ ] All 5 tab triggers verified in Page Layout test
- [ ] LLM mock data ('ollama-local') renders and is verified
- [ ] S3 mock data ('minio-local') renders and is verified
- [ ] Theme toggle: HTML class changes verified
- [ ] Theme toggle: CSS variable `--background` value changes verified
- [ ] Theme toggle: Bidirectional toggle verified
- [ ] All `if (count > 0)` conditional assertions removed
- [ ] `waitForSelector` added before strict assertions
- [ ] All 12 tests pass with tightened assertions
</verification>

<success_criteria>
- Grep for `page.locator('body')` returns zero matches in settings.spec.ts
- Grep for `if (count > 0)` returns zero matches in settings.spec.ts
- All 12 tests pass: 2 Page Layout + 2 Planner + 1 LLM + 1 S3 + 2 Agent + 2 Server Log + 2 Error Handling + 1 Theme Toggle (all in describe blocks)
- Theme toggle test toggles twice and verifies CSS variable --background changes
- Mock data tests verify name text AND item count
</success_criteria>

<output>
After completion, create `.planning/phases/01-settings-spec-ts/01-01-SUMMARY.md`
</output>
