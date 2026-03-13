# Phase 1: Production Stability - Research

**Researched:** 2026-03-13
**Domain:** Rust panic safety, mutex poisoning recovery, integration test patterns
**Confidence:** HIGH

## Summary

Phase 1 aims to eliminate panic risks in OpsBox's search path and fix mutex poisoning DoS risks. Research reveals a **critical finding**: the primary unwrap counts (175 in search_executor.rs, 82 in search.rs) are entirely in **test code**, not production code. The production code sections of both files have **zero unwraps**. This fundamentally changes the scope of SAFE-01.

The mutex poisoning risk in `agent/src/routes.rs` is real and requires fixing. The boundary test stubs (5 tests) and S3 test skip are genuine gaps that need implementation.

**Primary recommendation:** Re-scope SAFE-01 from "unwrap cleanup" to "production panic safety audit" -- verify all production paths have proper error handling, then focus effort on SAFE-02 (mutex fix) and SAFE-03/04 (test implementation).

## User Constraints (from CONTEXT.md)

### Locked Decisions

| Decision | Detail |
|----------|--------|
| Unwrap replacement: infallible | Use `expect("infallible: reason")` |
| Unwrap replacement: has default | Use `warn!` + `unwrap_or_default()` |
| Unwrap replacement: error path | Use `?` propagation |
| Mutex: synchronous code | Use `parking_lot::Mutex` |
| Mutex: HTTP handler | Use `tokio::sync::Mutex` |
| Test depth | Full verification (real results + actual attacks + 10+ concurrent) |
| S3 testing | Mock implementation, real assertions |

### Claude's Discretion
Not specified in CONTEXT.md -- open for recommendations.

### Deferred Ideas (OUT OF SCOPE)
- network.rs `ENV_MUTEX` and unsafe env operations
- Adding `clippy::unwrap_used` lint
- Unified project-wide mutex strategy
- Shutdown timeout configurability

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio | 1.x (workspace) | Async runtime | Already used throughout project |
| parking_lot | 0.12 | Poison-free sync Mutex | No poisoning, faster than std, no unsafe needed |
| tokio::sync::Mutex | (in tokio) | Async-safe Mutex | Required for `.await` while holding lock |
| thiserror | 2 | Error derive macros | Already used for ServiceError/AppError |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tracing | 0.1 (workspace) | Structured logging | warn!/error! for error recovery paths |
| tempfile | 3 (dev) | Temp directories in tests | Already used in boundary tests |
| test-common | workspace | Shared test utilities | create_test_file(), TestDatabase |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| parking_lot | std::sync::Mutex + catch_unwind | catch_unwind is fragile, doesn't prevent poisoning |
| parking_lot | tokio::sync::Mutex (sync contexts) | tokio Mutex requires runtime, overkill for sync code |
| Mock S3 | real S3/minio via docker | CI complexity, port conflicts, slow |

**Installation:**
```toml
# In agent/Cargo.toml [dependencies]
parking_lot = "0.12"
```

## Architecture Patterns

### Mutex Poisoning Recovery Strategy

**Current problem:** `agent/src/routes.rs:108,137` uses `std::sync::Mutex` with `.lock().unwrap()`. If the mutex is poisoned (panic while holding lock), every subsequent request panics -- DoS.

**Pattern 1: parking_lot::Mutex (sync contexts)**
```rust
// Source: https://docs.rs/parking_lot/latest/parking_lot/type.Mutex.html
// parking_lot never returns PoisonError -- no .unwrap() needed
use parking_lot::Mutex;

let data = Mutex::new(vec![1, 2, 3]);
let mut guard = data.lock(); // Returns MutexGuard directly, no Result
guard.push(4);
```

Use for: `daemon.rs`, `daemon_windows.rs`, sync initialization code.

**Pattern 2: tokio::sync::Mutex (async/HTTP handler contexts)**
```rust
// Source: https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html
use tokio::sync::Mutex;

async fn handler(State(state): State<AppState>) -> Result<Json<Response>> {
    let mut level = state.current_log_level.lock().await;
    *level = new_value;
    Ok(Json(response))
}
```

