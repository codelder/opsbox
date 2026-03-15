# E2E Test Architecture Research: Playwright Patterns for OpsBox

## Executive Summary

This document analyzes the current E2E test architecture in OpsBox's `web/tests/e2e/` directory (18 spec files) and recommends best practices for test organization, shared utilities, and performance optimization based on Playwright's capabilities and the existing patterns.

---

## 1. Current Architecture Analysis

### 1.1 File Structure

```
web/tests/e2e/
  fixtures.ts              # Custom test fixtures with ResourceTracker
  e2e-env.ts               # E2E database path configuration
  global-setup.ts          # Global setup (cleanup, agent pre-compilation)
  global-teardown.ts       # Global teardown (process/dir cleanup)
  utils/
    agent.ts               # Agent spawning and lifecycle utilities
  home.spec.ts             # Basic home page tests
  search.spec.ts           # Search functionality tests
  search_ux.spec.ts        # Search UX interaction tests
  settings.spec.ts         # Settings page tests (mix of real/mock APIs)
  image_viewer.spec.ts     # Image viewing tests
  explorer_interaction.spec.ts  # Explorer UI interaction tests
  local_gz_archive.spec.ts # Local gzip archive tests
  s3_archive.spec.ts       # S3 archive tests
  agent_archive_explorer.spec.ts  # Agent archive explorer tests
  integration_local.spec.ts       # Local file search integration
  integration_agent.spec.ts       # Agent-based search integration
  integration_explorer.spec.ts    # Explorer integration with agents
  integration_mixed.spec.ts       # Mixed source integration
  integration_multi_source.spec.ts  # Multi-source search
  integration_performance.spec.ts   # Performance boundary tests
  integration_query_syntax.spec.ts  # Query syntax validation
  integration_relative_glob.spec.ts # Relative glob pattern tests
```

### 1.2 Current Patterns Observed

#### Strengths
- **ResourceTracker** (`fixtures.ts`): Well-designed custom fixture for tracking and cleaning up processes, directories, agents, planners, and profiles
- **Agent Utilities** (`utils/agent.ts`): Centralized agent spawning with `getFreePort()`, `spawnAgent()`, `waitForAgentReady()`
- **Global Setup/Teardown**: Proper cleanup of orphaned processes and temp directories
- **Test Isolation**: Unique `RUN_ID` values prevent parallel test interference
- **Serial Mode**: Tests that write state use `test.describe.configure({ mode: 'serial' })`

#### Weaknesses
- **Duplicated Helper Functions**: `writeTarFile()`, `writeTarGzFile()`, `writeGzFile()`, `getFreePort()` are copied across multiple spec files
- **Inline Server/Agent Management**: `ensureBackendUp()`, `ensureWebUp()`, `stopProcess()` duplicated in `integration_agent.spec.ts`
- **No Page Object Model**: Direct selectors scattered throughout tests
- **No Shared Assertion Helpers**: Common validation patterns repeated
- **Mixed Testing Strategies**: Some tests use real APIs, others use mocks inconsistently
- **Large Test Files**: `integration_explorer.spec.ts` is 765 lines with complex inline setup

---

## 2. Recommended Architecture Patterns

### 2.1 Test Organization Strategy

**Recommendation: Hybrid approach by domain + test type**

Keep the current feature-based organization but add domain-specific directories:

```
web/tests/e2e/
  _setup/                  # Shared setup files (replaces root-level files)
    fixtures.ts            # Extended test fixtures
    e2e-env.ts             # Environment configuration
    global-setup.ts        # Global setup
    global-teardown.ts     # Global teardown

  _helpers/                # Shared utility modules
    agent.ts               # Agent utilities (existing, enhanced)
    archive.ts             # Archive creation helpers (NEW)
    backend.ts             # Backend server management (NEW)
    data.ts                # Test data factories (NEW)
    assertions.ts          # Custom assertion helpers (NEW)
    network.ts             # Network utilities: port allocation, health checks (NEW)

  _page-objects/           # Page Object Models (optional, for complex pages)
    SearchPage.ts          # Search page interactions
    ExplorerPage.ts        # Explorer page interactions
    SettingsPage.ts        # Settings page interactions
    ViewPage.ts            # File viewer interactions

  search/                  # Search feature tests
    search-basic.spec.ts
    search-ux.spec.ts
    search-query-syntax.spec.ts
    search-performance.spec.ts

  explorer/                # Explorer feature tests
    explorer-local.spec.ts
    explorer-agent.spec.ts
    explorer-archive.spec.ts
    explorer-interaction.spec.ts

  integration/             # Cross-feature integration tests
    integration-local.spec.ts
    integration-agent.spec.ts
    integration-mixed.spec.ts
    integration-multi-source.spec.ts

  settings/                # Settings page tests
    settings-planner.spec.ts
    settings-llm.spec.ts
    settings-agent.spec.ts
```

