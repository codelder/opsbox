# Refactoring Suggestions

**Document Version**: v1.1
**Last Updated**: 2026-01-01
**Status**: Planning (Not implemented)

## Overview

This document records refactoring suggestions identified during code review. These are architectural improvements that should be considered for future iterations, but are not immediately required for the current functionality.

---

## 1. Module Configuration Dependency Injection

### Current Implementation

**Problem**: Module configuration is passed via environment variables using `unsafe` blocks, which has several drawbacks:

1. **Type Safety**: Environment variables are strings, requiring parsing and error handling at multiple points
2. **Testability**: Hard to test modules in isolation without manipulating global environment state
3. **Thread Safety**: Requires `unsafe` blocks due to potential data races in multi-threaded environments
4. **Inconsistency**: Some modules read config at startup and cache it, others read it on every use
5. **Runtime Changes**: Modifying environment variables at runtime has unpredictable effects

**Current Code Locations**:
- `backend/opsbox-server/src/main.rs:293-309` - `setup_module_env_vars()` function
- `backend/opsbox-server/src/network.rs:31-75` - Proxy environment variable setup
- `backend/logseek/src/lib.rs:112-140` - Module reads from environment variables

### Proposed Solutions

#### Option 1: Configuration Trait (Recommended)

Create a configuration trait that modules can implement to receive typed configuration:

```rust
// In opsbox-core/src/module.rs
pub trait ModuleConfig: Send + Sync {
    fn from_app_config(config: &AppConfig) -> Self;
}

#[async_trait]
pub trait Module: Send + Sync {
    fn name(&self) -> &'static str;
    fn api_prefix(&self) -> &'static str;

    // Add configuration method
    fn configure(&self, config: Box<dyn ModuleConfig>) {
        // Default: no-op, modules can override
    }

    // ... existing methods
}
```

**Benefits**:
- Type-safe configuration passing
- No `unsafe` code required
- Easy to test (can pass mock configs)
- Clear dependency flow

**Drawbacks**:
- Requires defining a config type for each module
- Slightly more complex initial setup

#### Option 2: Configuration Enum

Use an enum to represent all possible module configurations:

```rust
pub enum ModuleConfig {
    LogSeek {
        io_max_concurrency: usize,
        io_timeout_sec: u64,
        io_max_retries: u32,
        server_id: Option<String>,
    },
    Explorer { /* ... */ },
    AgentManager { /* ... */ },
}
```

**Benefits**:
- Single configuration object
- Type-safe
- No `unsafe` required

**Drawbacks**:
- Requires updating enum when adding new modules
- Less flexible for modules with very different config needs

#### Option 3: Configuration Registry

Use a type-erased configuration registry:

```rust
pub struct ConfigRegistry {
    configs: HashMap<&'static str, Box<dyn Any + Send + Sync>>,
}

impl ConfigRegistry {
    pub fn get<T: 'static>(&self, key: &str) -> Option<&T> {
        self.configs.get(key)?.downcast_ref()
    }
}
```

**Benefits**:
- Flexible for different module config types
- Type-safe access
- No `unsafe` required

**Drawbacks**:
- More complex implementation
- Runtime type checking overhead

#### Option 4: Direct Object Passing

Pass configuration objects directly to module constructors:

```rust
impl LogSeekModule {
    pub fn new(config: LogSeekConfig) -> Self {
        // Store config internally
    }
}
```

**Benefits**:
- Simplest approach
- Fully type-safe
- No global state

**Drawbacks**:
- Requires changes to module registration system
- May conflict with `inventory`-based discovery

### Recommendation

**Preferred Approach**: Option 1 (Configuration Trait) combined with Option 4 (Direct Object Passing) for new modules.

**Migration Strategy**:
1. Add `ModuleConfig` trait to `opsbox-core`
2. Create typed config structs for each module (e.g., `LogSeekConfig`)
3. Update `Module::configure()` to accept typed config
4. Gradually migrate modules from environment variables to typed configs
5. Remove `setup_module_env_vars()` and related `unsafe` blocks

---

