# Domain Pitfalls

**Domain:** Code quality improvements — unwrap cleanup, frontend coverage, search performance
**Researched:** 2026-03-13
**Confidence:** MEDIUM (project-specific findings from CONCERNS.md HIGH; general Rust/Svelte patterns from domain knowledge)

## Critical Pitfalls

### Pitfall 1: Replace unwrap with expect (Still Panics)

**What goes wrong:**
Team replaces `.unwrap()` with `.expect("message")` across the codebase. The code still panics at runtime, just with a slightly better message. The fundamental problem (unhandled error cases) is not solved.

**Why it happens:**
Developers treat "remove unwrap" as a mechanical text replacement task. `expect` feels safer because it has a message, but it is still a panic in production. In `search_executor.rs` with 175 unwraps, a mechanical replacement yields 175 expects and zero improvement in reliability.

**How to avoid:**
- Categorize each unwrap before touching it: (a) truly infallible (e.g., parsing a hardcoded string), (b) should return an error to the caller, (c) has a sensible default.
- Only use `.expect()` for category (a) where the condition is provably impossible. Document the proof in a comment.
- Use `?` propagation or `AppError` for category (b).
- Use `.unwrap_or()` / `.unwrap_or_else()` for category (c).

**Warning signs:**
- PR shows bulk find-and-replace of `unwrap` with `expect`.
- No new error variants added to `AppError` despite many unwrap removals.
- Test suite passes without new test cases for error paths.

**Phase to address:**
Unwrap cleanup phase. Must be the first step of the phase, not skipped.

---

### Pitfall 2: Mutex Poisoning Cascades in HTTP Handlers

**What goes wrong:**
`.lock().unwrap()` on mutexes inside HTTP handlers (currently in `backend/agent/src/routes.rs:108,137` and `backend/opsbox-server/src/network.rs:89,112,134`) causes a poisoned mutex to crash every subsequent request. A single panic while holding a lock poisons the mutex permanently.

**Why it happens:**
Rust's `std::sync::Mutex::lock()` returns `Result` because a previous panic while holding the lock marks it as poisoned. Developers assume `.lock()` will always succeed and unwrap the result. In a long-running server, one bad request can DOS the entire endpoint.

**How to avoid:**
- Use `.lock().unwrap_or_else(|poisoned| poisoned.into_inner())` to recover from poisoned mutexes in production code.
- Better: replace `std::sync::Mutex` with `tokio::sync::Mutex` in async contexts, which does not use poisoning.
- Best: eliminate shared mutable state where possible; use message passing or immutable data.

**Warning signs:**
- Grep shows `.lock().unwrap()` in any non-test production code.
- HTTP handler functions contain mutex lock calls.
- Error logs show "poisoned lock" after a server restart.

**Phase to address:**
Unwrap cleanup phase, prioritized as highest-risk category (mutex locks in request handlers).

---

### Pitfall 3: Swallowing Errors Silently (The Over-Correction)

**What goes wrong:**
After the unwrap cleanup drive, developers over-correct by using `.unwrap_or_default()` or `let _ = ...` everywhere. Errors are silently swallowed. The system appears stable but data is silently lost or operations silently fail. Search returns incomplete results without any indication.

**Why it happens:**
The pain of panics drives an extreme reaction: "never panic, never propagate." But in a log search platform, a silently failed S3 read or encoding detection means missing search results with no feedback to the user.

**How to avoid:**
- Establish a rule: errors that affect data integrity or search completeness must be propagated to the user, not swallowed.
- Use `.unwrap_or_default()` only for truly optional data (e.g., an optional header, a display name).
- Add tracing/warning logs for every error path that does not propagate to the user, so silent failures are at least visible in server logs.

**Warning signs:**
- Increase in `let _ =` patterns in PRs.
- `.unwrap_or_default()` on `Result` types (not `Option`).
- No new `tracing::warn!` calls accompanying error suppression.

**Phase to address:**
Unwrap cleanup phase, enforced during code review.

---

### Pitfall 4: Testing Components Instead of Behavior

**What goes wrong:**
Frontend coverage push leads to tests that verify Svelte component internals (e.g., "component has this CSS class", "this prop is passed to child") rather than user-visible behavior. Coverage numbers rise but regressions still slip through because the tests are coupled to implementation.

**Why it happens:**
Route components like `explorer/+page.svelte` (1104 lines) are monolithic and hard to test behaviorally. It is tempting to write shallow tests that render the component and check a few attributes, inflating coverage without testing actual user flows.

**How to avoid:**
- For each route component, define 2-3 user stories before writing tests (e.g., "user navigates to a folder, sees its contents, downloads a file").
- Prefer `@testing-library/svelte` style queries (`getByRole`, `getByText`) over `querySelector`.
- Test composables in isolation with unit tests; test page components with integration tests that exercise real API mock responses.

**Warning signs:**
- Tests contain `querySelector('.some-class')` or check `classList.contains(...)`.
- Many tests for a component but zero tests exercise the component's event handlers.
- Coverage report shows 100% line coverage on a component but no test would fail if the component's logic was subtly changed.

