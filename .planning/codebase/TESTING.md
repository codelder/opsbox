# Testing Patterns

**Analysis Date:** 2026-03-13

## Test Framework

**Backend (Rust):**
- Framework: Built-in `#[test]` and `#[tokio::test]` attributes
- Assertion library: Standard `assert!`, `assert_eq!`, `assert!(matches!(...))`
- Test organization: Unit tests in `#[cfg(test)] mod tests`, integration tests in `tests/` directory
- Coverage tool: `cargo-llvm-cov`

**Run Commands:**
```bash
# Run all tests (requires OPSBOX_NO_PROXY=1 on macOS for LLM tests)
OPSBOX_NO_PROXY=1 cargo test --manifest-path backend/Cargo.toml

# Run specific module tests
cargo test -p logseek

# Run with network tests (disabled by default)
cargo test -p logseek --features network-tests

# Coverage
OPSBOX_NO_PROXY=1 cargo llvm-cov --manifest-path backend/Cargo.toml --workspace --lcov
```

**Frontend (TypeScript/Svelte):**
- Framework: Vitest with dual environments
- Browser tests: Playwright with Chromium
- Assertion library: Vitest `expect`

**Run Commands:**
```bash
# All tests
pnpm --dir web test

# Unit tests only (Node.js environment)
pnpm --dir web test:unit --run --project=server

# Browser tests only
pnpm --dir web test:unit --run --project=client
```

## Test File Organization

**Rust:**
- Unit tests: Co-located in source file as `#[cfg(test)] mod tests` at bottom
- Test modules: Separate file like `search_tests.rs` imported conditionally
- Integration tests: `backend/{module}/tests/` directory with `*_integration.rs` naming

Example structure:
```
backend/logseek/src/service/search.rs          # Source with inline tests
backend/logseek/src/service/search/search_tests.rs  # Extended test module
backend/logseek/tests/view_integration.rs      # Integration tests
backend/logseek/tests/path_filtering_integration.rs
```

**TypeScript/Svelte:**
- API tests: Co-located as `*.test.ts` (e.g., `search.test.ts` next to `search.ts`)
- Composable tests: `composables/useSearch.test.ts`
- Component tests: `*.svelte.test.ts` (e.g., `AgentManagement.svelte.test.ts`)
- Utility tests: `utils/orl.test.ts`

Coverage config in `vite.config.ts`:
```typescript
coverage: {
  provider: 'v8',
  thresholds: { lines: 70, functions: 70, branches: 60, statements: 70 }
}
```

## Test Structure

**Rust Unit Tests Pattern:**
```rust
#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_function_name_scenario() {
    // Arrange
    let spec = Arc::new(Query::new(vec![Term::Literal("foo".into())]));

    // Act
    let result = processor.should_process_path("foo.log");

    // Assert
    assert!(result);
  }

  #[tokio::test]
  async fn test_async_function() {
    // For async tests
  }
}
```

**Rust Integration Test Pattern:**
```rust
#[tokio::test]
#[cfg_attr(not(feature = "network-tests"), ignore)]
async fn test_view_cache_json_agent_integration() {
  // Runtime check for sandbox environments
  if !logseek::test_utils::is_network_binding_available() {
    eprintln!("Skip: network binding unavailable");
    return;
  }

  // Setup mock server
  let (host, port) = spawn_mock_agent().await;
  let pool = SqlitePool::connect(":memory:").await.unwrap();

  // Test logic...
}
```

**TypeScript API Test Pattern:**
```typescript
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { startSearch } from './search';

describe('Search API', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should send request with correct URL and method', async () => {
    const mockResponse = {
      ok: true,
      status: 200,
      headers: new Headers({ 'X-Logseek-SID': 'test-session-id' })
    } as unknown as Response;

    globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);

    const response = await startSearch('test query');

    expect(globalThis.fetch).toHaveBeenCalledWith(
      expect.stringContaining('/search.ndjson'),
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ q: 'test query' })
      })
    );
  });
});
```

**Svelte Component Test Pattern:**
```typescript
import { expect, test, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from '@vitest/browser/context';

// Mock API modules
vi.mock('$lib/modules/agent', () => ({
  useAgents: vi.fn()
}));

beforeEach(() => {
  vi.clearAllMocks();
  // Reset mock state
});

test('component renders with agents', async () => {
  mockAgentsStore.agents = [/* test data */];
  render(AgentManagement, {});

  const agentName = await page.getByText('Test Agent 1');
  await expect.element(agentName).toBeInTheDocument();
});
```

## Mocking

**Framework:** Vitest `vi` for TypeScript, manual mocks for Rust

**TypeScript Patterns:**
```typescript
// Global fetch mock
globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);

// Module mock
vi.mock('$lib/modules/agent', () => ({
  useAgents: vi.fn()
}));

// Clear mocks between tests
beforeEach(() => { vi.clearAllMocks(); });
afterEach(() => { vi.restoreAllMocks(); });

// Type casting for mock responses
const mockResponse = {
  ok: true,
  status: 200,
  headers: new Headers()
} as unknown as Response;
```

