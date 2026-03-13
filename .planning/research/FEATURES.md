# Feature Research: Code Quality and Performance Improvements

**Domain:** Log search platform (OpsBox)
**Researched:** 2026-03-13
**Confidence:** HIGH

## Feature Landscape

### Table Stakes (Users Expect These)

These are non-negotiable. If users hit panics or see corrupted results, the platform feels broken.

| Improvement | Why Expected | Complexity | Notes |
|-------------|--------------|------------|-------|
| **Unwrap cleanup in search path** | A `.unwrap()` panic in `search_executor.rs` (175 instances) kills the entire search. Users expect searches to return errors, not crash. | MEDIUM | Focus on hot path: search_executor.rs (175), search.rs (82). Replace with `?` propagation or `context()` from anyhow/thiserror. Pattern is mechanical but file size (2942 + 2152 lines) makes it laborious. |
| **Mutex poisoning recovery** | `agent/src/routes.rs:108,137` uses `.lock().unwrap()` in HTTP handlers. A single panic poisons the mutex, DoSing all subsequent requests. Users expect the agent endpoint to stay available. | LOW | Replace with `.lock().unwrap_or_else(|e| e.into_inner())` to recover from poisoned mutexes. 20 total `.lock().unwrap()` in production code across 8 files. |
| **Stub test removal or implementation** | 5 of 6 boundary tests are stubs (`boundary_integration.rs`). Users (and maintainers) expect tests that actually assert behavior. False coverage is worse than no coverage. | MEDIUM | Either implement real assertions for security boundary tests (path traversal, encoding attacks, concurrency) or delete stubs and track gaps explicitly. |
| **Search cancellation reliability** | Users expect to cancel a broad search without the server continuing to consume resources. The `CancellationToken` pattern exists but needs verification that all code paths check it. | LOW | Current implementation uses `tokio_util::sync::CancellationToken`. Audit that all search loops check `is_cancelled()`. |

### Differentiators (Competitive Advantage)

These improvements align with the core value: fast, reliable search. They distinguish OpsBox from tools that "work but are slow" or "work but crash under load."

| Improvement | Value Proposition | Complexity | Notes |
|-------------|-------------------|------------|-------|
| **Clone reduction in search path** | 25 clones in `search_executor.rs` and 10 in `search.rs`. Each clone copies data on the hot path. Reducing clones directly reduces per-search latency. | MEDIUM | Use `Arc` for shared immutable data (Query spec, config). Profile with `cargo-flamegraph` to identify the top 10 most expensive clones. Many clones are `String` copies that could be `Arc<str>` or `Cow<str>`. |
| **File size: refactor search_executor.rs** | At 2942 lines with 175 unwraps, this file is the primary source of bugs and the hardest to review. Splitting it enables parallel work and reduces change risk. | HIGH | Break into: SearchOrchestrator (top-level flow), ResourcePlanner (Starlark integration), SearchCoordinator (per-resource execution), ResultAggregator (caching/streaming). Each module stays under 500 lines. |
| **File size: refactor search.rs** | At 2152 lines with 82 unwraps, tightly couples encoding detection, grep searcher, and result formatting. | HIGH | Extract: EncodingHandler (detection + decoding), GrepSearcher (mmap/streaming search), LineProcessor (context lines, formatting). SearchProcessor struct already exists as a starting point. |
| **S3 client cache concurrency** | Global `Mutex<HashMap>` for S3 client cache (`storage/s3.rs:54`) serializes all S3 access. Under multi-search load, this becomes a bottleneck. | LOW | Replace with `DashMap` or `tokio::sync::RwLock`. Straightforward change, high impact for concurrent S3 searches. |
| **Large file indexing** | Files >25KB are streamed from archives but re-read entirely on each search. An offset index or content fingerprint would make repeat searches dramatically faster. | HIGH | Track file hashes + sizes in SQLite cache. Skip re-searching unchanged files. For changed files, use incremental search. Significant design work needed. |

### Anti-Features (Commonly Requested, Often Problematic)

Things that seem like good ideas but create more problems than they solve.

| Improvement | Why Requested | Why Problematic | Alternative |
|-------------|---------------|-----------------|-------------|
| **Aggressive parallel unwrapping** | "Fix all 175 unwraps at once" feels like thorough cleanup | Mechanical replacement risks introducing new bugs in untested paths. search_executor.rs has stub tests. | Fix in layers: (1) search path first (highest risk), (2) HTTP handlers second (user-facing), (3) daemon/startup code last (rarely hits). Each layer gets its own PR with test verification. |
| **Frontend coverage to 70% in one push** | 14.85% to 70% feels like a clear goal | Route components (explorer/+page.svelte at 1104 lines, view/+page.svelte at 757 lines) are monolithic and hard to unit test. Forcing coverage creates brittle snapshot tests. | Focus coverage on: (1) API clients and composables (already 80%+), (2) type safety (`as any` elimination), (3) new features get tests. Accept that route component coverage improves incrementally as they get decomposed. |
| **Replacing SQLite with PostgreSQL** | SQLite single-writer limits concurrent search metadata writes | Migration is high-risk, high-effort. Current load (single-user/small team) does not justify it. SQLite WAL mode handles concurrent reads fine. | Implement write batching for search metadata. Add connection pool tuning. Only migrate if write contention is observed in production. |
| **Full async tar library consolidation now** | Two competing libraries (`async-tar` and `tokio-tar`) is messy | Both work. Consolidation risks breaking archive handling for marginal benefit. Archive code has 80 unwraps (mostly in tests). | Pin versions, add integration tests for archive edge cases, then consolidate when touching archive code for other reasons. |
| **Adding .env file support** | "Makes local development easier" | Adds dependency, diverges from production config model (CLI + env vars + DB), creates risk of committing secrets. | Document the existing env vars clearly. The current model (CLI > env > DB > defaults) is clean and explicit. |