**Phase to address:**
Frontend coverage phase, first week (establish testing patterns before scaling).

---

### Pitfall 5: Dual Test Environment Confusion

**What goes wrong:**
The project has two Vitest environments: browser (Chromium/Playwright) and server (Node.js). Developers write tests assuming one environment, then they fail in the other. Or tests only run in one environment, leaving the other uncovered.

**Why it happens:**
Some Svelte components use browser APIs (DOM events, `window`, `localStorage`). These fail in the Node.js server environment. Developers either skip these tests (coverage gap) or add browser-only mocks that do not reflect real behavior.

**How to avoid:**
- Use `import.meta.env.SSR` or test project tags to explicitly route tests to the correct environment.
- Put pure logic (API clients, utilities, type guards) in server tests. Put DOM-dependent tests in browser tests.
- Document which test project to use for which type of test in a `TESTING.md` or similar.

**Warning signs:**
- Tests with `// @vitest-environment jsdom` comments scattered inconsistently.
- Browser test count stays flat while server test count grows.
- PR description says "only added server tests" for UI-heavy changes.

**Phase to address:**
Frontend coverage phase, initial setup.

---

### Pitfall 6: Profile Before Optimizing

**What goes wrong:**
Developers start optimizing clone usage and memory allocation in `search_executor.rs` based on intuition. They replace `.clone()` with `Arc`, restructure data flow, and introduce complexity — but profiling later shows the bottleneck was elsewhere (e.g., SQLite writes, S3 latency, regex compilation).

**Why it happens:**
303 `.clone()` calls in the search path look obviously wrong. But clone is cheap for small types (`String` under SSO threshold, integers, `Arc`). The real bottlenecks in search are often I/O (disk reads, network calls) or algorithmic (linear scans, redundant work).

**How to avoid:**
- Generate a `cargo-flamegraph` of a realistic search workload before touching any code.
- Identify the top 3-5 hot functions from the flamegraph.
- Only optimize code shown to be hot by the profiler.
- After optimization, re-profile to confirm improvement.

**Warning signs:**
- PR description says "reduced clones" without mentioning profiling results.
- New code uses `Arc` pervasively without evidence of contention.
- Optimization PR touches 10+ files but does not include before/after benchmarks.

**Phase to address:**
Search performance phase, must be the first step.

---

### Pitfall 7: Blocking the Async Runtime

**What goes wrong:**
Search performance optimizations introduce synchronous blocking operations (file reads, CPU-intensive regex matching) inside `async fn` on the Tokio runtime. This blocks the executor thread, reducing overall concurrency. The server becomes unresponsive under concurrent searches.

**Why it happens:**
The `grep-searcher` crate uses synchronous I/O. Wrapping it in `tokio::task::spawn_blocking` is the correct approach, but developers forget or find it cumbersome. They call blocking code directly in async functions, which works fine in tests (low concurrency) but degrades under load.

**How to avoid:**
- Audit all async functions in the search path for synchronous I/O calls.
- Use `tokio::task::spawn_blocking` for any operation that does disk I/O or CPU-intensive computation.
- Use `#[tokio::test]` with multiple concurrent search requests to verify no runtime blocking.

**Warning signs:**
- Tokio console or logs show "a future has been blocking for over 100ms".
- Server becomes unresponsive when two searches run simultaneously.
- `grep-searcher` calls inside `async fn` without `spawn_blocking`.

**Phase to address:**
Search performance phase.

---

### Pitfall 8: SQLite Write Serialization Under Load

**What goes wrong:**
Search performance optimization improves read speed, but the SQLite single-writer limitation (`max_connections(1)` in `search_executor.rs:400`) becomes the new bottleneck. Concurrent searches serialize on metadata writes, negating read optimizations.

**Why it happens:**
SQLite WAL mode allows concurrent reads but only one writer at a time. The search executor writes search metadata and cache entries to SQLite. Under concurrent search load, these writes queue up. The team optimizes the read/search path but ignores the write bottleneck.

**How to avoid:**
- Batch metadata writes instead of writing per-result.
- Consider in-memory caching with periodic SQLite flushes.
- Measure write contention separately from read performance.
- If write contention is the bottleneck, consider write-behind caching or a separate write queue.

**Warning signs:**
- Profiling shows search is fast but overall request latency is high.
- SQLite busy timeout errors appear in logs under concurrent load.
- Flamegraph shows time spent in SQLite write operations.