Use for: `agent/src/routes.rs` HTTP handlers.

**Pattern 3: Poison recovery (if migration is too large)**
```rust
// Fallback: recover from poisoned mutex by extracting inner data
let data = match mutex.lock() {
    Ok(guard) => guard,
    Err(poisoned) => {
        warn!("Mutex was poisoned, recovering");
        poisoned.into_inner()
    }
};
```

### Unwrap Classification System

The CONTEXT.md defines a three-tier replacement strategy. Production code in the search path already follows this correctly (zero unwraps). For any remaining unwraps found during audit:

| Category | Detection | Replacement |
|----------|-----------|-------------|
| Infallible | Value guaranteed non-empty by prior check | `.expect("infallible: [reason]")` |
| Has default | Fallback value is sensible | `tracing::warn!(...); value.unwrap_or_default()` |
| Error path | Failure should propagate | `?` operator |

### Test Implementation Patterns

**Existing pattern in boundary_integration.rs:**
```rust
#[tokio::test]
async fn test_example() {
    let mut generator = TestFileGenerator::new().expect("...");
    let path = generator.create_file("test.log", content).await.expect("...");
    // Assertions needed here
}
```

**Search test pattern (from agent/src/routes.rs tests):**
```rust
let response = app.oneshot(Request::builder()...).await.unwrap();
assert_eq!(response.status(), StatusCode::OK);
let events = collect_ndjson_events(response).await;
let found_match = events.iter().any(|e| e["type"] == "result");
assert!(found_match, "Should find match result");
```

**S3 mock pattern (from test-common/s3_mock.rs):**
```rust
let mock_server = s3_mock::start_mock_s3_server(port).await?;
let endpoint = mock_server.endpoint();
// Configure S3 client with mock endpoint
// Perform operations
mock_server.stop().await?;
```

### Anti-Patterns to Avoid

- **expect("unwrap")**: Useless message. Must describe WHY it cannot fail.
- **unwrap() in loops**: Use `?` with `continue` or proper error propagation.
- **Silent error swallowing**: Always log with `warn!` or `error!` when falling back.
- **std::sync::Mutex in HTTP handlers**: Always use `tokio::sync::Mutex` when holding lock across `.await`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Mutex poisoning recovery | catch_unwind wrapper | parking_lot::Mutex | parking_lot never poisons, no recovery needed |
| S3 mock server | Custom HTTP server matching S3 API | test-common/s3_mock.rs | Already exists, has ListObjectsV2 XML generation |
| Test file generation | Manual tempfile management | test-common/file_utils::TestFileGenerator | Handles cleanup, encoding, large files |
| Search event collection | Manual channel polling | collect_ndjson_events() helper | Already exists in agent route tests |

**Key insight:** The codebase already has test infrastructure for most patterns. New tests should reuse existing helpers from `test-common` and `opsbox_test_common`.

## Common Pitfalls

### Pitfall 1: Treating Test Unwraps as Production Unwraps
**What goes wrong:** Mechanical replacement of all unwraps in a file, including test code.
**Why it happens:** grep counts don't distinguish production from `#[cfg(test)]` code.
**How to avoid:** Always check if unwrap is in a `#[cfg(test)]` module before modifying. Test unwraps are idiomatic and should stay.
**Warning signs:** Changing code after line 385 in search_executor.rs or after line 862 in search.rs.

### Pitfall 2: parking_lot in async context
**What goes wrong:** Using `parking_lot::Mutex` in code that holds the lock across `.await` points.
**Why it happens:** parking_lot is synchronous -- holding its guard across await can cause deadlocks.
**How to avoid:** Use `tokio::sync::Mutex` for any lock held during async operations. Use parking_lot only for sync code or very short critical sections.
**Warning signs:** `let guard = mutex.lock(); some_async().await;` pattern.