**Rust Patterns:**
- Mock servers: Built with Axum for integration tests
- In-memory SQLite: `SqlitePool::connect(":memory:")` for database tests
- Mock utilities in `test-common` crate: `llm_mock`, `s3_mock`, `agent_mock`

## Fixtures and Test Data

**Rust - `test-common` Crate:**
Location: `backend/test-common/src/`

Modules:
- `database.rs` - `TestDatabase` with in-memory or file-based SQLite
- `file_utils.rs` - Temp file creation
- `archive_utils.rs` - Test archive creation
- `llm_mock.rs` - LLM client mocks
- `s3_mock.rs` - S3 service mocks
- `agent_mock.rs` - Agent server mocks
- `search_utils.rs` - Search test helpers
- `orl_utils.rs` - ORL URL test utilities

Constants (`test-common/src/lib.rs`):
```rust
pub const TEST_DB_CONNECTION: &str = ":memory:";
pub const TEST_DB_POOL_SIZE: u32 = 5;
pub const TEST_FILE_DIR_PREFIX: &str = "opsbox_test_";
```

**Test Data Creation Pattern:**
```rust
fn create_test_gzip_file(file_path: &std::path::Path, content: &str) {
  let file = std::fs::File::create(file_path).unwrap();
  let mut encoder = GzEncoder::new(file, Compression::default());
  encoder.write_all(content.as_bytes()).unwrap();
  encoder.finish().unwrap();
}
```

## Coverage

**Backend:**
- Total: 1,031 tests (99.7% pass rate)
- Tool: `cargo-llvm-cov`
- Estimated: ~75-80% overall

**Frontend:**
- Total: 95 tests (100% pass rate)
- Server tests: 55 passing
- Browser tests: 40 passing
- Current: 14.85% overall (thresholds set to 70%)

**View Coverage:**
```bash
# Backend
OPSBOX_NO_PROXY=1 cargo llvm-cov --manifest-path backend/Cargo.toml --workspace --lcov

# Frontend (generates coverage/ directory)
pnpm --dir web test:unit --coverage
```

## Test Types

**Unit Tests:**
- Scope: Single function/struct
- Location: Co-located in source files
- Example: `test_should_process_path`, `test_process_content_no_match`

**Integration Tests:**
- Scope: Module interaction, API endpoints
- Location: `tests/` directory with `*_integration.rs`
- Network-dependent tests gated by `#[cfg_attr(not(feature = "network-tests"), ignore)]`
- Runtime network check: `logseek::test_utils::is_network_binding_available()`

**E2E Tests (Frontend):**
- Framework: Playwright with Chromium
- Configuration: `playwright.config.ts`
- Not included in unit test suite

## Common Patterns

**Async Testing (Rust):**
```rust
#[tokio::test]
async fn test_async_operation() {
  let pool = SqlitePool::connect(":memory:").await.unwrap();
  let result = some_async_function(&pool).await;
  assert!(result.is_ok());
}
```

**Error Testing:**
```rust
// Pattern matching for specific error types
assert!(matches!(err, AppError::Config(msg) if msg == "config error"));

// Status code testing for API errors
let response = api_err.into_response();
assert_eq!(response.status(), StatusCode::BAD_REQUEST);

// JSON body extraction helper
async fn extract_json_body(response: Response) -> serde_json::Value {
  let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
  serde_json::from_slice(&bytes).unwrap()
}
```

**Temporary File Testing:**
```rust
let temp_dir = tempfile::tempdir().unwrap();
let file_path = temp_dir.path().join("test.log");
std::fs::write(&file_path, "content").unwrap();
// temp_dir auto-cleans on drop
```

**Mock Server Pattern:**
```rust
async fn spawn_mock_agent() -> (String, u16) {
  let app = Router::new()
    .route("/api/v1/search", post(|Json(body): Json<Value>| async move {
      // Return mock response
    }));

  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = listener.local_addr().unwrap();
  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  (addr.ip().to_string(), addr.port())
}
```

## Testing Configuration

**Environment Variables:**
- `OPSBOX_NO_PROXY=1` - Required on macOS for LLM tests (disables reqwest proxy detection)
- `CI=1` - CI environment indicator

**Feature Flags:**
- `network-tests` - Enables tests requiring network binding
- `mimalloc-collect` - Enables memory collection in tests

**Vitest Dual Environment:**
```typescript
projects: [
  {
    test: {
      name: 'client',
      environment: 'browser',
      browser: { enabled: true, provider: 'playwright' },
      include: ['src/**/*.svelte.{test,spec}.{js,ts}']
    }
  },
  {
    test: {
      name: 'server',
      environment: 'node',
      include: ['src/**/*.{test,spec}.{js,ts}'],
      exclude: ['src/**/*.svelte.{test,spec}.{js,ts}']
    }
  }
]
```

## Test Distribution (Backend)

| Module | Unit | Integration | Total |
|--------|------|-------------|-------|
| logseek | 413 | 55 | 468 |
| opsbox-core | 73 | 206 | 279 |
| agent | 10 | 144 | 154 |
| explorer | 17 | 9 | 26 |
| agent-manager | 11 | 11 | 22 |
| opsbox-server | 27 | - | 27 |
| test-common | 20 | - | 20 |

---

*Testing analysis: 2026-03-13*