## 2. Eliminate `unsafe` Code for Environment Variables

### Current Issues

**Locations with `unsafe` blocks**:
1. `backend/opsbox-server/src/main.rs:294` - Module config environment variables
2. `backend/opsbox-server/src/network.rs:31, 41, 64` - Proxy environment variables

**Why `unsafe` is required**:
- `std::env::set_var()` and `std::env::remove_var()` are marked `unsafe` in multi-threaded contexts
- Rust considers environment variable mutation as potentially causing data races

### Proposed Solution

**For Module Configuration**: Use the configuration trait approach (Section 1) to eliminate the need for environment variables entirely.

**For Network Proxy Configuration**:
- Use `OnceLock` or `LazyLock` to cache proxy settings at startup
- Pass proxy configuration directly to HTTP clients instead of relying on environment variables
- Only read environment variables once at startup, then cache the values

```rust
use std::sync::OnceLock;

static PROXY_CONFIG: OnceLock<ProxyConfig> = OnceLock::new();

pub struct ProxyConfig {
    http_proxy: Option<String>,
    https_proxy: Option<String>,
    no_proxy: String,
}

pub fn init_network_env() {
    let config = ProxyConfig {
        http_proxy: std::env::var("HTTP_PROXY").ok(),
        https_proxy: std::env::var("HTTPS_PROXY").ok(),
        no_proxy: std::env::var("NO_PROXY")
            .unwrap_or_else(|_| "localhost,127.0.0.1,::1,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16".to_string()),
    };

    PROXY_CONFIG.set(config).expect("Proxy config already initialized");
}

pub fn get_proxy_config() -> &'static ProxyConfig {
    PROXY_CONFIG.get().expect("Proxy config not initialized")
}
```

**Benefits**:
- Eliminates all `unsafe` blocks related to environment variables
- Thread-safe configuration access
- Clear initialization order

---

## 3. Consistent Configuration Reading Patterns

### Current Inconsistency

**Problem**: Configuration is read at different times and in different ways:

1. **Startup + Cached**:
   - `LOGSEEK_IO_MAX_CONCURRENCY` - Read once in `LogSeekModule::configure()`, cached in `OnceCell`
   - `LOGSEEK_IO_TIMEOUT_SEC` - Read once, but also read at runtime as fallback
   - `LOGSEEK_IO_MAX_RETRIES` - Read once, cached

2. **Runtime Every Use**:
   - `OPSBOX_IO_TIMEOUT_SEC` / `LOGSEEK_IO_TIMEOUT_SEC` - Read every time `io_timeout()` is called in `s3.rs`
   - `ENTRY_CONCURRENCY` - Read every time `entry_concurrency()` is called

3. **Runtime Fallback**:
   - `LOGSEEK_IO_TIMEOUT_SEC` - Read at runtime if `tuning::get()` returns `None`

**Issues**:
- Runtime environment variable changes have unpredictable effects
- Some configs are cached, others aren't
- Hard to reason about when configuration changes take effect

### Proposed Solution

**Unified Pattern**: All configuration should be:
1. Read once at startup
2. Cached in a type-safe structure
3. Accessed through a consistent API

**Implementation**:
- Extend `logseek/src/utils/tuning.rs` to include all configuration values
- Remove runtime environment variable reads
- Provide a single source of truth for all module configuration

```rust
// In logseek/src/utils/tuning.rs
pub struct Tuning {
    pub server_id: Option<String>,
    pub io_max_concurrency: usize,
    pub io_timeout_sec: u64,
    pub io_max_retries: u32,
    pub entry_concurrency: usize,  // Add this
}

// All code should use tuning::get() instead of std::env::var()
```

**Migration Steps**:
1. Audit all `std::env::var()` calls in the codebase
2. Identify which should be module configuration vs. system configuration
3. Move module configuration to typed config structs
4. Update all code to use cached configuration instead of direct environment variable reads

---

## 4. Network Proxy Configuration Refactoring

### Current Implementation

**Location**: `backend/opsbox-server/src/network.rs`