**Phase to address:**
Search performance phase, later sub-phase after read-path optimization.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Replace `unwrap` with `expect` | No panics on happy path, easy PR | Still panics in production on edge cases | Only for truly infallible operations (hardcoded parsing) |
| Add `as any` casts in TypeScript | Fixes type errors immediately | Bypasses all type checking; future refactors break silently | Never in production code; acceptable only as a TODO with a tracking comment |
| Skip browser tests, only write server tests | Faster test runs, easier setup | UI regressions undetected | Temporarily for pure utility modules; never for route components |
| Use `Arc<Mutex<>>` for all shared state | Compiles, no borrow checker fights | Lock contention, deadlock risk | When data is genuinely shared across threads and contention is low |
| Stub tests with TODO comments | Appears to increase coverage | False sense of security; real bugs slip through | Only as a placeholder in a draft PR, never merged to main |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Unbounded search result caching | Memory grows over time; server slows after hours | Implement LRU eviction with size limits (already have `lru` crate) | After hundreds of distinct searches |
| Re-streaming large archive files | Repeated searches on same archive are slow | Add file content hash-based caching for archive entries | Files over 25KB searched more than once |
| Global S3 client mutex contention | S3 operations queue up; latency spikes | Replace `Mutex<HashMap>` with `DashMap` or `RwLock` | More than ~4 concurrent S3 operations |
| Regex recompilation | CPU spikes during search | Compile regex once and reuse via `grep-regex::RegexMatcher` | Any concurrent search workload |
| Clone-heavy search path | High memory allocation rate in profiling | Use `Arc` for shared data; use references where lifetime allows | Search paths longer than ~1000 results |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Mutex `.unwrap()` in HTTP handlers | Poisoned mutex causes DOS on that endpoint | Use `.lock().unwrap_or_else(\|p\| p.into_inner())` or `tokio::sync::Mutex` |
| Stubbed boundary/security tests | Path traversal and injection attacks go undetected | Implement the 5 stub tests in `boundary_integration.rs` with real assertions |
| No input validation on agent registration | Malformed data stored in DB; potential injection | Add validation for hostname format, port range, URL scheme |
| Silent error swallowing in search | Users get incomplete results with no indication; security events missed | Propagate errors that affect data integrity; log warnings for suppressed errors |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| `grep-searcher` (sync I/O) | Calling directly in async functions | Wrap in `tokio::task::spawn_blocking` |
| `reqwest` proxy detection | Assuming proxy settings are static at startup | Call `init_network_env()` before any `reqwest::Client` creation; `reqwest` caches proxy at first use |
| `starlark` runtime | Assuming scripts are fast | Set execution timeout; scripts can loop or allocate excessively |
| SQLite WAL mode | Assuming concurrent writes work | Only one writer at a time; batch writes or use write queue |
| Vitest dual environments | Assuming tests run everywhere | Explicitly tag tests for `server` or `client` project |

## "Looks Done But Isn't" Checklist

- [ ] **Unwrap cleanup:** Grep shows zero `.unwrap()` in non-test production code — also verify zero `.lock().unwrap()` specifically
- [ ] **Unwrap cleanup:** Error paths are tested — not just happy path tests pass
- [ ] **Unwrap cleanup:** No increase in `.unwrap_or_default()` on `Result` types (silent error swallowing)
- [ ] **Frontend coverage:** Coverage is 70%+ AND route components have behavioral tests (not just line coverage)
- [ ] **Frontend coverage:** Browser tests and server tests both have new tests added
- [ ] **Search performance:** Flamegraph shows improvement in the optimized paths — not just "fewer clones"
- [ ] **Search performance:** Concurrent search latency did not increase (no async runtime blocking)
- [ ] **Search performance:** Memory usage under sustained search load is stable (no cache leaks)

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Mechanical unwrap-to-expect replacement merged | MEDIUM | Revert PR; categorize unwraps first; redo with proper error handling |
| Tests verify implementation not behavior | HIGH | Rewrite tests from user stories; accept temporary coverage drop |
| Clone optimization without profiling | MEDIUM | Revert changes; profile first; optimize only proven hot paths |
| Blocking async runtime discovered in production | HIGH | Add `spawn_blocking` wrappers; add Tokio console monitoring; load test before deploy |
| Silent error swallowing causing data loss | HIGH | Audit all `.unwrap_or_default()` on Results; add tracing to error paths; add integration tests for error scenarios |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Expect-as-replacement (still panics) | Unwrap cleanup (Phase 1) | Grep for new `expect` calls; verify each has infallibility comment |
| Mutex poisoning in handlers | Unwrap cleanup (Phase 1) | Zero `.lock().unwrap()` in non-test code; add poison-recovery test |
| Silent error over-correction | Unwrap cleanup (Phase 1) | Code review checklist; grep for `let _` increase |
| Testing implementation not behavior | Frontend coverage (Phase 2) | Review test queries used; prefer `getByRole` over `querySelector` |
| Dual environment confusion | Frontend coverage (Phase 2) | Both test project counts increase; test tagging documented |
| Profile-before-optimize | Search performance (Phase 3) | PR includes flamegraph screenshots; benchmark results |
| Async runtime blocking | Search performance (Phase 3) | Concurrent load test; Tokio console check |
| SQLite write contention | Search performance (Phase 3) | Concurrent search benchmark; measure write queue depth |

---

*Pitfalls research for: OpsBox code quality improvements (unwrap cleanup, frontend coverage, search performance)*
*Researched: 2026-03-13*
*Sources: CONCERNS.md codebase analysis, Rust/Svelte domain knowledge*
