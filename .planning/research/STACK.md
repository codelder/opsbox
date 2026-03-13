# Stack Research: Platform Quality Improvement

**Domain:** Rust error handling, SvelteKit testing, search performance optimization
**Researched:** 2026-03-13
**Confidence:** MEDIUM (WebSearch unavailable; recommendations based on CLAUDE.md analysis, codebase audit, and training data through early 2025)

## Recommended Stack

### Rust Error Handling (unwrap replacement)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `clippy::unwrap_used` | built-in | Lint to ban unwrap in production code | Standard Clippy lint; catches unwrap at compile time with clear error messages |
| `clippy::expect_used` | built-in | Optional stricter lint | Warns on `.expect()` too; use if you want zero panicking code paths |
| `thiserror` | 2.x | Derive macro for error types | Already in use; enables clean `#[from]` conversions without boilerplate |
| `anyhow` | 1.x | Context-rich error handling | Use ONLY in binary crates (opsbox-server, agent); NOT in library crates |
| `color-eyre` / `eyre` | 0.6 | Enhanced error reports | Alternative to anyhow with better formatting; consider for CLI tooling |

### SvelteKit/Svelte 5 Testing

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `vitest` | 3.2 | Test runner | Already configured; dual project setup (browser + server) is correct |
| `vitest-browser-svelte` | latest | Svelte component rendering in browser | Required for Svelte 5 component tests; replaces @testing-library/svelte |
| `@vitest/browser` | latest | Browser test provider | Already using Playwright provider; correct choice |
| `@testing-library/jest-dom` | latest | DOM assertions | Provides `toBeInTheDocument()`, `toBeDisabled()` etc. |
| `msw` (Mock Service Worker) | 2.x | API mocking | Better than vi.mock() for API-heavy components; intercepts at network level |
| `svelte-htm` | latest | Inline Svelte templates | Useful for quick component test scaffolding |

### Search Performance

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `cargo-flamegraph` | latest | CPU profiling | Generates flame graphs; identifies hot paths and expensive clones |
| `cargo-llvm-cov` | 0.6 | Code coverage | Already in use; verify coverage after refactor |
| `dashmap` | 6.x | Concurrent HashMap | Replace `Mutex<HashMap>` for S3 client cache; lock-free reads |
| `arc-swap` | 1.x | Atomic Arc swapping | For read-heavy shared state; cheaper than RwLock |
| `memmap2` | 0.9 | Memory-mapped file I/O | For large file search; avoids copying file contents |
| `pprof` | 0.14 | CPU/memory profiling | In-process profiling; generates flame graphs from runtime data |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| `cargo clippy -- -D clippy::unwrap_used` | Ban unwrap in CI | Add to CI pipeline; catches new unwraps |
| `cargo-deny` | Dependency auditing | Check for advisories, duplicate deps, license compliance |
| `cargo-tarpaulin` | Alternative coverage | Linux-only but more accurate than llvm-cov for some metrics |
| `svelte-check` | TypeScript/Svelte type checking | Run in CI to catch type errors; already available via SvelteKit |

## Installation

```bash
# Rust profiling tools
cargo install cargo-flamegraph
cargo install cargo-llvm-cov  # Already installed

# Add to Cargo.toml [workspace.dependencies]
dashmap = "6"
arc-swap = "1"
memmap2 = "0.9"

# Frontend testing (already installed)
# vitest, vitest-browser-svelte, @vitest/browser
```

## Error Handling Patterns for OpsBox

### Pattern 1: Clippy Lint Configuration (Cargo.toml)

Add to workspace `Cargo.toml`:

```toml
[workspace.lints.clippy]
unwrap_used = "warn"          # Start with warn, promote to deny after cleanup
expect_used = "warn"          # Optional: also flag .expect()
mutex_atomic = "warn"         # Flag Mutex<bool> etc. (use AtomicBool)
# For test modules, allow unwrap:
# #[cfg_attr(test, allow(clippy::unwrap_used))]
```

Per-crate `Cargo.toml`:
```toml
[lints]
workspace = true
```

**Why this approach:** Workspace-level lint configuration ensures consistency. Starting with `warn` allows incremental cleanup without blocking CI. Promote to `deny` once cleanup is complete.

### Pattern 2: Replacing unwrap() in HTTP Handlers

Current (problematic):
```rust
// backend/agent/src/routes.rs:108
let level = state.config.current_log_level.lock().unwrap();
```

