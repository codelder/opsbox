# Project Research Summary

**Project:** OpsBox 平台改进 (Platform Quality Improvement)
**Domain:** Rust code quality, SvelteKit frontend testing, search performance optimization
**Researched:** 2026-03-13
**Confidence:** MEDIUM

## Executive Summary

OpsBox is a modular log search and analysis platform built on Rust (Tokio async) and SvelteKit (Svelte 5 Runes), using SQLite for persistence and a DFS subsystem for unified access across Local/S3/Agent endpoints. The platform works but has accumulated significant technical debt that threatens production stability: 175 `.unwrap()` calls in the core search path can panic and crash searches, 20 `.lock().unwrap()` calls in HTTP handlers can cause mutex poisoning cascades that DoS endpoints, and the frontend has only 14.85% test coverage despite a 70% target.

The research unanimously recommends a three-phase approach: (1) "stop the bleeding" by cleaning up unwrap calls and mutex poisoning risks, (2) restructure large files by extracting inline tests and focused modules, and (3) optimize search performance through profiling-guided changes. A critical architectural finding is that the two largest files (`search_executor.rs` at 2942 lines and `search.rs` at 2152 lines) are mostly inline tests -- the actual production code is only ~383 and ~861 lines respectively. This means the refactoring effort is significantly smaller than the line counts suggest, but requires test extraction as a prerequisite before any other changes.

The key risk is over-correction: replacing `unwrap` with `expect` (still panics), or replacing errors with silent swallowing (data loss). The research identifies clear patterns to avoid these traps and provides a concrete priority matrix to guide execution.

## Key Findings

### Recommended Stack

**Core technologies:**
- `clippy::unwrap_used` lint: Enforce at workspace level, start as warn then promote to deny
- `thiserror` 2.x: Already in use for typed error derivation in library crates
- `dashmap` 6.x: Replace `Mutex<HashMap>` for S3 client cache, lock-free concurrent reads
- `vitest-browser-svelte`: Required for Svelte 5 component testing (vitest 3.2 already configured)
- `cargo-flamegraph`: Must use before any performance optimization, identify real bottlenecks
- `arc-swap` 1.x: For read-heavy shared state, cheaper than RwLock

**What NOT to use:**
- `anyhow` in library crates (hides error types) -- binary crates only
- `@testing-library/svelte` (does not support Svelte 5 Runes)
- `async-tar` AND `tokio-tar` simultaneously (consolidate to `tokio-tar` when touching archive code)

### Expected Features

**Must have (table stakes -- production risk):**
- Unwrap cleanup in search path (175 in search_executor.rs, 82 in search.rs) -- panics crash searches
- Mutex poisoning recovery (20 `.lock().unwrap()` across 8 files) -- poisoning cascades cause endpoint DoS
- Stub test implementation (5 of 6 boundary tests are stubs) -- false coverage worse than no coverage

**Should have (competitive advantage):**
- File refactoring: search_executor.rs and search.rs (revealed to be mostly inline tests, not production code)
- Clone reduction in search path (25 in search_executor.rs, 10 in search.rs)
- S3 client cache concurrency (global `Mutex<HashMap>` serializes all S3 access)

**Defer to v2+:**
- Large file indexing (needs profiling data to justify)
- SQLite write batching (only if write contention observed)
- Frontend coverage to 70% (incremental, not a sprint target)
- Tar library consolidation (do when touching archive code anyway)

### Architecture Approach

**Key finding:** The two "large files" are 60-87% inline tests. The real refactoring task is test extraction, not logic decomposition.

**Target structure after refactoring:**
1. `search_executor.rs` (~383 lines, tests extracted)
2. `query_qualifiers.rs` (~70 lines, new module)
3. `result_handler.rs` (~130 lines, new module)
4. `search.rs` (~400 lines, tests extracted)
5. `grep_search.rs` (~250 lines, new module)

**Performance optimization patterns:**
- DashMap for S3 client cache (lock-free reads)
- `Arc<str>` for shared immutable strings (cheaper clones)
- Query compilation cache with LRU eviction
- SQLite write batching with transaction grouping

### Critical Pitfalls

1. **Replace unwrap with expect (still panics)** -- Categorize each unwrap before touching: infallible (use `expect` with comment), error-path (use `?`), or has-default (use `unwrap_or`). Mechanical replacement is the #1 failure mode.
2. **Mutex poisoning cascades in HTTP handlers** -- `.lock().unwrap()` in request handlers poisons the mutex permanently after one panic. Use `.lock().unwrap_or_else(|e| e.into_inner())` or `tokio::sync::Mutex`.
3. **Silent error swallowing (over-correction)** -- Using `.unwrap_or_default()` on `Result` types or `let _ =` to suppress errors causes silent data loss in a log search platform. Errors affecting data integrity must propagate to users.
4. **Profile before optimizing** -- The 303 `.clone()` calls may not be bottlenecks. Use `cargo-flamegraph` first. Optimizing without profiling wastes effort and introduces complexity.
5. **Testing components instead of behavior** -- Frontend coverage push leads to brittle snapshot tests. Define user stories first, use `getByRole`/`getByText` queries, avoid `querySelector`.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Stop the Bleeding (Unwrap + Mutex Cleanup)
**Rationale:** Production stability risk. 175 unwraps in search path and 20 poisoned mutex risks in HTTP handlers are incidents waiting to happen. Must address before any other work.
**Delivers:** Zero `.unwrap()` in non-test production code, zero `.lock().unwrap()` in HTTP handlers, all boundary tests implemented with real assertions
**Addresses:** Mutex poisoning recovery, unwrap cleanup in search path, boundary test implementation
**Avoids:** Expect-as-replacement pitfall (categorize before replacing), silent error swallowing (data integrity errors must propagate)
**Research needed:** No -- patterns are well-documented, mechanical replacement with proper categorization

