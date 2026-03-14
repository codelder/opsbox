# Phase 02: search-spec-ts-search-ux-spec-ts — Validation Strategy

**Created:** 2026-03-14
**Source:** Extracted from 02-RESEARCH.md

## Test Framework

| Property | Value |
|----------|-------|
| Framework | @playwright/test |
| Config file | web/playwright.config.ts |
| Quick run command | `cd web && npx playwright test tests/e2e/search.spec.ts tests/e2e/search_ux.spec.ts` |
| Full suite command | `cd web && npx playwright test` |

## Phase Requirements -> Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ASSERT-02 | search.spec.ts regex and conditional fixes | E2E | `npx playwright test tests/e2e/search.spec.ts` | YES |
| ASSERT-03 | search_ux.spec.ts conditional and verification fixes | E2E | `npx playwright test tests/e2e/search_ux.spec.ts` | YES |

## Sampling Rate

- Per task commit: `npx playwright test tests/e2e/search.spec.ts tests/e2e/search_ux.spec.ts`
- Phase gate: All tests pass before `/gsd:verify-work`

## Wave 0 Gaps

- None — existing test files cover this phase

## Validation Checklist

- [ ] Zero `/\d+\s*个结果/` regex in final expect assertions
- [ ] waitForFunction uses specific conditions (not just regex format)
- [ ] Empty state test verifies exact `0 个结果`
- [ ] All `if (count > 0)` conditionals removed from search.spec.ts
- [ ] All nested `if` conditionals removed from search_ux.spec.ts
- [ ] Highlight verification uses exact text matching
- [ ] File path verification uses content length threshold (>50)
- [ ] All tests still pass
