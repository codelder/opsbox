# Phase 01: settings-spec-ts — Validation Strategy

**Created:** 2026-03-14
**Source:** Extracted from 01-RESEARCH.md

## Test Framework

| Property | Value |
|----------|-------|
| Framework | @playwright/test |
| Config file | web/playwright.config.ts |
| Quick run command | `cd web && npx playwright test tests/e2e/settings.spec.ts` |
| Full suite command | `cd web && npx playwright test` |

## Phase Requirements -> Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ASSERT-01 | Settings page assertions are tight and meaningful | E2E | `npx playwright test tests/e2e/settings.spec.ts` | YES |

## Sampling Rate

- Per task commit: `npx playwright test tests/e2e/settings.spec.ts`
- Phase gate: All 12 tests pass before `/gsd:verify-work`

## Wave 0 Gaps

- None — existing test file `web/tests/e2e/settings.spec.ts` covers this phase

## Validation Checklist

- [ ] Zero `body` visibility checks remain
- [ ] All 5 tab triggers verified in Page Layout test
- [ ] LLM mock data ('ollama-local') renders and is verified
- [ ] S3 mock data ('minio-local') renders and is verified
- [ ] Theme toggle: HTML class changes verified (contains 'dark' / not contains 'dark')
- [ ] Theme toggle: CSS variable `--background` value changes verified
- [ ] Theme toggle: Bidirectional toggle verified (back to original)
- [ ] All `if (count > 0)` conditional assertions removed
- [ ] `waitForSelector` added before strict assertions
- [ ] All 12 tests pass with tightened assertions