**Issues**:
- Uses `unsafe` blocks for environment variable mutation
- Modifies global environment state
- `reqwest` library already reads proxy environment variables automatically

### Proposed Solution

**Option A: Direct Client Configuration** (Recommended for new code)

Configure proxy settings directly on HTTP clients instead of relying on environment variables:

```rust
pub fn create_http_client(proxy_config: &ProxyConfig) -> reqwest::Client {
    let mut builder = reqwest::Client::builder();

    if let Some(ref http_proxy) = proxy_config.http_proxy {
        builder = builder.proxy(reqwest::Proxy::http(http_proxy)?);
    }

    if let Some(ref https_proxy) = proxy_config.https_proxy {
        builder = builder.proxy(reqwest::Proxy::https(https_proxy)?);
    }

    builder.no_proxy(reqwest::Proxy::custom(move |url| {
        // Check if URL matches no_proxy patterns
        // ...
    }));

    builder.build()
}
```

**Option B: Environment Variable Standardization** (Keep current approach but improve)

If we must use environment variables (for compatibility with system proxy settings):
- Read environment variables once at startup
- Cache in `OnceLock`
- Never modify environment variables at runtime
- Document that proxy settings must be configured before application startup

**Recommendation**: Use Option A for application-controlled proxy settings, Option B only if we need to respect system-wide proxy configuration that may change.

---

## 5. Module Discovery and Dependency Management

### Current Implementation

**Observation**: `opsbox-server/src/main.rs` contains `extern crate` declarations:

```rust
#[cfg(feature = "logseek")]
extern crate logseek;

#[cfg(feature = "agent-manager")]
extern crate agent_manager;

#[cfg(feature = "explorer")]
extern crate explorer;
```

**Question**: Does this create implicit dependencies?

**Analysis**:
- `extern crate` ensures the crate is linked, which is necessary for `inventory::submit!` macros to execute
- However, `opsbox-server` doesn't directly use types from these crates
- The dependency is only for module discovery, not for code usage

**Current Status**: This is acceptable for the `inventory`-based discovery pattern, but could be improved.

### Proposed Improvement

**Option 1: Keep Current Approach**
- Document that `extern crate` is required for `inventory` discovery
- This is a known pattern with `inventory` crate

**Option 2: Explicit Module Registry**
- Create an explicit module registry that doesn't require `extern crate`
- Modules register themselves via a different mechanism
- More complex but more explicit

**Recommendation**: Keep current approach but add documentation explaining why `extern crate` is necessary.

---

## 6. Configuration Validation and Error Handling

### Current Issues

**Problem**: Configuration parsing errors are handled inconsistently:

- Some use `.unwrap_or(default)` - silently falls back to defaults
- Some use `.ok().and_then(|s| s.parse().ok())` - silently ignores parse errors
- No centralized validation or error reporting

### Proposed Solution

**Centralized Configuration Validation**:

```rust
pub struct ConfigError {
    pub key: String,
    pub value: String,
    pub reason: String,
}

pub trait ConfigValidator {
    fn validate(&self) -> Result<(), Vec<ConfigError>>;
}

impl ConfigValidator for LogSeekConfig {
    fn validate(&self) -> Result<(), Vec<ConfigError>> {
        let mut errors = Vec::new();

        if self.io_max_concurrency == 0 {
            errors.push(ConfigError {
                key: "io_max_concurrency".to_string(),
                value: self.io_max_concurrency.to_string(),
                reason: "Must be greater than 0".to_string(),
            });
        }

        // ... more validation

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
```

**Benefits**:
- Clear error messages for invalid configuration
- Fail fast at startup instead of silently using wrong values
- Easier to debug configuration issues

---

## 7. Optimize Archive File Search with grep-searcher

### Current Implementation

**Problem**: Archive file entries (from tar/tar.gz archives) that are preloaded into memory still use the slower `grep_context` async method instead of leveraging `grep-searcher`'s optimized search capabilities.