### 2.2 Rationale for Current Flat Structure

The current flat structure works for OpsBox because:
1. **Clear Naming Convention**: Files are prefixed with `integration_` for integration tests
2. **Low Coupling**: Tests are self-contained with local setup/teardown
3. **Easy Discovery**: All tests visible at one level

**Recommendation**: Keep the flat structure for now but extract duplicated code into `_helpers/`. Reorganize into directories only when the test count exceeds ~25 files.

---

## 3. Page Object Model (POM) Analysis

### 3.1 Current State: No POM

Tests use direct selectors throughout:
```typescript
// Current pattern - scattered selectors
const searchInput = page.getByPlaceholder('搜索...');
await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果');
await expect(page.getByRole('button', { name: '本地文件' })).toBeVisible();
```

### 3.2 POM Recommendation: Selective Adoption

**When to use POM:**
- Complex pages with many interactive elements (Explorer, Settings)
- Pages with multiple test files covering the same UI
- When selector changes require updates in many places

**When to skip POM:**
- Simple pages with few interactions
- One-off tests
- Tests focused on API integration rather than UI

**Recommended POM Example for Search Page:**
```typescript
// _page-objects/SearchPage.ts
export class SearchPage {
  constructor(private page: Page) {}

  async goto() {
    await this.page.goto('/search');
    await this.page.waitForLoadState('networkidle');
  }

  async search(query: string) {
    await this.page.getByPlaceholder('搜索...').fill(query);
    await this.page.getByPlaceholder('搜索...').press('Enter');
  }

  async waitForResults(expectedCount?: number) {
    await this.page.waitForFunction(
      () => /\d+\s*个结果/.test(
        document.querySelector('.text-lg.font-semibold')?.textContent || ''
      ),
      { timeout: 60000 }
    );
    if (expectedCount !== undefined) {
      await expect(this.page.locator('.text-lg.font-semibold'))
        .toContainText(`${expectedCount} 个结果`);
    }
  }

  getResultCards() {
    return this.page.locator('[data-result-card]');
  }

  async getResultCount(): Promise<number> {
    const text = await this.page.locator('.text-lg.font-semibold').textContent();
    return parseInt(text?.match(/(\d+)/)?.[1] || '0');
  }
}
```

---

## 4. Shared Utilities Organization

### 4.1 Extract to `_helpers/archive.ts`

**Problem**: `writeTarFile()` is duplicated in 4+ spec files (60+ lines each).

**Solution**:
```typescript
// _helpers/archive.ts
export interface ArchiveEntry {
  name: string;
  content: string;
}

export function writeTarFile(outFile: string, entries: ArchiveEntry[]): void { /* ... */ }
export function writeTarGzFile(outFile: string, entries: ArchiveEntry[]): void { /* ... */ }
export function writeGzFile(outFile: string, content: string): void { /* ... */ }
```

### 4.2 Extract to `_helpers/backend.ts`

**Problem**: Backend/web server lifecycle management duplicated in `integration_agent.spec.ts`.

**Solution**:
```typescript
// _helpers/backend.ts
export interface ServerProcess {
  proc: ChildProcessWithoutNullStreams | null;
  started: boolean;
}

export async function ensureBackendUp(request: APIRequestContext, repoRoot: string): Promise<ServerProcess>
export async function ensureWebUp(request: APIRequestContext, repoRoot: string): Promise<ServerProcess>
export async function waitForHttpOk(request: APIRequestContext, url: string, timeoutMs: number): Promise<void>
```