Recommended replacement:
```rust
// Option A: Recover from poisoned mutex
let level = state.config.current_log_level.lock()
    .unwrap_or_else(|e| e.into_inner());

// Option B: Return proper error (preferred for HTTP handlers)
let level = state.config.current_log_level.lock()
    .map_err(|_| AppError::internal("配置锁被污染"))?;

// Option C: Use parking_lot::Mutex (never poisons)
use parking_lot::Mutex;
// Then .lock() never returns Err
```

**Why Option B:** HTTP handlers should return errors to clients, not panic. The `AppError` type already handles RFC 7807 responses.

### Pattern 3: Replacing unwrap() in Search Path

Current (175 unwraps in search_executor.rs):
```rust
let value = some_option.unwrap();
```

Recommended replacement by context:

```rust
// When the value MUST exist (programmer error if missing):
let value = some_option.expect("source_plan must exist after planning phase");

// When missing value is a runtime error:
let value = some_option.ok_or_else(|| AppError::internal("source plan missing"))?;

// When missing value means "skip this item":
let Some(value) = some_option else { continue; };

// When you have a sensible default:
let value = some_option.unwrap_or_default();
```

**Why differentiate:** Not all unwraps are equal. `expect()` documents invariants. `?` propagates errors. `let-else` handles control flow. Using the right pattern makes intent clear.

### Pattern 4: Mutex Locking Strategy

Current: `std::sync::Mutex` with `.lock().unwrap()`

Recommended migration:

| Context | Use | Why |
|---------|-----|-----|
| Async code (tokio) | `tokio::sync::Mutex` | Doesn't block async runtime |
| Sync code, global cache | `parking_lot::Mutex` | Faster, never poisons |
| Read-heavy shared data | `arc_swap::ArcSwap` or `RwLock` | Concurrent reads without blocking |
| S3_CLIENT_CACHE | `dashmap::DashMap` | Lock-free concurrent access |

## SvelteKit Testing Strategy

### Current State Analysis

- 95 tests exist (55 server + 40 browser)
- Coverage: 14.85% (threshold: 70%)
- Well-tested: ORL utils (92%), Explorer API (88%), UI primitives (84-100%)
- Untested: Route components (+page.svelte files), most composables

### Coverage Gap Breakdown

| Area | Files | Current Coverage | Target |
|------|-------|------------------|--------|
| API clients | 6 files | ~88% | Maintain |
| Composables | 5 files | ~40% | 80% |
| Route components | 8 files | ~5% | 60% |
| UI components | 15+ files | ~85% | Maintain |
| Utilities | 8 files | ~93% | Maintain |

### Testing Priority Order

1. **Composables** (highest ROI)
   - `useSearch.test.ts` exists but needs more scenarios
   - Add: error states, edge cases, concurrent operations
   - Pattern: Mock API, test state transitions

2. **Route component logic extraction**
   - Extract business logic from `+page.svelte` into testable functions/composables
   - Example: `explorer/+page.svelte` (1104 lines) should have logic in a separate module
   - Then test the module, not the component

3. **Route component rendering tests**
   - Use `vitest-browser-svelte` `render()` function
   - Mock all API calls with `vi.mock()` or `msw`
   - Test: renders correctly, handles loading/error states, user interactions

### What NOT to Test

| Avoid | Why | Do Instead |
|-------|-----|------------|
| Snapshot tests for Svelte components | Brittle, low value | Test behavior and accessibility |
| 100% line coverage on components | Diminishing returns | Focus on branches and user flows |
| Testing Svelte internals | Framework tests its own code | Test your component's public interface |

### Example Test Pattern for Route Components

```typescript
// web/src/routes/search/+page.svelte.test.ts
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from '@vitest/browser/context';
import SearchPage from './+page.svelte';

// Mock the composables
vi.mock('$lib/modules/logseek/composables/useSearch', () => ({
  useSearch: vi.fn(() => ({
    results: [],
    loading: false,
    error: null,
    search: vi.fn()
  }))
}));

test('renders empty state when no results', async () => {
  render(SearchPage, {});
  const emptyState = await page.getByText('输入关键词开始搜索');
  await expect.element(emptyState).toBeInTheDocument();
});
```

## Search Performance Optimization

### Profiling First, Optimizing Second

Before making ANY performance changes:

```bash
# 1. Generate flamegraph for search workload
CARGO_PROFILE_RELEASE_DEBUG=true cargo flamegraph \
  --manifest-path backend/Cargo.toml \
  -p opsbox-server -- \
  --port 4000

# 2. Then trigger search workload and capture profile
# 3. Analyze flamegraph for:
#    - Wide bars (hot functions)
#    - Clone() calls in stack traces
#    - Mutex contention
```

