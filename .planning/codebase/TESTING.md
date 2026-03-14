# Testing Patterns

**Analysis Date:** 2026-03-13

## Test Framework

**Runner (Rust):**
- Built-in `#[test]` and `#[tokio::test]` for async tests
- Config: Tests are in-module (`#[cfg(test)] mod tests`) and in `tests/` directory
- Coverage: `cargo-llvm-cov` with `OPSBOX_NO_PROXY=1` for LLM tests

**Runner (Frontend):**
- Vitest 3.2.4
- Config: `web/vitest.config.ts`
- Dual project setup: browser (Chromium via Playwright) + server (Node.js)

**Assertion Library:**
- Rust: Built-in `assert!`, `assert_eq!`, `assert_ne!`, `matches!`
- TypeScript: Vitest `expect` with `@vitest/browser` for component testing

**Run Commands:**
```bash
# Backend tests (requires proxy bypass for LLM tests on macOS)
OPSBOX_NO_PROXY=1 cargo test --manifest-path backend/Cargo.toml

# Frontend tests
pnpm --dir web test:unit              # All tests
pnpm --dir web test:unit --run --project=server    # Server (Node.js) only
pnpm --dir web test:unit --run --project=client    # Browser (Chromium) only

# Coverage
OPSBOX_NO_PROXY=1 cargo llvm-cov --manifest-path backend/Cargo.toml --workspace --lcov
```

## Test File Organization

**Rust Location:**
- Unit tests: Co-located in source files under `#[cfg(test)] mod tests`
- Submodule tests: `*_tests.rs` files (e.g., `search/search_tests.rs`)
- Integration tests: `tests/` directory with `*_integration.rs` naming

**TypeScript Location:**
- Unit tests: Co-located with source files (`*.test.ts`, `*.svelte.test.ts`)
- Separate test directory not used; tests follow source structure

**Structure:**
```
backend/
  logseek/src/
    service/search.rs           # Contains #[cfg(test)] mod tests
    service/search/search_tests.rs  # Extended test submodule
  logseek/tests/
    view_integration.rs         # Integration tests
    archive_search_integration.rs
web/src/
  lib/modules/logseek/api/
    search.ts                   # Source
    search.test.ts              # Co-located test
  lib/modules/logseek/composables/
    useSearch.svelte.ts         # Source
    useSearch.test.ts           # Co-located test
  routes/settings/
    AgentManagement.svelte      # Component
    AgentManagement.svelte.test.ts  # Component test
```

## Test Structure

**Rust Suite Organization:**
```rust
#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_error_constructors() {
    let err = AppError::config("config error");
    assert!(matches!(err, AppError::Config(msg) if msg == "config error"));
  }

  #[tokio::test]
  async fn test_async_operation() {
    let result = some_async_fn().await;
    assert!(result.is_ok());
  }
}
```

**TypeScript Suite Organization:**
```typescript
import { describe, it, expect, beforeEach, vi } from 'vitest';

describe('Search API', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should send correct request', async () => {
    globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);
    const result = await startSearch('query');
    expect(globalThis.fetch).toHaveBeenCalledWith(/* ... */);
  });
});
```

**Patterns:**
- Setup: `beforeEach` for resetting mocks/state
- Teardown: `afterEach` with `vi.restoreAllMocks()` for TypeScript
- Assertions: `assert!`/`assert_eq!` for Rust, `expect().toBe/toEqual()` for TypeScript

## Mocking

**Framework:** Vitest `vi` for TypeScript; manual mock structs for Rust

**Rust Mocking Pattern:**
```rust
// Mock module implementation for testing
struct MockModule;
#[async_trait]
impl Module for MockModule {
  fn name(&self) -> &'static str { "Mock" }
  fn api_prefix(&self) -> &'static str { "/api/v1/mock" }
  fn router(&self, _pool: SqlitePool) -> Router {
    Router::new().route("/test", get(|| async { "mock ok" }))
  }
  // ... other trait methods
}
```

**TypeScript Mocking Pattern:**
```typescript
// Module-level mocks
vi.mock('../api', () => ({
  startUnifiedSearch: vi.fn(),
  extractSessionId: vi.fn(() => 'test-sid'),
  deleteSearchSession: vi.fn()
}));

// Global fetch mock
globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);

// Component store mock
vi.mock('$lib/modules/agent', () => ({
  useAgents: vi.fn()
}));
```

