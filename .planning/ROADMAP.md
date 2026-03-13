# Roadmap: OpsBox Platform Quality Improvement

## Overview

OpsBox is a modular log search platform built on Rust and SvelteKit. The platform works but has accumulated technical debt: 175 `.unwrap()` calls in the core search path can panic and crash searches, 20 `.lock().unwrap()` calls in HTTP handlers can cause mutex poisoning cascades, and the frontend has only 14.85% test coverage. This roadmap systematically addresses production stability, structural quality, performance, and frontend coverage across 4 phases.

## Phases

- [ ] **Phase 1: Production Stability (止血)** - Eliminate panic points in search path, fix mutex poisoning, implement stub tests
- [ ] **Phase 2: Structural Improvement (结构改进)** - Extract inline tests from oversized files, migrate S3 cache to DashMap
- [ ] **Phase 3: Performance Optimization (性能优化)** - Profile-guided optimization: reduce clones, implement query caching, SQLite write batching
- [ ] **Phase 4: Frontend Coverage (前端覆盖)** - Decompose large components, increase test coverage to 70%, eliminate type safety gaps

## Phase Details

### Phase 1: Production Stability (止血)
**Goal**: Eliminate panic points that can crash searches and cause DoS via mutex poisoning
**Depends on**: Nothing (first phase)
**Requirements**: SAFE-01, SAFE-02, SAFE-03, SAFE-04
**Success Criteria** (what must be TRUE):
  1. Search path (search_executor.rs, search.rs) executes with zero `.unwrap()` in production code paths -- searches never panic
  2. HTTP handler mutex operations recover from poisoning instead of cascading failures -- agent and logseek endpoints stay available after a panic
  3. Boundary integration tests (boundary_integration.rs) have real assertions for encoding, path traversal, concurrency, permissions, and large files -- security gaps are actually tested
  4. S3 integration tests are either implemented with real assertions or removed and tracked as a coverage gap -- no false coverage signals
**Plans**: 4 plans

Plans:
- [ ] 01-01: Categorize and replace all `.unwrap()` in search_executor.rs and search.rs (175 + 82 occurrences)
- [ ] 01-02: Fix mutex poisoning in HTTP handlers across agent and server modules
- [ ] 01-03: Implement 5 stub tests in boundary_integration.rs with real assertions
- [ ] 01-04: Implement or remove skipped S3 API endpoint test

### Phase 2: Structural Improvement (结构改进)
**Goal**: Reduce oversized files by extracting inline tests and migrate S3 cache to lock-free concurrent access
**Depends on**: Phase 1
**Requirements**: STRC-01, STRC-02, STRC-03
**Success Criteria** (what must be TRUE):
  1. search_executor.rs is reduced from 2942 lines to approximately 383 lines of production code -- inline tests moved to dedicated test files
  2. search.rs is reduced from 2152 lines to approximately 861 lines of production code -- inline tests moved to dedicated test files
  3. S3 client cache uses DashMap instead of Mutex<HashMap> -- concurrent S3 operations no longer serialize on a global lock
**Plans**: 3 plans

Plans:
- [ ] 02-01: Extract inline tests from search_executor.rs to independent test file
- [ ] 02-02: Extract inline tests from search.rs to independent test file
- [ ] 02-03: Replace S3_CLIENT_CACHE Mutex<HashMap> with DashMap

### Phase 3: Performance Optimization (性能优化)
**Goal**: Reduce search latency through profiling-guided optimization of clones, query compilation, and database writes
**Depends on**: Phase 2
**Requirements**: PERF-01, PERF-02, PERF-03, PERF-04
**Success Criteria** (what must be TRUE):
  1. Performance baseline established with cargo-flamegraph -- real bottlenecks identified, not guessed
  2. Shared immutable strings in search path use Arc<str> instead of String clones -- clone count reduced from ~100 to <50
  3. Repeated identical queries hit a compiled cache instead of recompiling -- query compilation time eliminated for cache hits
  4. SQLite writes batch into transactions instead of individual commits -- write throughput improved
**Plans**: 4 plans

Plans:
- [ ] 03-01: Establish performance baseline with cargo-flamegraph profiling
- [ ] 03-02: Reduce clone overhead in search path (Arc<str> migration)
- [ ] 03-03: Implement query compilation cache with LRU eviction
- [ ] 03-04: Implement SQLite write batching with transaction grouping

### Phase 4: Frontend Coverage (前端覆盖)
**Goal**: Make frontend testable by decomposing large components and increase coverage from 14.85% to 70%
**Depends on**: Phase 2
**Requirements**: FE-01, FE-02, FE-03, FE-04
**Success Criteria** (what must be TRUE):
  1. Large route components (explorer/+page.svelte at 1104 lines, view/+page.svelte at 757 lines) are decomposed into sub-components with extracted composables -- each component under 300 lines
  2. API client coverage reaches 80%+ -- nl2q, planners, llm, search, view, settings, agent, explorer clients all tested
  3. Composable coverage reaches 70%+ -- search, view, explorer, agent composables have behavioral tests
  4. All `as any` type casts eliminated from frontend code -- proper TypeScript interfaces defined for component props
**Plans**: 4 plans

Plans:
- [ ] 04-01: Decompose large route components into testable sub-components
- [ ] 04-02: Expand API client test coverage to 80%+
- [ ] 04-03: Expand composable test coverage to 70%+
- [ ] 04-04: Eliminate `as any` casts with proper TypeScript interfaces

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Production Stability | 0/4 | Not started | - |
| 2. Structural Improvement | 0/3 | Not started | - |
| 3. Performance Optimization | 0/4 | Not started | - |
| 4. Frontend Coverage | 0/4 | Not started | - |