### Pitfall 3: Incomplete boundary test assertions
**What goes wrong:** Test creates files but doesn't actually run searches or verify results.
**Why it happens:** SearchExecutor setup is complex, so tests skip the hard part.
**How to avoid:** Follow the pattern in agent/src/routes.rs tests -- use `create_router` + `oneshot` for integration, or construct SearchExecutor directly for unit tests.
**Warning signs:** Tests that only print success messages without `assert!()` calls.

### Pitfall 4: S3 mock server port conflicts
**What goes wrong:** Multiple tests try to bind the same port, causing intermittent failures.
**Why it happens:** Hard-coded port numbers in tests.
**How to avoid:** Use port 0 to let OS assign, or use the test-common port range with unique offsets.
**Warning signs:** Tests that work locally but fail in CI.

## Code Examples

### Mutex Migration: HTTP Handler (agent/src/routes.rs:108)

**Before (current -- panics on poisoned mutex):**
```rust
let current_level = state.config.current_log_level.lock().unwrap().clone();
```

**After (tokio::sync::Mutex -- no poisoning):**
```rust
// In AgentConfig:
pub current_log_level: Arc<tokio::sync::Mutex<String>>,

// In handler:
let current_level = state.config.current_log_level.lock().await.clone();
```

### Mutex Migration: Sync Code (daemon.rs)

**Before:**
```rust
let mut guard = SOME_GLOBAL.lock().unwrap();
```

**After:**
```rust
use parking_lot::Mutex;
// parking_lot::Mutex::lock() returns MutexGuard directly, never panics
let mut guard = SOME_GLOBAL.lock();
```

### Boundary Test: Mixed Encoding Search

**Pattern to follow (from agent route tests):**
```rust
#[tokio::test]
async fn test_mixed_encoding_search() {
    let mut generator = TestFileGenerator::new().expect("...");

    // Create files with different encodings
    let utf8_path = generator.create_file("utf8.log", "UTF-8 content\nERROR message\n").await.unwrap();
    // For GBK: write raw bytes
    let gbk_bytes = encoding_rs::GBK.encode("GBK 错误消息").0.into_owned();
    std::fs::write(generator.dir().join("gbk.log"), &gbk_bytes).unwrap();

    // Use SearchProcessor directly for unit-level testing
    let query = Query::parse_github_like("ERROR|错误").unwrap();
    let processor = SearchProcessor::new(Arc::new(query), 0);

    let mut file = tokio::fs::File::open(&utf8_path).await.unwrap();
    let result = processor.process_content(utf8_path.to_string_lossy().to_string(), &mut file).await;
    assert!(result.is_ok());
    let search_result = result.unwrap();
    assert!(search_result.is_some(), "Should find match in UTF-8 file");
}
```

### S3 API Test: Using Mock Server