**Why profile first:** The 303 `.clone()` calls and 175 `.unwrap()` calls may NOT be performance bottlenecks. Measure before optimizing.

### Optimization Candidates (from codebase analysis)

| Issue | Location | Fix | Expected Impact |
|-------|----------|-----|-----------------|
| Global Mutex for S3 cache | `opsbox-core/storage/s3.rs:54` | Replace with `DashMap` | High - eliminates contention |
| No file content indexing | `entry_stream.rs` | Add mmap-based offset cache | Medium - avoids re-streaming |
| Excessive cloning in search | `search_executor.rs` | Use `Arc` for shared data | Medium - depends on profiling |
| SQLite single-writer | `search_executor.rs:400` | Batch writes, use WAL properly | Low - single-user OK |
| `std::sync::Mutex` in async | Multiple files | Migrate to `tokio::sync::Mutex` | Medium - prevents blocking |

### S3 Client Cache Optimization

Current (global Mutex):
```rust
static S3_CLIENT_CACHE: Lazy<Mutex<HashMap<String, Arc<S3Client>>>> = Lazy::new(...);
```

Recommended (lock-free reads):
```rust
// Option A: DashMap
static S3_CLIENT_CACHE: Lazy<DashMap<String, Arc<S3Client>>> = Lazy::new(DashMap::new);

// Usage - no explicit lock needed
if let Some(client) = S3_CLIENT_CACHE.get(profile_name) {
    return client.clone();
}
// Insert with entry API
let client = S3_CLIENT_CACHE.entry(profile_name.to_string())
    .or_insert_with(|| Arc::new(build_client(config)))
    .clone();
```

**Why DashMap:** Shard-based concurrent HashMap. Reads are lock-free. Writes lock only the relevant shard. Perfect for read-heavy cache patterns.

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| `clippy::unwrap_used` | `#![deny(clippy::unwrap_used)]` per-file | If you want to enforce per-module rather than workspace |
| `thiserror` | `anyhow` | Only in binary crates where you want context chains, not typed errors |
| `DashMap` | `RwLock<HashMap>` | When you need atomic multi-key operations (DashMap locks per-shard) |
| `vitest-browser-svelte` | `@testing-library/svelte` | Testing-library does not support Svelte 5 Runes yet |
| `cargo-flamegraph` | `perf` + `flamegraph.pl` | perf is more powerful but Linux-only; cargo-flamegraph cross-platform |
| `memmap2` | `std::io::BufReader` | Use BufReader for <10MB files; mmap for large files with random access |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| `anyhow` in library crates | Hides error types from callers; makes matching impossible | `thiserror` with typed errors |
| `unsafe` for env var manipulation | Deprecated in Rust 2024 edition for multi-threaded contexts | Config struct passed through app state |
| `@testing-library/svelte` for Svelte 5 | Does not support Runes `$state`/`$derived` | `vitest-browser-svelte` |
| `async-tar` AND `tokio-tar` simultaneously | Two competing libraries; maintenance burden | Consolidate to `tokio-tar` only |
| `.unwrap_or(panic!("..."))` | No better than unwrap; still panics | `.expect("descriptive message")` |

## Version Compatibility Notes

| Concern | Note |
|---------|------|
| Svelte 5 + vitest-browser-svelte | Ensure using latest vitest-browser-svelte; Svelte 5 support is recent |
| Rust 2024 edition | `std::env::set_var` is deprecated; use config struct instead |
| `clippy::unwrap_used` | Workspace lints require Rust 1.74+ (resolver v3 already in use) |
| `dashmap` 6.x | API changes from 5.x; check if concurrent entry API is available |

## Sources

- CLAUDE.md — Project architecture and test infrastructure
- CONCERNS.md — Codebase audit (unwrap counts, mutex patterns, test gaps)
- STACK.md — Current technology stack
- Rust Clippy documentation — `unwrap_used` lint configuration
- Vitest documentation — Coverage configuration and browser testing
- DashMap documentation — Concurrent HashMap patterns
- Training data through early 2025 (MEDIUM confidence — verify with official docs)

---

*Stack research for: Platform quality improvement (unwrap cleanup, test coverage, search performance)*
*Researched: 2026-03-13*
*Confidence: MEDIUM — WebSearch unavailable; based on codebase analysis and training data*