**Current Code Flow**:
1. Small archive entries (< 10MB) are preloaded into memory (`PreloadResult::Complete(content)`)
2. Data is wrapped in `Cursor::new(content)` as `AsyncRead`
3. `process_content` is called, which uses `grep_context` (line-by-line async reading)
4. `grep_context` performs regex matching line-by-line, which is slower than `grep-searcher`'s optimized search

**Current Code Locations**:
- `backend/logseek/src/service/entry_stream.rs:215-257` - Preloading and processing archive entries
- `backend/logseek/src/service/search.rs:113-155` - `process_content` method
- `backend/logseek/src/service/search.rs:675-705` - `grep_context` implementation

### Performance Analysis

**Current Method (`grep_context`)**:
- Line-by-line reading from `AsyncRead`
- Multiple system calls
- Encoding conversion overhead
- Line-by-line regex matching

**Optimized Method (`grep-searcher::search_slice`)**:
- Direct search in memory buffer
- Fewer system calls
- SIMD optimizations from `grep-searcher`
- More efficient regex engine

**Expected Performance Improvement**:

| File Size | Current Method | Optimized Method | Improvement |
|-----------|---------------|------------------|-------------|
| < 1MB     | ~10ms         | ~2ms             | **5x**      |
| 1-5MB     | ~50ms         | ~10ms            | **5x**      |
| 5-10MB    | ~100ms        | ~20ms            | **5x**      |

### Proposed Solution

#### Step 1: Add `grep_memory_blocking` Function

Create a new function specifically for searching in-memory buffers:

```rust
// In backend/logseek/src/service/search.rs

fn grep_memory_blocking(
    content: &[u8],
    spec: &Query,
    context_lines: usize,
    encoding_override: Option<String>,
) -> Result<Option<SearchResult>, String> {
    // 1. Build Regex Pattern (same as grep_file_blocking)
    let mut patterns = Vec::new();
    for term in &spec.terms {
        match term {
            crate::query::Term::Literal(s) | crate::query::Term::Phrase(s) => {
                patterns.push(regex::escape(s));
            }
            crate::query::Term::RegexStd { pattern, .. } => {
                patterns.push(pattern.clone());
            }
            _ => return Err("Unsupported term type for grep".to_string()),
        }
    }

    if patterns.is_empty() {
        return Ok(None);
    }

    let combined_pattern = patterns.join("|");
    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(true)
        .build(&combined_pattern)
        .map_err(|e| format!("Regex build failed: {}", e))?;

    // 2. Detect encoding from memory buffer
    let mut detected_encoding_label = "UTF-8".to_string();
    let sample = &content[..content.len().min(4096)];
    if let Some(enc) = detect_encoding(sample) {
        detected_encoding_label = enc.name().to_string();
    }

    if let Some(enc) = encoding_override {
        detected_encoding_label = enc;
    }

    // 3. Build Searcher (no mmap needed, data already in memory)
    let enc_res = GrepEncoding::new(&detected_encoding_label);
    let mut searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .encoding(enc_res.ok())
        .memory_map(MmapChoice::never()) // No mmap for in-memory data
        .line_number(true)
        .build();

    // 4. Search in memory buffer using search_slice
    let mut occurs = vec![false; spec.terms.len()];
    let mut matched_lines: Vec<usize> = Vec::new();
    let mut matched_count = 0;

    let mut sink = BooleanContextSink::new(
        spec,
        &mut occurs,
        &mut matched_lines,
        &mut matched_count,
        Some(&detected_encoding_label),
    );

    searcher
        .search_slice(&matcher, content, &mut sink)  // Use search_slice for memory
        .map_err(|e| e.to_string())?;

    // 5. Evaluate Boolean Logic
    let expr_match = spec.eval_file(&occurs);

    if expr_match && matched_count > 0 {
        matched_lines.sort();
        matched_lines.dedup();

        // 6. Decode content to lines (reuse existing logic)
        let encoding = Encoding::for_label(detected_encoding_label.as_bytes())
            .unwrap_or(UTF_8);
        let lines = decode_buffer_to_lines(encoding, content, "grep_memory_result ")?;

        // 7. Generate merged ranges (same as grep_file_blocking)
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        let max_idx = lines.len().saturating_sub(1);
        for idx in matched_lines {
            let s = idx.saturating_sub(context_lines);
            let e = std::cmp::min(idx + context_lines, max_idx);
            ranges.push((s, e));
        }

        // Merge overlapping ranges
        let merged = merge_ranges(ranges);

        return Ok(Some(SearchResult::new(
            String::new(), // path not available for in-memory data
            lines,
            merged,
            Some(detected_encoding_label),
        )));
    }

    Ok(None)
}
```

