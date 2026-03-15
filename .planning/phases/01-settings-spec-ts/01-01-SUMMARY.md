---
phase: 01-settings-spec-ts
plan: 01
subsystem: testing
tags: [playwright, e2e, settings, svelte, assertions]

# Dependency graph
requires: []
provides:
  - Tightened E2E assertions for settings page (15 tests)
  - Body visibility checks replaced with section-specific element assertions
  - Bidirectional theme toggle verification with CSS variable checks
  - Mock data content verification with item counts
  - All conditional assertions removed
affects:
  - 02-search-spec-ts
  - 03-integration-explorer-spec-ts

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Route function matcher: (url) => url.pathname.endsWith('/profiles')"
    - "Mock response format must match backend API contracts (e.g., { profiles: [...] } not flat array)"
    - "span.font-semibold for item count verification in card lists"

key-files:
  created: []
  modified:
    - web/tests/e2e/settings.spec.ts

key-decisions:
  - "Used span.font-semibold selector for item count instead of div.filter({ hasText }) to avoid matching nested containers"
  - "Used exact URL pattern for S3 profiles route instead of glob to avoid intercepting Vite source file loads"
  - "Removed waitForLoadState('networkidle') calls that were timing out due to persistent connections"
  - "Fixed mock response formats to match actual backend API contracts (LLM: { backends: [...], default }, S3: { profiles: [...] })"

patterns-established:
  - "Mock data verification: assert text visible + assert item count via span.font-semibold"
  - "Tab navigation: click tab first, then assert section content"
  - "Theme toggle: verify bidirectional (light->dark->light) with HTML class and CSS variable --background"
  - "waitForSelector before strict assertions for dynamic elements"

requirements-completed: [ASSERT-01]

# Metrics
duration: 35min
completed: 2026-03-14
---

# Phase 01: settings-spec-ts - Plan 01 Summary

**Tightened 15 E2E tests in settings.spec.ts by replacing all body visibility checks with section-specific assertions, fixing mock data formats to match backend API contracts, and implementing bidirectional theme toggle verification**

## Performance

- **Duration:** 35 min
- **Started:** 2026-03-14T18:30:00Z
- **Completed:** 2026-03-14T19:20:00Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments
- Replaced 10 `page.locator('body').toBeVisible()` assertions with section-specific element checks (headings, tabs, card titles)
- Fixed LLM mock response format from flat array to `{ backends: [...], default: null }` to match actual API
- Fixed S3 mock response format from flat array to `{ profiles: [...] }` to match actual API
- Implemented bidirectional theme toggle test verifying HTML class 'dark' and CSS variable --background changes
- Removed all `if (count > 0)` conditional assertions from Settings Navigation and Theme Toggle tests
- Added `waitForSelector` before strict assertions for dynamic elements
- Added item count verification using `span.font-semibold` locator for LLM and S3 mock data

## Task Commits

Each task was committed atomically:

1. **Task 1: Replace body assertions and tighten mock data verification** - `f92c0f8` (test)
2. **Task 2: Implement bidirectional theme toggle and remove conditional assertions** - `157b0e4` (test)

## Files Created/Modified
- `web/tests/e2e/settings.spec.ts` - Tightened all 15 E2E tests with section-specific assertions, fixed mock formats, bidirectional theme verification

## Decisions Made
- Used `span.font-semibold` selector for item count verification instead of `div.filter({ hasText })` because div filter matches 14+ nested containers, not just the item
- Used exact URL pattern `**/api/v1/logseek/profiles` for S3 mock route instead of glob to avoid intercepting Vite source file loads (which was causing module loading failures)
- Removed `waitForLoadState('networkidle')` calls that were timing out due to persistent backend connections (planners API polling)
- Fixed mock response formats to match actual backend: LLM returns `{ backends: [...], default }`, S3 returns `{ profiles: [...] }`

## Deviations from Plan

### Auto-fixed Issues

**1. [Mock Format - LLM API] Mock returning flat array instead of { backends: [...] }**
- **Found during:** Task 1 (LLM Management test)
- **Issue:** Mock returned `[{ name: 'ollama-local', ... }]` but `listLlmBackends()` expects `{ backends: [...], default: null }` and accesses `data.backends`
- **Fix:** Changed mock response to `{ backends: [...], default: null }`
- **Files modified:** web/tests/e2e/settings.spec.ts
- **Verification:** LLM test passes, ollama-local text visible with count 1
- **Committed in:** f92c0f8 (Task 1 commit)

**2. [Mock Format - S3 API] Mock returning flat array instead of { profiles: [...] }**
- **Found during:** Task 1 (S3 Profile Management test)
- **Issue:** Mock returned `[{ profile_name: 'minio-local', ... }]` but `listProfiles()` expects `S3ProfileListResponse` with `{ profiles: [...] }` and returns `data.profiles || []`
- **Fix:** Changed mock response to `{ profiles: [{ profile_name: 'minio-local', ... }] }`
- **Files modified:** web/tests/e2e/settings.spec.ts
- **Verification:** S3 test passes, minio-local text visible with count 1
- **Committed in:** f92c0f8 (Task 1 commit)

**3. [Route Pattern - S3] Glob pattern intercepting Vite source files**
- **Found during:** Task 1 debugging (S3 Profile test)
- **Issue:** Pattern `**/profiles**` matched both API endpoint AND source file `profiles.ts`, corrupting module loading
- **Fix:** Used exact URL pattern `**/api/v1/logseek/profiles` to only match the API endpoint
- **Files modified:** web/tests/e2e/settings.spec.ts
- **Verification:** S3 mock intercepts only the API call, not source files
- **Committed in:** f92c0f8 (Task 1 commit)

**4. [Timing - waitForLoadState] networkidle timeout due to persistent connections**
- **Found during:** Task 1 (Planner Management test)
- **Issue:** `waitForLoadState('networkidle')` was timing out because the planners API polls continuously
- **Fix:** Removed redundant `waitForLoadState` calls since beforeEach already verifies page loaded via heading assertion
- **Files modified:** web/tests/e2e/settings.spec.ts
- **Verification:** All tests complete within timeout
- **Committed in:** f92c0f8 (Task 1 commit)

**5. [Count Locator - div.filter] Too many matches for item count**
- **Found during:** Task 1 (LLM Management test)
- **Issue:** `locator('div').filter({ hasText: 'ollama-local' })` matched 14 elements (all parent containers)
- **Fix:** Used `span.font-semibold` with hasText filter which matches exactly the item name span
- **Files modified:** web/tests/e2e/settings.spec.ts
- **Verification:** toHaveCount(1) passes for both LLM and S3 items
- **Committed in:** f92c0f8 (Task 1 commit)

---

**Total deviations:** 5 auto-fixed (2 mock format, 1 route pattern, 1 timing, 1 count locator)
**Impact on plan:** All auto-fixes were necessary for tests to actually verify mock data rendering. Without these fixes, the mock assertions would fail or match incorrectly.

## Issues Encountered
- Significant debugging required to discover that backend API response formats differ from what was assumed in the plan. The actual backend returns wrapped objects (`{ profiles: [...] }`, `{ backends: [...] }`) rather than flat arrays. This required inspecting actual API responses via browser fetch interception.

## Next Phase Readiness
- Plan 01 complete - all 15 tests passing with tightened assertions
- Ready for Phase 2 (search.spec.ts and search_ux.spec.ts tightening)
- Pattern established: check actual API response format before writing mocks

---
*Phase: 01-settings-spec-ts*
*Completed: 2026-03-14*
