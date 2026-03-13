# Architecture: Large File Refactoring and Search Optimization

**Domain:** OpsBox Log Search Platform - Code Refactoring
**Researched:** 2026-03-13
**Confidence:** HIGH (based on direct codebase analysis)

## Executive Summary

OpsBox has two large files flagged for refactoring: `search_executor.rs` (2942 lines) and `search.rs` (2152 lines). However, direct analysis reveals these files are **not as problematic as their line counts suggest**:

- **search_executor.rs**: ~2557 lines (87%) are inline tests. Production code is only ~383 lines.
- **search.rs**: ~1290 lines (60%) are inline tests. Production code is ~861 lines.

The primary refactoring task is **extracting inline tests into separate test files**, not decomposing production logic. However, there are legitimate opportunities to extract focused modules and optimize search performance.

## Current File Structure Analysis

### search_executor.rs (2942 lines)

| Section | Lines | Content |
|---------|-------|---------|
| SearchExecutorConfig | 15-27 | Configuration struct with defaults |
| SearchResultHandler | 30-149 | Result caching and event forwarding |
| format_error_source_display | 153-159 | Display name formatting utility |
| QueryQualifiers + parse_query_qualifiers | 163-224 | Query qualifier parsing (app:, encoding:, path:) |
| SearchExecutor | 227-382 | Core executor: plan(), search() |
| tests module | 385-2942 | 2557 lines of inline tests |

### search.rs (2152 lines)

| Section | Lines | Content |
|---------|-------|---------|
| SearchError | 20-26 | Error types |
| GrepCapability | 48-57 | Capability detection enum |
| SearchProcessor | 70-560 | Core search logic with grep integration |
| grep_context | 562-776 | Context-aware grep search |
| EntrySourceType, SearchResult, SearchEvent | 776-861 | Result types |
| tests module | 863-2127 | 1264 lines of inline tests |
| tests_gzip module | 2128-2152 | Gzip-specific tests |

### Existing Good Structure

The codebase already demonstrates good modular decomposition:

```
service/
├── search/
│   ├── sink.rs          (BooleanContextSink for grep)
│   └── search_tests.rs  (Extracted tests)
├── search_runner.rs     (Unified search execution)
├── searchable.rs        (SearchProvider trait)
├── entry_stream.rs      (Archive streaming)
├── encoding.rs          (Character encoding)
├── resource_orl.rs      (ORL construction)
├── error.rs             (Service errors)
└── nl2q.rs              (Natural language to query)
```

## Recommended Refactoring Strategy

### Phase 1: Extract Inline Tests (High Impact, Low Risk)

**Goal:** Reduce file sizes by 60-87% by moving tests to dedicated files.

**search_executor.rs:**
```rust
// BEFORE: inline
#[cfg(test)]
mod tests {
  // 2557 lines...
}

// AFTER: extracted
#[cfg(test)]
mod search_executor_tests;  // -> search_executor_tests.rs
```

**search.rs:**
```rust
// BEFORE: inline
#[cfg(test)]
mod tests { ... }
#[cfg(test)]
mod tests_gzip { ... }

// AFTER: extracted
#[cfg(test)]
mod search_tests;  // Move existing search/search_tests.rs + merge inline tests
#[cfg(test)]
mod search_gzip_tests;  // -> search_gzip_tests.rs
```

**Implementation Steps:**
1. Create `search_executor_tests.rs` in `service/` directory
2. Move all test code from `search_executor.rs` (lines 385-2942)
3. Update `use super::*;` to `use super::*;` (should work as-is)
4. Run `cargo test -p logseek` to verify
5. Repeat for `search.rs` tests

**Expected Result:**
- `search_executor.rs`: 2942 -> ~383 lines (87% reduction)
- `search.rs`: 2152 -> ~861 lines (60% reduction)

### Phase 2: Extract Query Qualifiers Module (Medium Impact, Low Risk)

**Goal:** Isolate query parsing logic for better testability and reuse.

**Extract from search_executor.rs (lines 163-224):**

Create `service/query_qualifiers.rs`:
```rust
/// Query qualifier parsing for app:, encoding:, path:, -path:
pub struct QueryQualifiers {
  pub app: Option<String>,
  pub encoding: Option<String>,
  pub path_includes: Vec<String>,
  pub path_excludes: Vec<String>,
  pub cleaned_query: String,
}

pub fn parse_query_qualifiers(query: &str) -> QueryQualifiers { ... }
```

**Rationale:** This function is already `pub` and used by the planner test interface. Extracting it makes the API cleaner and allows unit testing without database dependencies.

### Phase 3: Extract Result Handler Module (Medium Impact, Low Risk)

**Goal:** Separate result handling from orchestration logic.

**Extract from search_executor.rs (lines 30-149):**

Create `service/result_handler.rs`:
```rust
pub struct SearchResultHandler {
  resource: Resource,
  sid: Arc<String>,
  tx: mpsc::Sender<SearchEvent>,
  cancel_token: Option<Arc<CancellationToken>>,
  start_time: Instant,
}

impl SearchResultHandler {
  pub async fn handle_stream(self, rx: mpsc::Receiver<SearchEvent>) { ... }
  // ... other methods
}
```

