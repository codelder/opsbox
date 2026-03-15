---
phase: 07-settings-s3-llm
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - web/tests/e2e/settings.spec.ts
autonomous: true
requirements:
  - SETTINGS-01
  - SETTINGS-02
  - SETTINGS-03
  - SETTINGS-04
  - SETTINGS-05
  - SETTINGS-06
user_setup: []

must_haves:
  truths:
    - "User can create a new S3 Profile by filling name, endpoint, access key, secret key and saving, then it appears in the list"
    - "User can edit an existing S3 Profile (endpoint, access key, secret key) and see changes reflected in the list"
    - "User can delete an S3 Profile with confirmation dialog and it is removed from the list"
    - "User can create a new LLM Backend by filling name, provider, base URL, model and saving, then it appears in the list"
    - "User can set an LLM Backend as default and see it visually marked as default while others show 'set default'"
    - "User can delete an LLM Backend with confirmation dialog and it is removed from the list"
  artifacts:
    - path: "web/tests/e2e/settings.spec.ts"
      provides: "E2E tests for S3 Profile and LLM Backend CRUD operations"
      min_lines: 250
      contains:
        - "test.describe('S3 Profile CRUD'"
        - "test.describe('LLM Backend CRUD'"
  key_links:
    - from: "web/tests/e2e/settings.spec.ts"
      to: "ProfileManagement.svelte"
      via: "form field locators (#profile-name, #profile-endpoint, #profile-access-key, #profile-secret-key) and save button"
      pattern: "profile-name|profile-endpoint|profile-access-key|profile-secret-key"
    - from: "web/tests/e2e/settings.spec.ts"
      to: "LlmManagement.svelte"
      via: "form field locators (#llm-name, #llm-provider, #llm-base-url, #llm-model) and set-default button"
      pattern: "llm-name|llm-provider|llm-base-url|llm-model|设为默认"
    - from: "web/tests/e2e/settings.spec.ts"
      to: "page.route() mock handlers"
      via: "mutable state arrays shared across GET/POST/DELETE handlers"
      pattern: "mockProfiles|mockBackends|mockLlmDefault"
    - from: "web/tests/e2e/settings.spec.ts"
      to: "page.on('dialog')"
      via: "dialog handler for native confirm() on delete"
      pattern: "page.on\\('dialog'"
---

<objective>
Add S3 Profile CRUD and LLM Backend CRUD E2E tests to settings.spec.ts

Purpose: The existing settings.spec.ts has display-only mock tests that verify list rendering. Phase 7 replaces the weak S3 and LLM test sections with full CRUD interaction tests covering create, edit, delete, and set-default operations. All 6 requirements (SETTINGS-01 through SETTINGS-06) are covered in a single plan because they share the same test file and follow the same mock-based pattern.

Output: Extended settings.spec.ts with two new test.describe blocks replacing the existing "LLM Management (Mock)" and "S3 Profile Management (Mock)" sections.
</objective>

<execution_context>
@./.claude/get-shit-done/workflows/execute-plan.md
@./.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/phases/07-settings-s3-llm/RESEARCH.md
@.planning/ROADMAP.md
@.planning/REQUIREMENTS.md

# Existing test file to extend (replace S3 and LLM mock sections)
@web/tests/e2e/settings.spec.ts

# Component sources for locator reference
@web/src/routes/settings/ProfileManagement.svelte
@web/src/routes/settings/LlmManagement.svelte
</context>

<tasks>