#### Step 2: Detect In-Memory Data in `process_content`

Modify `process_content` to detect when data is already in memory:

```rust
pub async fn process_content<R: AsyncRead + Unpin>(
    &self,
    path: String,
    reader: &mut R,
) -> Result<Option<SearchResult>, SearchError> {
    // Try to detect if reader is a Cursor (in-memory data)
    // This is a heuristic: if we can get a reference to the underlying buffer
    if let Some(content) = try_get_cursor_content(reader) {
        // Data is already in memory, use grep-searcher's search_slice
        let spec = self.spec.clone();
        let ctx = self.context_lines;
        let enc_override = self.encoding.clone();

        let handle = tokio::task::spawn_blocking(move || {
            Self::grep_memory_blocking(&content, &spec, ctx, enc_override)
        });

        match handle.await {
            Ok(Ok(Some(mut res))) => {
                // Restore path information
                res.path = path;
                return Ok(Some(res));
            }
            Ok(Ok(None)) => return Ok(None),
            Ok(Err(e)) => {
                debug!("grep memory search failed, fallback to async: {}: {}", path, e);
                // Fallback continues below
            }
            Err(e) => {
                warn!("grep memory task join failed: {}", e);
                // Fallback
            }
        }
    }

    // Original logic: try file-based grep first
    let use_grep = self.can_use_grep(&path);
    if use_grep {
        // ... existing grep_file_blocking logic
    }

    // Fallback to async grep_context
    match grep_context(reader, &self.spec, self.context_lines, self.encoding.as_deref()).await? {
        Some((lines, merged, encoding)) => {
            Ok(Some(SearchResult::new(path, lines, merged, encoding)))
        }
        None => Ok(None),
    }
}

// Helper function to extract content from Cursor
fn try_get_cursor_content<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Option<Vec<u8>> {
    // This requires type introspection or a trait extension
    // Option 1: Use Any trait (requires downcasting)
    // Option 2: Add a trait method to detect in-memory readers
    // Option 3: Pass content separately when known to be in memory

    // For now, we can modify EntryStreamProcessor to pass content directly
    // when it's known to be preloaded
    None
}
```

#### Step 3: Modify `EntryStreamProcessor` to Pass Content Directly

Update `EntryStreamProcessor` to pass content directly when it's preloaded:

```rust
// In backend/logseek/src/service/entry_stream.rs

match preload_entry(&mut reader, MAX_PRELOAD_SIZE).await {
    Ok(PreloadResult::Complete(content)) => {
        // Small file fully loaded, can use optimized grep-searcher
        let proc_clone = processor.clone();
        let tx_clone = tx.clone();
        let path = meta.path.clone();
        let container_path = meta.container_path.clone();
        let spec = proc_clone.spec.clone(); // Need to expose spec
        let ctx = proc_clone.context_lines;

        let handle = tokio::task::spawn_blocking(move || {
            // Use optimized grep-searcher for in-memory data
            SearchProcessor::grep_memory_blocking(&content, &spec, ctx, None)
        });

        // Handle result...
    }
    // ... rest of the logic
}
```

### Benefits

1. **Performance**: 5x faster for small archive entries (< 10MB)
2. **Consistency**: Uses the same optimized search engine (`grep-searcher`) for both file-based and in-memory searches
3. **SIMD Optimization**: Leverages `grep-searcher`'s SIMD optimizations
4. **Better Resource Usage**: Fewer system calls, more efficient memory access

### Implementation Considerations