### Phase 2: Structural Improvement (Test Extraction + Module Refactoring)
**Rationale:** Depends on Phase 1 (unwrap cleanup is step 1 of refactor). Extract inline tests first (zero functional risk), then extract focused modules.
**Delivers:** search_executor.rs reduced from 2942 to ~383 lines, search.rs reduced from 2152 to ~861 lines, 3 new focused modules (query_qualifiers, result_handler, grep_search), S3 client cache migrated to DashMap
**Uses:** `dashmap` 6.x, standard Rust module extraction patterns
**Implements:** Test extraction, query qualifiers module, result handler module, grep search module
**Research needed:** Minimal -- test extraction is standard Rust pattern, module boundaries already clear

### Phase 3: Performance Optimization (Profile-Guided)
**Rationale:** Depends on Phase 2 (easier to profile clones after refactor). Must profile first, optimize second.
**Delivers:** Reduced clone overhead (target <50 from ~100), query compilation cache, verified cancellation coverage, type safety improvements (eliminate `as any`)
**Avoids:** Profile-before-optimize pitfall (PRs must include flamegraph), async runtime blocking (use `spawn_blocking` for grep-searcher), SQLite write serialization (batch writes)
**Research needed:** Yes -- need to profile actual search workloads with `cargo-flamegraph` to identify real bottlenecks before optimization

### Phase 4: Frontend Coverage (Incremental)
**Rationale:** Lower priority than backend stability. Route components (1104 lines, 757 lines) need decomposition before they are testable. Incremental goal, not a sprint.
**Delivers:** Composable and API client coverage increase, type safety improvements, behavioral tests for decomposed route components
**Avoids:** Testing implementation not behavior pitfall, dual test environment confusion (tag tests for server/client projects explicitly)
**Research needed:** Yes -- testing patterns for Svelte 5 Runes with vitest-browser-svelte need validation

### Phase Ordering Rationale

- Phase 1 before Phase 2 because unwrap cleanup is a prerequisite step of file refactoring
- Phase 2 before Phase 3 because profiling is more effective on cleanly structured code
- Phase 4 is parallel to Phase 2-3 but lower priority (frontend does not have production stability risk)
- Each phase should be its own PR or series of PRs with test verification between phases

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 3:** Needs `cargo-flamegraph` profiling on realistic workloads to identify actual bottlenecks before optimization
- **Phase 4:** Svelte 5 + vitest-browser-svelte testing patterns need validation; route component decomposition strategy needs design

Phases with standard patterns (skip research-phase):
- **Phase 1:** Unwrap cleanup patterns are well-documented; categorize-then-replace is mechanical
- **Phase 2:** Test extraction is standard Rust refactoring; module extraction boundaries already clear

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | MEDIUM | Based on CLAUDE.md analysis and codebase audit; WebSearch unavailable, training data through early 2025 |
| Features | HIGH | Direct codebase analysis confirms all findings; priority matrix based on concrete code metrics |
| Architecture | HIGH | Direct file analysis reveals inline test inflation; refactoring boundaries clearly visible in code |
| Pitfalls | HIGH | Based on CONCERNS.md codebase audit and established Rust/Svelte patterns |

**Overall confidence:** MEDIUM-HIGH

The feature and architecture findings are high confidence (direct code analysis). Stack recommendations and some performance estimates are medium confidence (would benefit from WebSearch verification of latest crate versions and benchmarks).

### Gaps to Address

- **Frontend coverage strategy:** The 14.85% to 70% gap is large. Research suggests incremental approach, but a concrete plan for which components to decompose and test first needs to be developed during planning.
- **SQLite write contention:** Whether this is actually a bottleneck depends on usage patterns. Needs real-world measurement before investing in write batching.
- **Performance baseline:** No current benchmarks exist. Need to establish baseline metrics before Phase 3 can set concrete targets.
- **Svelte 5 testing patterns:** vitest-browser-svelte support for Svelte 5 Runes is recent. Need to verify current API compatibility during Phase 4 planning.

## Sources

### Primary (HIGH confidence)
- `.planning/codebase/CONCERNS.md` -- Codebase audit with exact unwrap counts, mutex patterns, test gaps
- Direct codebase analysis of `backend/logseek/src/service/search_executor.rs` (2942 lines, 175 unwraps)
- Direct codebase analysis of `backend/logseek/src/service/search.rs` (2152 lines, 82 unwraps)
- `CLAUDE.md` -- Project architecture, test infrastructure, API endpoints

### Secondary (MEDIUM confidence)
- STACK.md -- Technology recommendations (clippy, dashmap, vitest-browser-svelte)
- FEATURES.md -- Feature prioritization and dependency analysis
- PITFALLS.md -- Domain pitfalls from Rust/Svelte best practices

### Tertiary (LOW confidence)
- Performance improvement estimates (10-20%, 30%, 50-90%) -- based on general Rust patterns, need benchmarking to verify
- DashMap concurrent access patterns -- verify API compatibility with dashmap 6.x
- vitest-browser-svelte Svelte 5 support -- verify current API with latest version

---
*Research completed: 2026-03-13*
*Ready for roadmap: yes*