### 4.3 Extract to `_helpers/network.ts`

**Problem**: Port allocation and health checking duplicated across agent and integration tests.

**Solution**:
```typescript
// _helpers/network.ts
export function getFreePort(): Promise<number>
export async function waitForHealthy(url: string, timeout?: number, interval?: number): Promise<boolean>
```

### 4.4 Create `_helpers/data.ts` for Test Data Factories

```typescript
// _helpers/data.ts
export function createTestId(prefix: string): string {
  return `${prefix}_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`;
}

export function createPlannerScript(sources: string[]): string {
  return `SOURCES = [${sources.map(s => `"${s}"`).join(', ')}]`;
}

export function createTestLogContent(id: string, level = 'INFO'): string {
  return `2025-01-01 12:00:00 [${level}] Test entry ${id}`;
}
```

---

## 5. Assertion Organization

### 5.1 Current State: Inline Assertions

Tests use raw Playwright assertions directly:
```typescript
await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果');
await expect(page.getByText(UNI_ID)).toBeVisible();
```

### 5.2 Recommended Custom Assertions

```typescript
// _helpers/assertions.ts
export async function expectSearchResults(page: Page, expectedCount: number) {
  await expect(page.locator('.text-lg.font-semibold'))
    .toContainText(`${expectedCount} 个结果`, { timeout: 10000 });
}

export async function expectResultCardVisible(page: Page, text: string) {
  await expect(page.getByText(text)).toBeVisible();
}

export async function expectNoErrors(page: Page) {
  await expect(page.locator('body')).not.toContainText(/error|错误|500/i);
}

export async function expectSidebarButton(page: Page, name: string) {
  await expect(page.getByRole('button', { name })).toBeVisible();
}

export async function expectFileLink(page: Page, filename: string, orlPattern?: RegExp) {
  const link = page.getByRole('link', { name: filename });
  await expect(link).toBeVisible();
  if (orlPattern) {
    await expect(link).toHaveAttribute('href', orlPattern);
  }
}
```

### 5.3 API Response Validation

```typescript
// _helpers/assertions.ts
export async function expectApiSuccess(response: APIResponse) {
  expect(response.ok()).toBeTruthy();
  expect(response.status()).toBeLessThan(300);
}

export async function expectApiError(response: APIResponse, expectedStatus: number) {
  expect(response.status()).toBe(expectedStatus);
}
```

---

## 6. Test Data Strategies

### 6.1 Current Approach: Real Backend + Dynamic Test Data

**Observed Pattern**: Tests create real files and planner scripts, search against real backend.

**Pros**:
- Tests actual end-to-end behavior
- No mocking complexity
- Catches real integration issues

**Cons**:
- Slower test execution
- Requires backend compilation
- Flaky when backend is slow to start

### 6.2 Recommendation: Layered Approach

**Tier 1 - Pure UI Tests** (Fast, mock API responses):
```typescript
test('should display search results', async ({ page }) => {
  await page.route('**/search.ndjson', async (route) => {
    const mockResults = generateMockSearchResults(10);
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'application/x-ndjson' },
      body: mockResults
    });
  });
  // ... test UI rendering
});
```

**Tier 2 - Integration Tests** (Real backend, test data):
```typescript
test('should search local files', async ({ page, request }) => {
  // Create test files
  const testDir = createTempDir(tracker, 'search_test');
  writeTestFile(testDir, 'test.log', 'unique_marker_123');

  // Create planner script via API
  await createPlannerScript(request, 'test_app', `orl://local${testDir}`);

  // Search and verify
  await searchPage.search('app:test_app unique_marker_123');
  await expectSearchResults(page, 1);
});
```

**Tier 3 - E2E Tests** (Full stack, minimal):
- Reserved for critical user flows
- Run in CI with real backend
- Maximum 5-10 tests

### 6.3 Test Data Factories

```typescript
// _helpers/data.ts
export interface TestLogOptions {
  lines?: number;
  level?: string;
  marker?: string;
}