**Pattern from existing s3_integration.rs + s3_mock.rs:**
```rust
#[tokio::test]
async fn test_s3_api_endpoints() {
    // Start mock S3 server
    let port = 19050; // Use unique port
    let mock = match s3_mock::start_mock_s3_server(port).await {
        Ok(s) => s,
        Err(_) => {
            println!("Skipping: port unavailable");
            return;
        }
    };

    // Create test DB and save profile
    let db = TestDatabase::in_memory().await.unwrap();
    init_logseek_schema(&db.pool).await.unwrap();
    let profile = S3Profile {
        profile_name: "test".to_string(),
        endpoint: mock.endpoint(),
        access_key: "key".to_string(),
        secret_key: "secret".to_string(),
    };
    s3_repo::save_s3_profile(&db.pool, &profile).await.unwrap();

    // Test list endpoint via router
    // ... create router, send request, assert response

    mock.stop().await.unwrap();
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `.unwrap()` in production | Proper error handling with `?` | Already done in search path | Search code is panic-safe |
| `std::sync::Mutex` everywhere | Context-appropriate Mutex types | Needed now | Eliminates DoS via mutex poisoning |
| Stub integration tests | Real assertion-based tests | Needed now | Actual confidence in boundary behavior |
| Manual S3 test setup | Mock server from test-common | Infrastructure exists | Reuse existing mock |

**Deprecated/outdated:**
- CONCERNS.md unwrap counts (175 + 82): These are test code unwraps, not production. The counts led to an inflated sense of production risk.

## Open Questions

1. **What production unwraps actually exist?**
   - What we know: search_executor.rs and search.rs production sections have zero unwraps
   - What's unclear: Whether other files in the search path (e.g., search_runner.rs, searchable.rs) have production unwraps
   - Recommendation: Do a quick audit of the actual search path production code, not just the two largest files

2. **Should test unwraps be cleaned up?**
   - What we know: Test unwraps are idiomatic in Rust and generally acceptable
   - What's unclear: Whether the project wants test unwraps replaced too
   - Recommendation: Leave test unwraps as-is. They fail fast in tests, which is the desired behavior.

3. **parking_lot version compatibility?**
   - What we know: Project uses Rust 2024 edition
   - What's unclear: Whether parking_lot 0.12 supports Rust 2024
   - Recommendation: Verify during implementation; 0.12 should work, may need 0.13+

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) |
| Config file | none -- standard cargo test |
| Quick run command | `OPSBOX_NO_PROXY=1 cargo test -p logseek --test boundary_integration` |
| Full suite command | `OPSBOX_NO_PROXY=1 cargo test --manifest-path backend/Cargo.toml` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|--------------|
| SAFE-01 | No panics in search path production code | audit | Manual code review | N/A -- already clean |
| SAFE-02 | Mutex recovery from poisoning | unit + integration | `cargo test -p opsbox-agent -- routes::tests` | Partial (existing tests) |
| SAFE-03 | Boundary tests with real assertions | integration | `cargo test -p logseek --test boundary_integration` | Exists (stubs) |
| SAFE-04 | S3 API endpoint tests | integration | `cargo test -p logseek --test s3_integration` | Exists (stub) |

### Sampling Rate
- Per task commit: `OPSBOX_NO_PROXY=1 cargo test -p logseek --test boundary_integration -p opsbox-agent`
- Per wave merge: `OPSBOX_NO_PROXY=1 cargo test --manifest-path backend/Cargo.toml`
- Phase gate: Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `backend/logseek/tests/boundary_integration.rs` -- 5 stub tests need real assertions (SAFE-03)
- [ ] `backend/logseek/tests/s3_integration.rs` -- test_s3_api_endpoints needs implementation (SAFE-04)
- [ ] `backend/agent/Cargo.toml` -- needs `parking_lot = "0.12"` dependency (SAFE-02)

*(If no gaps: "None -- existing test infrastructure covers all phase requirements")*

## Sources

### Primary (HIGH confidence)
- Codebase analysis: search_executor.rs lines 1-384 (production), 385-2942 (tests)
- Codebase analysis: search.rs lines 1-861 (production), 862+ (tests)
- Codebase analysis: agent/src/routes.rs lines 108, 137 (mutex unwrap)
- Context7: parking_lot Mutex API (poison-free semantics)
- Context7: tokio::sync::Mutex API (async-safe mutex)

### Secondary (MEDIUM confidence)
- CONCERNS.md -- unwrap counts verified as primarily test code
- CONTEXT.md -- locked decisions for unwrap replacement strategy
- test-common/s3_mock.rs -- existing mock infrastructure

### Tertiary (LOW confidence)
- None

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- parking_lot and tokio::sync::Mutex are well-established, APIs verified
- Architecture: HIGH -- Mutex migration patterns are standard Rust practice
- Pitfalls: HIGH -- Test vs production unwrap confusion is a real, verified finding
- Unwrap scope: HIGH -- Verified via grep that production sections have zero unwraps

**Research date:** 2026-03-13
**Valid until:** 2026-04-13 (30 days -- stable domain, patterns won't change)