## Feature Dependencies

```
[Unwrap cleanup in search path]
    └──requires──> [Stub test implementation]
                      (need real tests before touching 175 unwraps)

[File size: refactor search_executor.rs]
    └──requires──> [Unwrap cleanup in search path]
                      (unwrap cleanup is step 1 of refactor)
    └──requires──> [Stub test implementation]
                      (need coverage before splitting 2942-line file)

[Clone reduction in search path]
    └──requires──> [File size: refactor search_executor.rs]
                      (easier to profile clones after refactor)

[Mutex poisoning recovery]
    └──enhances──> [Unwrap cleanup in search path]
                      (same pattern, different files)

[Search cancellation reliability]
    └──enhances──> [Unwrap cleanup in search path]
                      (cancel checks often near unwrap calls)

[Large file indexing]
    └──requires──> [File size: refactor search_executor.rs]
                      (indexing logic goes in new SearchCoordinator)
```

## MVP Definition

### Phase 1: Stop the Bleeding (Must-Do First)

These address the highest-risk production issues. A panic in the search path or a poisoned mutex in the agent endpoint are incidents waiting to happen.

- [ ] **Mutex poisoning recovery** (LOW complexity, HIGH impact) -- 20 `.lock().unwrap()` calls across 8 files. Mechanical replacement with `.unwrap_or_else(|e| e.into_inner())`. Do this first because it is low-risk and high-safety.
- [ ] **Unwrap cleanup: search path layer 1** (MEDIUM complexity) -- The 257 unwraps in search_executor.rs + search.rs. Start with the most-called functions: `SearchResultHandler::cache_and_send`, `SearchProcessor` methods. Replace with `?` or `.context()`.
- [ ] **Boundary test implementation** (MEDIUM complexity) -- The 5 stub tests in `boundary_integration.rs` cover real attack surfaces (path traversal, encoding attacks). Implement before modifying search code.

### Phase 2: Structural Improvement

After the panic risk is addressed, make the code maintainable.

- [ ] **Refactor search_executor.rs** (HIGH complexity) -- Break 2942 lines into 4 modules. This unblocks parallel development and makes future search changes safer.
- [ ] **Refactor search.rs** (HIGH complexity) -- Break 2152 lines into 3 modules.
- [ ] **S3 client cache: DashMap** (LOW complexity) -- Drop-in replacement for `Mutex<HashMap>`. Quick win for concurrent S3 search performance.

### Phase 3: Performance Optimization

After the code is safe and maintainable, optimize.

- [ ] **Clone profiling and reduction** (MEDIUM complexity) -- Use `cargo-flamegraph` to find the most expensive clones. Replace top offenders with `Arc<str>` or `Cow<str>`.
- [ ] **Search cancellation audit** (LOW complexity) -- Verify all search loops check `CancellationToken`.
- [ ] **Type safety: eliminate `as any`** (LOW complexity) -- 3 occurrences in explorer/+page.svelte. Define proper interface for virtual scroll component props.

### Future Consideration (Defer)

- [ ] Large file indexing -- Significant design work. Defer until profiling shows repeat-search-on-same-file is a real bottleneck.
- [ ] SQLite write batching -- Only needed if write contention observed.
- [ ] Frontend coverage 70% -- Incremental goal, not a sprint target.
- [ ] Tar library consolidation -- Do when touching archive code anyway.

## Feature Prioritization Matrix

| Improvement | User Value | Implementation Cost | Priority |
|-------------|------------|---------------------|----------|
| Mutex poisoning recovery | HIGH | LOW | P1 |
| Unwrap cleanup: search path | HIGH | MEDIUM | P1 |
| Boundary test implementation | HIGH | MEDIUM | P1 |
| Refactor search_executor.rs | MEDIUM | HIGH | P2 |
| Refactor search.rs | MEDIUM | HIGH | P2 |
| S3 client cache: DashMap | MEDIUM | LOW | P2 |
| Clone profiling/reduction | MEDIUM | MEDIUM | P2 |
| Search cancellation audit | MEDIUM | LOW | P2 |
| Type safety: as any | LOW | LOW | P2 |
| Large file indexing | HIGH | HIGH | P3 |
| Frontend coverage 70% | LOW | HIGH | P3 |
| Tar library consolidation | LOW | MEDIUM | P3 |

**Priority key:**
- P1: Must have -- production risk, do first
- P2: Should have -- structural improvement, do after P1
- P3: Nice to have -- defer until P1/P2 done

## Sources

- `.planning/codebase/CONCERNS.md` -- Codebase audit (2026-03-13)
- `backend/logseek/src/service/search_executor.rs` -- 2942 lines, 175 unwraps, 25 clones
- `backend/logseek/src/service/search.rs` -- 2152 lines, 82 unwraps, 10 clones
- `backend/opsbox-core/src/storage/s3.rs` -- Global `Mutex<HashMap>` S3 client cache
- `backend/agent/src/routes.rs` -- `.lock().unwrap()` in HTTP handlers
- `backend/logseek/tests/boundary_integration.rs` -- 5 of 6 tests are stubs

---
*Feature research for: OpsBox code quality and performance improvements*
*Researched: 2026-03-13*