<task type="auto">
  <name>P7-01: Replace S3 Profile mock section with CRUD tests</name>
  <files>web/tests/e2e/settings.spec.ts</files>
  <action>
    Replace the existing `test.describe('S3 Profile Management (Mock)', ...)` block (lines 91-120) with a new `test.describe('S3 Profile CRUD', ...)` block containing:

    1. **Shared mutable mock state** (module-scoped `let mockProfiles`):
       ```typescript
       let mockProfiles: Array<{
         profile_name: string;
         endpoint: string;
         access_key: string;
         secret_key: string;
       }> = [];
       ```

    2. **Mock setup** in `test.beforeEach`:
       - Reset `mockProfiles = []` before each test
       - Register `page.route('**/api/v1/logseek/profiles', ...)` handler:
         - GET: respond with `{ profiles: mockProfiles }`
         - POST: parse body, push to `mockProfiles`, respond 204
       - Register `page.route('**/api/v1/logseek/profiles/*', ...)` for individual profile operations:
         - DELETE: filter out matching `profile_name` from `mockProfiles`, respond 204

    3. **Test: SETTINGS-01 - Create S3 Profile**:
       - Navigate to /settings (already done by outer beforeEach)
       - Click "对象存储" tab
       - Click "新建 Profile" button
       - Fill #profile-name with "test-minio"
       - Fill #profile-endpoint with "http://127.0.0.1:9000"
       - Fill #profile-access-key with "minioadmin"
       - Fill #profile-secret-key with "miniosecret"
       - Verify save button is enabled
       - Set up `waitForResponse` for the profiles GET (list refresh)
       - Click "保存 Profile" button
       - Assert success alert "Profile 已保存" visible
       - Assert "test-minio" and "http://127.0.0.1:9000" visible in list

    4. **Test: SETTINGS-01 variant - Save disabled with empty fields**:
       - Click "对象存储" tab
       - Click "新建 Profile" button
       - Verify save button is disabled (all fields empty)

    5. **Test: SETTINGS-02 - Edit S3 Profile**:
       - Seed `mockProfiles` with one profile: `{ profile_name: 'test-minio', endpoint: 'http://127.0.0.1:9000', access_key: 'minioadmin', secret_key: '' }`
       - Navigate to /settings, click "对象存储" tab
       - Click edit button (ghost icon button with Edit2 icon) on the test-minio card
       - Assert form title shows "编辑 Profile: test-minio"
       - Assert #profile-name is disabled and shows "test-minio"
       - Clear #profile-endpoint and fill with "http://192.168.1.100:9000"
       - Fill #profile-access-key with "newkey"
       - Fill #profile-secret-key with "newsecret"
       - Set up `waitForResponse` for the profiles GET (list refresh)
       - Click "保存 Profile" button
       - Assert success alert visible
       - Assert updated endpoint "http://192.168.1.100:9000" visible in list

    6. **Test: SETTINGS-03 - Delete S3 Profile**:
       - Seed `mockProfiles` with one profile
       - Navigate to /settings, click "对象存储" tab
       - Register dialog handler BEFORE clicking delete:
         ```typescript
         page.on('dialog', async (dialog) => {
           expect(dialog.type()).toBe('confirm');
           expect(dialog.message()).toContain('test-minio');
           await dialog.accept();
         });
         ```
       - Click delete button (ghost icon button with Trash2 icon, destructive variant) on the test-minio card
       - Set up `waitForResponse` for the profiles GET (list refresh after delete)
       - Assert "test-minio" no longer visible in the list

    7. **Test: SETTINGS-03 variant - Multiple profiles in list**:
       - Seed `mockProfiles` with two profiles: 'profile-a' and 'profile-b'
       - Navigate, click "对象存储" tab
       - Assert both profile names visible
       - Delete 'profile-a' (with dialog handler)
       - Assert only 'profile-b' remains visible

    Locators to use (from ProfileManagement.svelte source):
    - Tab: `page.getByRole('tab', { name: '对象存储' })`
    - New button: `page.getByRole('button', { name: '新建 Profile' })`
    - Save button: `page.getByRole('button', { name: '保存 Profile' })`
    - Name input: `#profile-name` (disabled in edit mode)
    - Endpoint input: `#profile-endpoint`
    - Access key input: `#profile-access-key`
    - Secret key input: `#profile-secret-key`
    - Profile card: `page.locator('div').filter({ hasText: 'profile-name' })` with edit/delete buttons
    - Edit button: `button` with `Edit2` icon (ghost variant)
    - Delete button: `button` with `Trash2` icon (ghost, destructive)

    Pitfalls to handle (from RESEARCH.md):
    - Mock state mutation: POST handler pushes to `mockProfiles`, next GET returns updated array
    - Name field disabled on edit: assert `disabled` attribute, do not try to type
    - Dialog handler: register BEFORE clicking delete button
    - Save button disabled: fill ALL required fields before asserting enabled
    - List refresh timing: use `waitForResponse` after save/delete operations
  </action>
  <verify>
    <automated>pnpm --dir web test:e2e -- -g "SETTINGS-0[123]" 2>&1 | tail -30</automated>
  </verify>
  <done>
    - All S3 Profile CRUD tests (SETTINGS-01, SETTINGS-02, SETTINGS-03) pass
    - Create test: form fills, saves, new profile appears in list
    - Edit test: form pre-fills with existing values, name disabled, changes persist
    - Delete test: dialog confirmed, profile removed from list
    - Save button disabled test: verifies validation
    - Multiple profiles test: selective deletion works
  </done>