export function createTestLogFile(dir: string, filename: string, options: TestLogOptions = {}): string {
  const { lines = 1, level = 'INFO', marker = createTestId('MARKER') } = options;
  const content = Array.from({ length: lines }, (_, i) =>
    `2025-01-01 12:${String(i).padStart(2, '0')}:00 [${level}] Line ${i} ${marker}`
  ).join('\n');

  const filePath = path.join(dir, filename);
  fs.writeFileSync(filePath, content);
  return marker;
}

export function createMockSearchResult(path: string, text: string, lineNo = 1) {
  return {
    type: 'result',
    data: {
      path,
      keywords: [{ type: 'literal', text }],
      chunks: [{ range: [lineNo, lineNo], lines: [{ no: lineNo, text }] }]
    }
  };
}
```

---

## 7. Performance Considerations

### 7.1 Current Configuration

```typescript
// playwright.config.ts
fullyParallel: true,          // Good - parallel execution
workers: process.env.CI ? 1 : undefined,  // CI: serial, local: parallel
retries: process.env.CI ? 2 : 0,         // CI: 2 retries
globalTimeout: 600000,                    // 10 minutes total
```

### 7.2 Optimization Recommendations

#### 7.2.1 Test Parallelization

**Current**: All tests in a file run serially if `mode: 'serial'` is set.

**Recommendation**: Group tests that need serial execution:
```typescript
test.describe('Agent Integration', () => {
  // These tests share an agent process - must be serial
  test.describe.configure({ mode: 'serial' });

  test.describe('Basic Operations', () => {
    // These don't share state - can run in parallel with other groups
    test('should list files', ...);
    test('should navigate directories', ...);
  });
});
```

#### 7.2.2 Shared Browser Contexts

**Current**: Each test creates a new browser context (default Playwright behavior).

**Recommendation**: For read-only tests, consider `storageState`:
```typescript
// Pre-authenticated state for settings tests
test.use({ storageState: 'tests/e2e/.auth/admin.json' });
```

#### 7.2.3 Resource Cleanup Optimization

**Current**: `ResourceTracker` cleans up everything after each test.

**Recommendation**: Batch cleanup operations:
```typescript
// Use global teardown for cross-test cleanup
// Keep per-test cleanup minimal
async cleanupAll(): Promise<void> {
  await Promise.allSettled([
    this.cleanupProcesses(),
    this.cleanupApiResources(),
    this.cleanupDirectories()
  ]);
}
```

#### 7.2.4 Agent Pre-compilation

**Already Implemented**: `global-setup.ts` pre-compiles `opsbox-agent`.

**Enhancement**: Consider pre-compiling all binaries:
```typescript
async function globalSetup() {
  // Pre-compile both agent and server
  execSync('cargo build --release -p opsbox-agent -p opsbox-server', {
    cwd: backendDir,
    timeout: 300000
  });
}
```

#### 7.2.5 Database Isolation

**Already Implemented**: E2E tests use separate database (`opsbox-e2e.db`).

**Enhancement**: Consider per-worker databases for true parallelism:
```typescript
// In worker-specific setup
const workerId = process.env.TEST_WORKER_INDEX;
const dbPath = path.join(__dirname, `opsbox-e2e-worker-${workerId}.db`);
```

### 7.3 Performance Metrics to Track

| Metric | Current | Target |
|--------|---------|--------|
| Total test suite time | ~5-10 min | <5 min |
| Agent compilation time | ~2 min | Pre-compiled |
| Test isolation overhead | ~1s/test | <500ms |
| Search result wait time | 10-60s | Configurable |

---

## 8. Cleanup Strategies

### 8.1 Current Implementation

**Strengths**:
- `ResourceTracker` fixture cleans up after each test
- `global-teardown.ts` kills orphaned processes
- `global-setup.ts` forces cleanup of stale temp directories

**Weaknesses**:
- `afterAll` cleanup in integration tests sometimes fails silently
- No guaranteed cleanup if test runner crashes

### 8.2 Recommended Improvements

#### 8.2.1 Add Process PID Tracking

```typescript
// _helpers/cleanup.ts
const PID_FILE = path.join(__dirname, '.e2e-pids.json');

export function trackProcess(pid: number, label: string) {
  const pids = readPidFile();
  pids.push({ pid, label, started: Date.now() });
  fs.writeFileSync(PID_FILE, JSON.stringify(pids));
}

