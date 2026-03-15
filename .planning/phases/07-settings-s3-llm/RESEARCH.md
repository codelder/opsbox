# Phase 7: Settings -- S3 Profiles & LLM Backends - Research

**Researched:** 2026-03-15
**Domain:** E2E testing for Settings page S3 Profile CRUD and LLM Backend CRUD
**Confidence:** HIGH

## Summary

This phase covers E2E testing for full CRUD operations on S3 Profiles and LLM Backends through the Settings page. The existing `settings.spec.ts` has weak display-only tests with mock data; Phase 7 requires replacing them with tests that exercise create, edit, delete, and set-default interactions.

Both `ProfileManagement.svelte` and `LlmManagement.svelte` are fully implemented with forms, list views, and action buttons. Tests will need to mock API responses since S3 and LLM require external services not available in E2E environments.

**Primary recommendation:** Use `page.route()` to intercept API calls and return controlled mock responses. Test user-visible behavior: form filling, save success alerts, list refresh after operations, and confirmation dialog interactions.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Playwright | 1.52+ | E2E test framework | Already used in project, configured in `playwright.config.ts` |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `page.route()` | built-in | API response mocking | S3/LLM tests (external services unavailable) |
| `page.getByRole()` | built-in | Semantic element selection | Locating buttons, inputs, tabs |
| `page.getByText()` | built-in | Text-based selection | Verifying displayed values |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `page.route()` mocking | Real API calls | Real API not possible (requires S3/LLM services); mocks already accepted pattern in `settings.spec.ts` |

**Installation:** No additional packages needed. Playwright is already installed.

## Architecture Patterns

### API Endpoints to Mock

**S3 Profile endpoints** (base: `/api/v1/logseek`):
| Operation | Method | URL Pattern | Response |
|-----------|--------|-------------|----------|
| List | GET | `**/profiles` | `{ profiles: S3ProfilePayload[] }` |
| Create/Update | POST | `**/profiles` | 204 No Content |
| Delete | DELETE | `**/profiles/{name}` | 204 No Content |

**LLM Backend endpoints** (base: `/api/v1/logseek`):
| Operation | Method | URL Pattern | Response |
|-----------|--------|-------------|----------|
| List | GET | `**/settings/llm/backends` | `{ backends: LlmBackendListItem[], default: string \| null }` |
| Create/Update | POST | `**/settings/llm/backends` | 204 No Content |
| Delete | DELETE | `**/settings/llm/backends/{name}` | 204 No Content |
| Set Default | POST | `**/settings/llm/default` | `{ name: string }` |

### Data Types

```typescript
// S3 Profile payload
interface S3ProfilePayload {
  profile_name: string;
  endpoint: string;
  access_key: string;
  secret_key: string;
}

// LLM Backend list item
interface LlmBackendListItem {
  name: string;
  provider: 'ollama' | 'openai';
  base_url: string;
  model: string;
  timeout_secs: number;
  has_api_key: boolean;
}

// LLM Backend upsert payload
interface LlmBackendUpsertPayload {
  name: string;
  provider: 'ollama' | 'openai';
  base_url: string;
  model: string;
  timeout_secs?: number;
  api_key?: string;       // openai only
  organization?: string;  // openai only
  project?: string;       // openai only
}
```

### UI Components and Locators

**ProfileManagement.svelte:**
- Tab: `page.getByRole('tab', { name: '对象存储' })`
- "New Profile" button: `page.getByRole('button', { name: '新建 Profile' })`
- Form fields: `#profile-name`, `#profile-endpoint`, `#profile-access-key`, `#profile-secret-key`
- Save button: `page.getByRole('button', { name: '保存 Profile' })`
- Cancel button: `page.getByRole('button', { name: '取消' })`
- Edit icon button: `Button` with `Edit2` icon (ghost variant, size="icon")
- Delete icon button: `Button` with `Trash2` icon (destructive variant, size="icon")
- Profile name display: `span.font-semibold` containing profile name
- Success alert: `Alert type="success"` with message "Profile 已保存"

**LlmManagement.svelte:**
- Tab: `page.getByRole('tab', { name: '大模型' })`
- "New Backend" button: `page.getByRole('button', { name: '新建后端' })`
- Form fields: `#llm-name`, `#llm-provider` (select), `#llm-base-url`, `#llm-model`, `#llm-timeout`
- OpenAI-only fields (conditional): `#llm-api-key`, `#llm-org`, `#llm-project`
- Save button: `page.getByRole('button', { name: /保存/ })`
- Cancel button: `page.getByRole('button', { name: '取消' })`
- "Set Default" button: `page.getByRole('button', { name: '设为默认' })`
- "Already Default" indicator: `page.getByText('已默认')`
- Backend name display: `span.font-semibold` containing backend name
- Provider badge: `Badge` with provider text
- Success alert: `Alert type="success"` with message "已保存大模型配置"
- Confirmation dialog: native `confirm()` dialog

### Pattern: Mock-Based CRUD Testing