</task>

<task type="auto">
  <name>P7-02: Replace LLM Backend mock section with CRUD tests</name>
  <files>web/tests/e2e/settings.spec.ts</files>
  <action>
    Replace the existing `test.describe('LLM Management (Mock)', ...)` block (lines 57-89) with a new `test.describe('LLM Backend CRUD', ...)` block containing:

    1. **Shared mutable mock state** (module-scoped):
       ```typescript
       let mockBackends: Array<{
         name: string;
         provider: 'ollama' | 'openai';
         base_url: string;
         model: string;
         timeout_secs: number;
         has_api_key: boolean;
       }> = [];
       let mockLlmDefault: string | null = null;
       ```

    2. **Mock setup** in `test.beforeEach`:
       - Reset `mockBackends = []` and `mockLlmDefault = null` before each test
       - Register `page.route('**/settings/llm/backends', ...)` handler:
         - GET: respond with `{ backends: mockBackends, default: mockLlmDefault }`
         - POST: parse body, push/update in `mockBackends`, respond 204
       - Register `page.route('**/settings/llm/backends/*', ...)` for individual backend:
         - DELETE: filter out matching `name` from `mockBackends`, clear default if it was the deleted one, respond 204
       - Register `page.route('**/settings/llm/default', ...)` handler:
         - POST: parse body `{ name }`, set `mockLlmDefault = name`, respond with `{ name }`
         - GET: respond with `mockLlmDefault`

    3. **Test: SETTINGS-04 - Create LLM Backend**:
       - Navigate to /settings, click "大模型" tab
       - Click "新建后端" button
       - Fill #llm-name with "ollama-local"
       - Select "ollama" in #llm-provider select
       - Fill #llm-base-url with "http://127.0.0.1:11434"
       - Fill #llm-model with "qwen3:8b"
       - Fill #llm-timeout with "60"
       - Verify save button is enabled
       - Set up `waitForResponse` for the backends GET (list refresh)
       - Click save button (matches "保存")
       - Assert success alert "已保存大模型配置" visible
       - Assert "ollama-local" and "qwen3:8b" visible in list

    4. **Test: SETTINGS-04 variant - Save disabled with empty required fields**:
       - Click "大模型" tab
       - Click "新建后端" button
       - Verify save button is disabled (name, base_url, model all empty)
       - Fill only #llm-name, verify still disabled (base_url and model still empty)

    5. **Test: SETTINGS-05 - Set LLM Backend as Default**:
       - Seed `mockBackends` with two backends: 'ollama-local' (ollama) and 'openai-prod' (openai)
       - Navigate to /settings, click "大模型" tab
       - Assert neither backend shows "已默认" badge
       - Set up `waitForResponse` for the set-default POST
       - Click "设为默认" button on 'ollama-local' card
       - Assert "已默认" badge visible on 'ollama-local'
       - Assert 'openai-prod' still shows "设为默认" button (not default)

    6. **Test: SETTINGS-05 variant - Already default button is disabled**:
       - Seed `mockBackends` with one backend, `mockLlmDefault = 'ollama-local'`
       - Navigate, click "大模型" tab
       - Assert "已默认" button/text visible and the set-default button is disabled

    7. **Test: SETTINGS-06 - Delete LLM Backend**:
       - Seed `mockBackends` with one backend: 'ollama-local'
       - Navigate, click "大模型" tab
       - Register dialog handler BEFORE clicking delete:
         ```typescript
         page.on('dialog', async (dialog) => {
           expect(dialog.type()).toBe('confirm');
           expect(dialog.message()).toContain('ollama-local');
           await dialog.accept();
         });
         ```
       - Click delete button (ghost icon with Trash2 icon, destructive variant) on the 'ollama-local' card
       - Set up `waitForResponse` for the backends GET (list refresh)
       - Assert "ollama-local" no longer visible

    8. **Test: SETTINGS-06 variant - Delete default backend clears default**:
       - Seed `mockBackends` with two backends, `mockLlmDefault = 'ollama-local'`
       - Navigate, click "大模型" tab
       - Delete 'ollama-local' (with dialog handler)
       - Assert "ollama-local" removed from list
       - Assert 'openai-prod' does NOT show "已默认" badge

    Locators to use (from LlmManagement.svelte source):
    - Tab: `page.getByRole('tab', { name: '大模型' })`
    - New button: `page.getByRole('button', { name: '新建后端' })`
    - Save button: `page.getByRole('button', { name: /保存/ })`
    - Name input: `#llm-name` (disabled in edit mode)
    - Provider select: `#llm-provider`
    - Base URL input: `#llm-base-url`
    - Model input: `#llm-model` (with datalist `#llm-models`)
    - Timeout input: `#llm-timeout`
    - Set default button: `page.getByRole('button', { name: '设为默认' })`
    - Default indicator: `page.getByText('已默认')`
    - Delete button: `button` with `Trash2` icon (ghost, destructive)

    Pitfalls to handle:
    - Mock state mutation for both backends array and default backend
    - Set-default endpoint is separate from backends endpoint
    - Delete default backend must also update `mockLlmDefault`
    - Dialog handler: register BEFORE clicking delete
    - Save disabled: requires name + base_url + model all non-empty
  </action>
  <verify>
    <automated>pnpm --dir web test:e2e -- -g "SETTINGS-0[456]" 2>&1 | tail -30</automated>
  </verify>
  <done>
    - All LLM Backend CRUD tests (SETTINGS-04, SETTINGS-05, SETTINGS-06) pass
    - Create test: form fills (name, provider, base_url, model), saves, backend appears in list
    - Set-default test: "设为默认" click changes to "已默认", other backends unaffected
    - Delete test: dialog confirmed, backend removed from list
    - Save disabled test: verifies validation for required fields
    - Delete default variant: default marker cleared after deleting default backend
  </done>
</task>

</tasks>

<verification>
After both tasks complete, run the full settings test suite to ensure no regressions from replacing the old mock sections:

```bash
pnpm --dir web test:e2e -- settings 2>&1 | tail -40
```

Expected: All tests pass including existing real-API tests (Planner, Agent, Server Log) and new CRUD tests.
</verification>

<success_criteria>
1. settings.spec.ts contains new `S3 Profile CRUD` test.describe block with 5 tests covering SETTINGS-01, SETTINGS-02, SETTINGS-03
2. settings.spec.ts contains new `LLM Backend CRUD` test.describe block with 6 tests covering SETTINGS-04, SETTINGS-05, SETTINGS-06
3. Both blocks use stateful mutable mock arrays (mockProfiles, mockBackends, mockLlmDefault) reset in beforeEach
4. Delete tests use `page.on('dialog')` handlers registered before click
5. All existing tests in settings.spec.ts still pass (Page Layout, Planner, Agent, Server Log, Theme, Error Handling, Navigation)
6. `pnpm --dir web test:e2e -- settings` returns exit code 0
</success_criteria>

<output>
After completion, create `.planning/phases/07-settings-s3-llm/07-01-SUMMARY.md`
</output>