export function cleanupTrackedProcesses() {
  const pids = readPidFile();
  for (const { pid, label } of pids) {
    try {
      process.kill(pid, 'SIGTERM');
      console.log(`[Cleanup] Terminated ${label} (PID ${pid})`);
    } catch {
      // Process already dead
    }
  }
  fs.unlinkSync(PID_FILE);
}
```

#### 8.2.2 Temp Directory Prefix Standardization

**Current**: Multiple prefixes (`temp_`, `e2e_test_`, `temp_logs_`, `temp_agent_`, etc.)

**Recommendation**: Standardize on `e2e_temp_` prefix:
```typescript
export function createTempDir(baseName: string): string {
  const dir = path.join(__dirname, `e2e_temp_${baseName}_${Date.now()}`);
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}
```

---

## 9. Recommended Migration Plan

### Phase 1: Extract Shared Utilities (Low Risk)
1. Create `_helpers/` directory
2. Move duplicated functions: `writeTarFile`, `getFreePort`, etc.
3. Update imports in all spec files
4. Verify tests still pass

### Phase 2: Add Custom Assertions (Low Risk)
1. Create `_helpers/assertions.ts`
2. Add `expectSearchResults`, `expectNoErrors`, etc.
3. Gradually refactor tests to use new assertions

### Phase 3: Introduce Page Objects (Medium Risk)
1. Create `_page-objects/` directory
2. Start with `SearchPage.ts` (most reused)
3. Migrate one spec file at a time
4. Keep POMs thin - don't over-abstract

### Phase 4: Organize Test Directories (High Risk)
1. Only if test count exceeds 25 files
2. Create domain-specific directories
3. Move files incrementally
4. Update CI configuration

---

## 10. Anti-Patterns to Avoid

### 10.1 Over-Abstraction
- Don't create deep inheritance hierarchies
- Keep Page Objects simple and focused
- Avoid abstract base classes for test utilities

### 10.2 Shared State Between Tests
- Never rely on test execution order
- Always create fresh test data
- Clean up completely in teardown

### 10.3 Brittle Selectors
- Avoid CSS class selectors (`.text-lg.font-semibold`)
- Prefer semantic selectors: `getByRole`, `getByText`, `getByPlaceholder`
- Use `data-testid` attributes for critical elements

### 10.4 Hardcoded Waits
```typescript
// BAD
await page.waitForTimeout(5000);

// GOOD
await expect(page.getByText('Results')).toBeVisible({ timeout: 5000 });
```

---

## 11. Summary of Recommendations

| Area | Current State | Recommendation | Priority |
|------|--------------|----------------|----------|
| Helper Duplication | High (4+ files share code) | Extract to `_helpers/` | High |
| Page Objects | None | Add for Search, Explorer | Medium |
| Custom Assertions | None | Add common validators | Medium |
| Test Organization | Flat, prefixed | Keep flat, add `_setup/`, `_helpers/` | Low |
| Test Data | Inline creation | Add factory functions | Medium |
| Mocking | Inconsistent | Define tier strategy | Medium |
| Cleanup | Good but incomplete | Add PID tracking | Low |
| Performance | Acceptable | Pre-compile binaries | Medium |

---

## 12. References

### Playwright Best Practices
- [Playwright Test Fixtures](https://playwright.dev/docs/test-fixtures)
- [Page Object Model](https://playwright.dev/docs/pom)
- [Parallel Tests](https://playwright.dev/docs/test-parallel)
- [Mock APIs](https://playwright.dev/docs/mock)

### Current Test Infrastructure Files
- `/Users/wangyue/workspace/codelder/opsboard/web/playwright.config.ts`
- `/Users/wangyue/workspace/codelder/opsboard/web/tests/e2e/fixtures.ts`
- `/Users/wangyue/workspace/codelder/opsboard/web/tests/e2e/utils/agent.ts`
- `/Users/wangyue/workspace/codelder/opsboard/web/tests/e2e/global-setup.ts`
- `/Users/wangyue/workspace/codelder/opsboard/web/tests/e2e/global-teardown.ts`