The existing `settings.spec.ts` already demonstrates the mock pattern:
```typescript
await page.route('**/api/v1/logseek/profiles', async (route) => {
  await route.fulfill({
    status: 200,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ profiles: [...] })
  });
});
```

The key difference in Phase 7 tests: we need to simulate **state changes** across multiple API calls. A profile created via POST should appear in the next GET. This requires mutable mock state.

### Anti-Patterns to Avoid
- **`body` visibility assertions:** Always passes, provides no signal. Use specific content assertions.
- **`waitForTimeout` for synchronization:** Use `waitForResponse` or `waitForSelector` instead.
- **Conditional test bodies:** Tests that skip assertions when data is empty create false passes.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| API response mocking | Custom fetch interceptors | `page.route()` | Playwright's built-in route interception handles this |
| Confirmation dialog | Custom modal detection | `page.on('dialog')` handler | Native `confirm()` requires dialog event listener |
| Form state tracking | Manual DOM inspection | `inputValue()` | Built-in method for reading input values |

## Common Pitfalls

### Pitfall 1: Mock State Not Mutating
**What goes wrong:** Tests create a profile via POST but the subsequent GET still returns empty list.
**Why it happens:** Mock handler returns static data regardless of prior actions.
**How to avoid:** Use module-scoped mutable array that both POST and GET handlers read/write.
**Warning signs:** Test passes the "save" step but fails on "profile appears in list."

### Pitfall 2: Name Field Disabled on Edit
**What goes wrong:** Test tries to edit the profile name field when editing existing profile.
**Why it happens:** `ProfileManagement.svelte` sets `disabled={!!editingProfile}` on the name input to prevent renaming.
**How to avoid:** Only assert that the name field shows the existing value and is disabled. Test other fields (endpoint, keys) for editability.
**Warning signs:** Test fails with "element is disabled" error.

### Pitfall 3: Delete Confirmation Dialog Not Handled
**What goes wrong:** Test clicks delete but nothing happens because `confirm()` dialog blocks.
**Why it happens:** Both components use native `confirm()` which needs a dialog handler.
**How to avoid:** Register `page.on('dialog', ...)` before clicking delete.
**Warning signs:** Delete button click times out or test hangs.

### Pitfall 4: Save Button Disabled State
**What goes wrong:** Test cannot click save because all required fields are not filled.
**Why it happens:** Save buttons are disabled until all required fields have non-empty trimmed values.
**How to avoid:** Fill ALL required fields before asserting save button is enabled.
**Warning signs:** Test fails with "element is disabled" on save button click.

### Pitfall 5: List Refresh Timing
**What goes wrong:** Test asserts new profile appears immediately after save.
**Why it happens:** After save, composables call `loadProfiles()`/`load()` which makes a new GET request. The assertion may run before this completes.
**How to avoid:** Use `waitForResponse` for the list GET call after save, or wait for the success alert to appear first.
**Warning signs:** Intermittent failures where profile sometimes appears, sometimes does not.

## Code Examples

### SETTINGS-01: Create S3 Profile
```typescript
// Mutable mock state
let mockProfiles: S3ProfilePayload[] = [];

// Mock GET handler
await page.route('**/api/v1/logseek/profiles', async (route) => {
  if (route.request().method() === 'GET') {
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ profiles: mockProfiles })
    });
  } else {
    // POST handler - extract and store
    const body = JSON.parse(route.request().postData()!);
    mockProfiles.push(body);
    await route.fulfill({ status: 204 });
  }
});

// Navigate and interact
await page.goto('/settings');
await page.getByRole('tab', { name: '对象存储' }).click();
await page.getByRole('button', { name: '新建 Profile' }).click();

// Fill form
await page.locator('#profile-name').fill('test-minio');
await page.locator('#profile-endpoint').fill('http://127.0.0.1:9000');
await page.locator('#profile-access-key').fill('minioadmin');
await page.locator('#profile-secret-key').fill('miniosecret');

// Save and verify
const listPromise = page.waitForResponse('**/api/v1/logseek/profiles');
await page.getByRole('button', { name: '保存 Profile' }).click();
await listPromise;

// Assert
await expect(page.getByText('Profile 已保存')).toBeVisible();
await expect(page.getByText('test-minio')).toBeVisible();
await expect(page.getByText('http://127.0.0.1:9000')).toBeVisible();
```

### SETTINGS-03: Delete S3 Profile (with confirmation)
```typescript
// Set up dialog handler BEFORE clicking delete
page.on('dialog', async (dialog) => {
  expect(dialog.type()).toBe('confirm');
  expect(dialog.message()).toContain('test-minio');
  await dialog.accept();
});

// Mock DELETE handler
await page.route('**/api/v1/logseek/profiles/test-minio', async (route) => {
  if (route.request().method() === 'DELETE') {
    mockProfiles = mockProfiles.filter(p => p.profile_name !== 'test-minio');
    await route.fulfill({ status: 204 });
  } else {
    await route.continue();
  }
});

// Click delete on the test-minio profile card
const profileCard = page.locator('.grid.gap-4 > div').filter({ hasText: 'test-minio' });
await profileCard.getByRole('button', { name: '删除' }).click();

// Verify removed from list
await expect(page.getByText('test-minio')).not.toBeVisible();
```