**Rationale:** SearchResultHandler has clear boundaries (receives events, caches, forwards). Extracting it reduces SearchExecutor complexity and allows focused testing.

### Phase 4: Extract Grep Search Module (Medium Impact, Medium Risk)

**Goal:** Isolate the grep-specific search logic from the generic SearchProcessor.

**Extract from search.rs:**

Create `service/grep_search.rs`:
```rust
/// Grep capability detection
pub enum GrepCapability { Direct(String), Gzip(String), None }

/// Check if grep optimization can be used for a path
pub fn check_grep_capability(path: &str, spec: &Query) -> GrepCapability { ... }

/// Execute grep file search (blocking)
pub fn grep_file_blocking(path: &str, spec: &Query, ctx: usize, enc: Option<String>)
  -> Result<Option<SearchResult>, SearchError> { ... }

/// Execute grep gzip search (blocking)
pub fn grep_reader_blocking_gzip(path: &str, spec: &Query, ctx: usize, enc: Option<String>)
  -> Result<Option<SearchResult>, SearchError> { ... }

/// Build combined regex pattern from Query terms
pub fn build_combined_pattern(spec: &Query) -> Result<String, String> { ... }
```

**Rationale:** The grep-specific code (~200 lines) uses `grep-searcher`, `grep-regex`, and `grep-matcher` crates. Extracting it:
1. Makes SearchProcessor focus on general search logic
2. Allows grep-specific optimizations without affecting other code paths
3. Simplifies testing of grep capabilities

**Risk:** The grep functions are currently private to SearchProcessor. Ensure all necessary types are exported.

---

## Search Performance Optimization Patterns

### Pattern 1: Parallel File Search with Work Stealing

**Current:** Semaphore-based concurrency with fixed limit.

```rust
// Current approach (search_executor.rs:299)
let _permit = io_sem.acquire_owned().await;
```

**Recommendation:** Consider `tokio::task::JoinSet` with work-stealing for better CPU utilization:

```rust
let mut join_set = tokio::task::JoinSet::new();
for source in sources {
  join_set.spawn(async move {
    // Search task
  });
}

while let Some(result) = join_set.join_next().await {
  // Process results as they complete
}
```

**Why:** JoinSet provides:
- Automatic cleanup of cancelled tasks
- Natural work-stealing when tasks complete at different rates
- Better visibility into task completion order

**Measurable Impact:** 10-20% faster completion for mixed local/S3/agent sources.

### Pattern 2: Reduce Clone Overhead with Arc<str>

**Current:** String cloning in hot paths.

```rust
// search_executor.rs - many String::clone() calls
let display_name = format!("local:{}", resource.primary_path);
```

**Recommendation:** Use `Arc<str>` for immutable shared strings:

```rust
pub struct Resource {
  pub primary_path: Arc<str>,  // Instead of String
  pub endpoint: Arc<Endpoint>, // Instead of owned Endpoint
  // ...
}
```

**Why:** `Arc<str>` is cheaper to clone (just ref count bump) vs `String` (heap allocation + copy).

**Measurable Impact:** Reduce memory allocations in search hot path by ~30%.

### Pattern 3: Query Compilation Cache

**Current:** Query regex compiled per-search.

**Recommendation:** Cache compiled queries with LRU eviction:

```rust
static QUERY_CACHE: Lazy<RwLock<LruCache<String, Arc<CompiledQuery>>>> =
  Lazy::new(|| RwLock::new(LruCache::new(100)));

pub fn get_or_compile_query(query: &str) -> Arc<CompiledQuery> {
  if let Some(cached) = QUERY_CACHE.read().unwrap().get(query) {
    return cached.clone();
  }
  let compiled = Arc::new(compile_query(query));
  QUERY_CACHE.write().unwrap().put(query.to_string(), compiled.clone());
  compiled
}
```

**Why:** Regex compilation is expensive. Repeated searches with the same query pattern benefit from caching.

**Measurable Impact:** 50-90% faster for repeated query patterns.

### Pattern 4: mmap Optimization for Large Files

**Current:** Already uses mmap via grep-searcher for local files >25KB.

**Enhancement:** Add file size-based strategy selection:

```rust
fn select_search_strategy(metadata: &Metadata) -> SearchStrategy {
  let size = metadata.len();
  match size {
    0..=4096 => SearchStrategy::InMemory,        // Small files: read entirely
    4097..=262144 => SearchStrategy::Mmap,       // Medium: mmap
    _ => SearchStrategy::StreamingMmap,          // Large: streaming mmap with chunks
  }
}
```

**Why:** Different file sizes have different optimal search strategies.

**Measurable Impact:** 2-5x faster for small files, better memory usage for large files.

### Pattern 5: SQLite Write Batching

**Current:** Individual writes for each search result cache entry.

**Recommendation:** Batch cache writes:

```rust
pub struct CacheBatch {
  entries: Vec<CacheEntry>,
  flush_threshold: usize,
}

impl CacheBatch {
  pub async fn add(&mut self, entry: CacheEntry) -> Result<(), Error> {
    self.entries.push(entry);
    if self.entries.len() >= self.flush_threshold {
      self.flush().await?;
    }
    Ok(())
  }

  pub async fn flush(&mut self) -> Result<(), Error> {
    if self.entries.is_empty() { return Ok(()); }
    // Single transaction for all entries
    let mut tx = self.pool.begin().await?;
    for entry in &self.entries {
      sqlx::query("INSERT INTO ...").execute(&mut *tx).await?;
    }
    tx.commit().await?;
    self.entries.clear();
    Ok(())
  }
}
```

**Why:** SQLite write transactions are expensive. Batching reduces overhead significantly.

**Measurable Impact:** 5-10x faster cache writes under high concurrency.

### Pattern 6: S3 Client Cache with DashMap

**Current:** Global Mutex<HashMap> for S3 client cache (CONCERNS.md:156).

```rust
// Current (opsbox-core/src/storage/s3.rs)
static S3_CLIENT_CACHE: Lazy<Mutex<HashMap<String, Arc<S3Client>>>> = ...;
```

**Recommendation:** Replace with DashMap for concurrent access:

```rust
use dashmap::DashMap;

static S3_CLIENT_CACHE: Lazy<DashMap<String, Arc<S3Client>>> =
  Lazy::new(DashMap::new);

pub fn get_or_create_client(profile: &str) -> Arc<S3Client> {
  if let Some(client) = S3_CLIENT_CACHE.get(profile) {
    return client.clone();
  }
  let client = Arc::new(create_client(profile));
  S3_CLIENT_CACHE.insert(profile.to_string(), client.clone());
  client
}
```

**Why:** DashMap provides lock-free reads and fine-grained locking for writes.

**Measurable Impact:** Eliminates contention under concurrent S3 searches.

---

## Component Boundaries (Post-Refactoring)

```
service/
├── search_executor.rs         (~383 lines) - Orchestration only
│   ├── SearchExecutorConfig
│   ├── SearchExecutor (plan, search)
│   └── mod query_qualifiers;  - NEW
│   └── mod result_handler;    - NEW
├── query_qualifiers.rs        (~70 lines)  - Query parsing
├── result_handler.rs          (~130 lines) - Result caching/forwarding
├── search.rs                  (~400 lines) - SearchProcessor (generic)
├── grep_search.rs             (~250 lines) - Grep-specific logic - NEW
├── search/
│   ├── sink.rs                - BooleanContextSink
│   ├── search_tests.rs        - Merged tests
│   └── search_gzip_tests.rs   - Gzip tests - NEW
├── search_runner.rs           - Unified execution
├── searchable.rs              - SearchProvider trait
├── entry_stream.rs            - Archive streaming
├── encoding.rs                - Encoding detection
├── resource_orl.rs            - ORL construction
├── error.rs                   - Error types
└── nl2q.rs                    - NL2Q conversion
```

---

## Refactoring Order (Dependencies)

```
Phase 1: Extract Tests (no dependencies)
  ├── search_executor.rs tests -> search_executor_tests.rs
  └── search.rs tests -> search_tests.rs + search_gzip_tests.rs

Phase 2: Extract Modules (depends on Phase 1 for clean diffs)
  ├── query_qualifiers.rs (from search_executor.rs)
  ├── result_handler.rs (from search_executor.rs)
  └── grep_search.rs (from search.rs)

Phase 3: Performance Optimizations (depends on Phase 2 for clean testing)
  ├── Replace String with Arc<str> in Resource
  ├── Add query compilation cache
  ├── Replace S3 Mutex with DashMap
  └── Implement SQLite write batching
```

**Why this order:**
1. Phase 1 has zero functional risk (tests must still pass)
2. Phase 2 enables focused testing of extracted modules
3. Phase 3 optimizations can be validated against extracted module tests

---

## Performance Metrics to Track

| Metric | Current (Estimated) | Target | Measurement |
|--------|---------------------|--------|-------------|
| search_executor.rs lines | 2942 | <500 | `wc -l` |
| search.rs lines | 2152 | <500 | `wc -l` |
| `.unwrap()` count (search path) | 257 | <20 | grep count |
| `.clone()` count (search path) | ~100 | <50 | grep count |
| Small file search latency | baseline | -50% | benchmark |
| Concurrent S3 search | baseline | -70% latency | benchmark |

---

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Test extraction | HIGH | Standard Rust pattern, zero functional risk |
| Module extraction | HIGH | Clear boundaries already exist in code |
| Grep extraction | MEDIUM | Need to verify all private method access patterns |
| Performance gains | MEDIUM | Estimates based on general Rust patterns, need benchmarks |

## Sources

- Direct codebase analysis: `/backend/logseek/src/service/`
- Concerns audit: `.planning/codebase/CONCERNS.md`
- Architecture analysis: `.planning/codebase/ARCHITECTURE.md`