1. **Type Detection**: Need a way to detect when `AsyncRead` is actually a `Cursor` with in-memory data
2. **API Changes**: May need to expose `spec` and `context_lines` from `SearchProcessor`
3. **Backward Compatibility**: Must maintain fallback to `grep_context` for non-memory cases
4. **Testing**: Add tests for in-memory search path

### Migration Steps

1. **Phase 1**: Add `grep_memory_blocking` function
   - Implement the function with same logic as `grep_file_blocking` but using `search_slice`
   - Add unit tests

2. **Phase 2**: Modify `EntryStreamProcessor`
   - Pass content directly when preloaded
   - Call `grep_memory_blocking` for preloaded entries

3. **Phase 3**: Optional - Enhance `process_content`
   - Add detection for in-memory readers
   - Fallback gracefully if detection fails

4. **Phase 4**: Performance Testing
   - Benchmark before/after performance
   - Verify correctness with integration tests

### Expected Impact

- **Small archive entries (< 10MB)**: 5x performance improvement
- **Memory usage**: No change (data already in memory)
- **Code complexity**: Moderate increase (new function, detection logic)
- **Maintainability**: Better (consistent use of `grep-searcher`)

---

## Implementation Priority

### High Priority (Address Soon)
1. **Section 1**: Module Configuration Dependency Injection
   - Eliminates `unsafe` code
   - Improves type safety and testability
   - Foundation for other improvements

2. **Section 2**: Eliminate `unsafe` for Environment Variables
   - Reduces risk of data races
   - Improves code safety

### Medium Priority (Plan for Next Release)
3. **Section 3**: Consistent Configuration Reading Patterns
   - Improves maintainability
   - Makes behavior predictable

4. **Section 6**: Configuration Validation
   - Improves user experience
   - Prevents runtime errors from bad config

5. **Section 7**: Optimize Archive File Search with grep-searcher
   - Significant performance improvement (5x for small files)
   - Better resource utilization
   - Consistent search engine usage

### Low Priority (Future Consideration)
6. **Section 4**: Network Proxy Configuration
   - Current implementation works
   - Improvement is nice-to-have

7. **Section 5**: Module Discovery Documentation
   - Current approach is acceptable
   - Just needs better documentation

---

## Migration Strategy

### Phase 1: Foundation (Week 1-2)
1. Add `ModuleConfig` trait to `opsbox-core`
2. Create `LogSeekConfig` struct
3. Update `Module` trait to accept typed config

### Phase 2: Migration (Week 3-4)
1. Migrate `LogSeekModule` to use typed config
2. Remove `setup_module_env_vars()` for LogSeek
3. Update all `LOGSEEK_*` environment variable reads to use cached config

### Phase 3: Cleanup (Week 5-6)
1. Migrate other modules (Explorer, AgentManager) if applicable
2. Remove remaining `unsafe` blocks for environment variables
3. Add configuration validation

### Phase 4: Documentation (Week 7)
1. Update architecture documentation
2. Add examples for new configuration pattern
3. Document migration guide for future modules

---

## Testing Considerations

### Current Testing Challenges
- Hard to test modules in isolation due to global environment state
- Cannot easily mock configuration
- Environment variable pollution between tests

### After Refactoring
- Modules can be tested with mock configuration objects
- No global state manipulation required
- Clear test boundaries

### Test Strategy
1. Unit tests: Pass mock config objects directly
2. Integration tests: Use test fixtures for configuration
3. No need to manipulate environment variables in tests

---

## References

- Current module architecture: `docs/architecture/module-architecture.md`
- Rust `unsafe` guidelines: https://doc.rust-lang.org/nomicon/meet-safe-and-unsafe.html
- `inventory` crate documentation: https://docs.rs/inventory/
- `OnceLock` documentation: https://doc.rust-lang.org/std/sync/struct.OnceLock.html

---

## Notes

- These refactoring suggestions are based on code review and architectural analysis
- They are not urgent fixes but represent improvements for maintainability and safety
- Implementation should be done incrementally with thorough testing
- Each phase should be completed and tested before moving to the next