### SETTINGS-05: Set LLM Backend as Default
```typescript
// Mock set-default endpoint
await page.route('**/api/v1/logseek/settings/llm/default', async (route) => {
  if (route.request().method() === 'POST') {
    const body = JSON.parse(route.request().postData()!);
    mockLlmDefault = body.name;
    await route.fulfill({ status: 200, body: JSON.stringify({ name: body.name }) });
  } else {
    await route.fulfill({ status: 200, body: JSON.stringify(mockLlmDefault) });
  }
});

// Click "设为默认" on a backend
const backendCard = page.locator('.grid.gap-4 > div').filter({ hasText: 'ollama-local' });
await backendCard.getByRole('button', { name: '设为默认' }).click();

// Verify default badge appears
await expect(page.getByText('已默认')).toBeVisible();
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Display-only mock tests | Full CRUD interaction tests | Phase 7 (now) | Real behavior coverage |
| `body` visibility assertions | Specific content assertions | v1.0 milestone | Eliminates false positives |

**Deprecated/outdated:**
- The existing `settings.spec.ts` LLM and S3 test sections are display-only with static mocks. Phase 7 replaces them.

## Open Questions

1. **Should tests use serial mode for CRUD flows?**
   - What we know: Create-then-read flows depend on mock state mutation.
   - What's unclear: Whether parallel execution of CRUD tests within the same file would corrupt shared mock state.
   - Recommendation: Use `test.describe.configure({ mode: 'serial' })` for each CRUD test group, or use per-test mock state isolation via `test.beforeEach`.

2. **Should we test form validation (e.g., empty required fields)?**
   - What we know: Save buttons are disabled when required fields are empty.
   - What's unclear: Whether this is a core Phase 7 requirement or edge case.
   - Recommendation: Include at least one test that verifies save is disabled with empty fields (SETTINGS-01 variant).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Playwright 1.52+ |
| Config file | `/Users/wangyue/workspace/codelder/opsboard/web/playwright.config.ts` |
| Quick run command | `pnpm --dir web test:e2e -- settings` |
| Full suite command | `pnpm --dir web test:e2e` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SETTINGS-01 | Create S3 Profile | E2E (mock) | `pnpm --dir web test:e2e -- -g "SETTINGS-01"` | ❌ Wave 0 |
| SETTINGS-02 | Edit S3 Profile | E2E (mock) | `pnpm --dir web test:e2e -- -g "SETTINGS-02"` | ❌ Wave 0 |
| SETTINGS-03 | Delete S3 Profile | E2E (mock) | `pnpm --dir web test:e2e -- -g "SETTINGS-03"` | ❌ Wave 0 |
| SETTINGS-04 | Create LLM Backend | E2E (mock) | `pnpm --dir web test:e2e -- -g "SETTINGS-04"` | ❌ Wave 0 |
| SETTINGS-05 | Set LLM Default | E2E (mock) | `pnpm --dir web test:e2e -- -g "SETTINGS-05"` | ❌ Wave 0 |
| SETTINGS-06 | Delete LLM Backend | E2E (mock) | `pnpm --dir web test:e2e -- -g "SETTINGS-06"` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `pnpm --dir web test:e2e -- settings`
- **Per wave merge:** `pnpm --dir web test:e2e`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `web/tests/e2e/settings_crud.spec.ts` — new test file for all 6 requirements
- [ ] Replace existing mock tests in `web/tests/e2e/settings.spec.ts` (lines 57-119) with expanded CRUD coverage or integrate into new file

## Sources

### Primary (HIGH confidence)
- `web/src/routes/settings/ProfileManagement.svelte` — S3 profile form, list, edit/delete UI
- `web/src/routes/settings/LlmManagement.svelte` — LLM backend form, list, edit/delete/default UI
- `web/src/lib/modules/logseek/api/profiles.ts` — S3 profile API client (GET, POST, DELETE)
- `web/src/lib/modules/logseek/api/llm.ts` — LLM backend API client (GET, POST, DELETE, set-default)
- `web/src/lib/modules/logseek/types/index.ts` — S3ProfilePayload, LlmBackendListItem, LlmBackendUpsertPayload types
- `web/tests/e2e/settings.spec.ts` — Existing test patterns and mock approach

### Secondary (MEDIUM confidence)
- `.planning/research/PITFALLS.md` — E2E testing anti-patterns and prevention
- `.planning/research/STACK.md` — Playwright assertion best practices

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — Playwright already configured, mock patterns proven in codebase
- Architecture: HIGH — Components are fully implemented, all form fields and buttons visible in source
- Pitfalls: HIGH — Confirmation dialogs, disabled fields, and mock state mutation are well understood

**Research date:** 2026-03-15
**Valid until:** 2026-04-15 (stable feature area, no expected changes)