**What to Mock:**
- External API calls (fetch, HTTP clients)
- Database connections (use `SqlitePool::connect("sqlite::memory:")`)
- Network services (mock agent servers on random ports)
- Time-dependent operations

**What NOT to Mock:**
- Core business logic
- Data transformations
- Internal pure functions

## Fixtures and Factories

**Test Data (Rust):**
```rust
// Test database helper
let db = TestDatabase::file_based().await.expect("Failed to create test database");
let service = ExplorerService::new(db.pool().clone());

// Temp file creation
let test_dir = TempDir::new().expect("Failed to create test directory");
fs::write(test_dir.path().join("file1.txt"), "content").await;

// ORL construction for tests
let orl = format!("orl://local{}", test_dir.path().display());
```

**Test Data (TypeScript):**
```typescript
// Mock response builder
const mockResponse = {
  ok: true,
  status: 200,
  headers: new Headers({ 'X-Logseek-SID': 'test-session-id' })
} as unknown as Response;

// Mock data factory
const mockAgents: AgentInfo[] = [{
  id: 'agent-1',
  name: 'Test Agent 1',
  hostname: 'host1',
  version: '1.0.0',
  last_heartbeat: Math.floor(Date.now() / 1000),
  status: { type: 'Online' },
  tags: [],
  search_roots: ['/var/log']
}];
```

**Location:**
- Rust: `backend/test-common/` crate with shared utilities (`TestDatabase`, `agent_mock`, `archive_utils`)
- TypeScript: Test data inline in test files

## Coverage

**Requirements:**
- Backend: ~75-80% overall, tested via `cargo-llvm-cov`
- Frontend: 70% lines/functions/statements, 60% branches (configured in `vitest.config.ts`)

**View Coverage:**
```bash
# Backend
OPSBOX_NO_PROXY=1 cargo llvm-cov --manifest-path backend/Cargo.toml --workspace --lcov

# Frontend
pnpm --dir web test:unit --coverage
```

## Test Types

**Unit Tests:**
- Scope: Single function/module behavior
- Location: `#[cfg(test)]` modules in source files
- Examples: Error constructors, encoding detection, path filtering

**Integration Tests:**
- Scope: Multi-module interactions, API endpoints, real I/O
- Location: `tests/` directory in each crate
- Examples: Archive navigation, view cache, search execution
- Uses `TestDatabase` from `test-common` crate

**E2E Tests:**
- Framework: Playwright (configured in `web/package.json`)
- Commands: `pnpm --dir web test:e2e`
- Variants: `test:e2e:local`, `test:e2e:mixed`, `test:e2e:search`

## Common Patterns

**Async Testing (Rust):**
```rust
#[tokio::test]
async fn test_async_search() {
  let processor = SearchProcessor::new(spec, 0);
  let mut reader = Cursor::new(b"line 1\nline 2");
  let result = processor.process_content("test.log".to_string(), &mut reader).await.unwrap();
  assert!(result.is_some());
}
```

**Async Testing (TypeScript):**
```typescript
it('should handle async search', async () => {
  vi.mocked(api.startUnifiedSearch).mockResolvedValueOnce(mockResponse);
  const state = useSearch();
  await state.search('test query');
  expect(state.query).toBe('test query');
});
```

**Error Testing (Rust):**
```rust
#[test]
fn test_error_status_codes() {
  assert_eq!(AppError::bad_request("").status_code(), StatusCode::BAD_REQUEST);
  assert_eq!(AppError::not_found("").status_code(), StatusCode::NOT_FOUND);
}
```

**Error Testing (TypeScript):**
```typescript
it('should throw on HTTP error', async () => {
  globalThis.fetch = vi.fn().mockResolvedValueOnce({ ok: false, status: 500 });
  await expect(startSearch('query')).rejects.toThrow(/HTTP 500/);
});
```

**Network Test Skipping (Rust):**
```rust
#[tokio::test]
async fn test_with_network() {
  opsbox_core::test_utils::skip_if_no_network();
  // test code...
}
```

---

*Testing analysis: 2026-03-13*
