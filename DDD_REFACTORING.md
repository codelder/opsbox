# DDD Refactoring Summary

## Overview

This document summarizes the Domain-Driven Design (DDD) refactoring work for the OpsBox project, tracking completed tasks, pending work, and architectural decisions.

**Branch:** `refactor/ddd-domain-model`

**Status:** Phase 1-2 Complete, Phase 3 In Progress

---

## Architecture Design

### New Layer Structure

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                        │
│  (explorer, logseek, agent-manager - high-level services)   │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                   Domain Layer (opsbox-domain)              │
│  - Pure domain models (ResourceIdentifier, QueryExpression) │
│  - Domain traits (EndpointConnector, ResourceRegistry)      │
│  - No external dependencies                                 │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                Infrastructure (opsbox-resource)              │
│  - EndpointConnector implementations                        │
│  - Adapters to existing OpsFileSystem                       │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                   Core (opsbox-core)                        │
│  - OpsFileSystem, AgentClient, Storage                       │
└─────────────────────────────────────────────────────────────┘
```

### Key Domain Types

**ResourceIdentifier** (opsbox-domain)
```rust
pub struct ResourceIdentifier {
    pub endpoint: EndpointReference,
    pub path: ResourcePath,
    pub archive_entry: Option<ArchiveEntryPath>,
}
```

**EndpointConnector** (opsbox-domain trait)
```rust
#[async_trait]
pub trait EndpointConnector: Send + Sync {
    async fn metadata(&self, path: &ResourcePath) -> Result<ResourceMetadata, DomainError>;
    async fn list(&self, path: &ResourcePath) -> Result<Vec<ResourceMetadata>, DomainError>;
    async fn read(&self, path: &ResourcePath) -> Result<Pin<Box<dyn AsyncRead + Send + Unpin>>, DomainError>;
    async fn exists(&self, path: &ResourcePath) -> Result<bool, DomainError>;
}
```

---

## Completed Work (Phase 1-2)

### Phase 1: Domain Layer Creation ✅

**Commit:** `6c9c639` → `905b279` (rewritten)

**New Crate:** `opsbox-domain`

**Components:**
- `ResourceIdentifier` - Type-safe ORL parsing with `FromStr` trait
- `EndpointReference` - Endpoint identification with type and id
- `ResourcePath` - Newtype wrapper for paths
- `ArchiveEntryPath` - Newtype wrapper for archive entry paths
- `EndpointConnector` trait - Abstract resource access interface
- `QueryExpression` - Parsed query AST (moved from logseek)
- `SearchSession` - Search session management

**Test Results:** 71 unit tests passing

### Phase 2: Resource Access Layer ✅

**Commits:**
- `d6f02d1` → `1e64d96` (opsbox-resource crate)
- `12d8021` (S3 and Agent connectors)
- `e5fce02` (archive navigation)

**New Crate:** `opsbox-resource`

**Implementations:**
- `LocalEndpointConnector` - Delegates to `LocalOpsFS`
- `S3EndpointConnector` - Delegates to `S3OpsFS`
- `AgentEndpointConnector` - Delegates to `AgentOpsFS`
- `ArchiveEndpointConnector` - Tar/tar.gz/zip navigation support

**Key Features:**
- Adapter pattern: wraps existing OpsFileSystem implementations
- Type-safe metadata conversion
- Archive kind detection based on file extension
- All 5 module tests passing

### Phase 3: Application Layer Migration ✅

**Commit:** `b8254eb`

**Explorer Module:**
- Migrated to use `ResourceIdentifier` instead of ORL strings
- Type-safe ORL parsing in API routes
- Archive navigation support with `archive_entry` field

**Agent Manager Module:**
- Integrated `DomainAgent` with `AgentConverter`
- Type conversions between domain and repository models

**Logseek Module:**
- Adopted `ParsedQuery` and `QueryExpression` from domain layer
- Removed duplicate type definitions

### Bug Fixes ✅

**Commit:** `502687f`

**Issues Fixed:**
1. **Agent Search Root Path Handling**
   - Problem: Agent returns canonicalized absolute paths for search roots
   - Solution: `convert_entry` function detects and preserves absolute paths
   - Test: `test_convert_entry_with_absolute_path`

2. **Archive Navigation Path Filtering**
   - Problem: Path filtering logic showed all nested entries instead of direct children
   - Solution: Use `split_once('/')` logic from old TarOpsFS
   - Tests: `test_list_archive_root_directory_only`, `test_list_archive_subdirectory`

3. **Archive Path Combination**
   - Problem: Explorer service not passing `archive_entry` to connector
   - Solution: Combine archive file path and inner entry path before passing to connector
   - Tests: `test_list_with_archive_entry_path_combination`, `test_list_archive_root_without_entry`

**Test Results:**
- 63 E2E tests passing
- 42 opsbox-resource tests passing
- 12 explorer tests passing
- 416 logseek tests passing (1 ignored)

---

## Completed Work (Phase 3-4) ✅

### Logseek Module: EndpointConnector Migration

**Status:** Completed

**Commits:** Multiple commits implementing the migration

**Implementation Details:**

#### Phase 1: EntryStream Creation ✅
**File:** `logseek/src/service/entry_stream.rs`

- `create_entry_stream_v1` - Old implementation using OpsFileSystem
- `create_entry_stream_v2` - New implementation using EndpointConnector
- `EntryStreamAdapter` - Adapter to bridge opsbox-resource and opsbox-core EntryStream traits
- Conditional compilation selects implementation based on `use-endpoint-connector` feature

#### Phase 2-3: SearchableFileSystem Implementation ✅
**File:** `logseek/src/service/searchable.rs`

- `create_search_provider_v1` - Old factory using OpsFileSystem
- `create_search_provider_v2` - New factory using EndpointConnector
- `impl SearchableFileSystem for LocalEndpointConnector` - Direct EndpointConnector search
- `impl SearchableFileSystem for S3EndpointConnector` - S3 EndpointConnector search
- `impl SearchableFileSystem for LocalOpsFS` - Legacy support
- `impl SearchableFileSystem for S3OpsFS` - Legacy support

#### Phase 4: Feature Flag Configuration ✅
**File:** `logseek/Cargo.toml`

```toml
[features]
use-endpoint-connector = []
default = ["use-endpoint-connector"]
```

Default enables new EndpointConnector implementation. Can rollback by disabling feature.

**Test Results:**
- 487 unit tests passing
- 63 E2E tests passing
- 0 failures

---

## Pending Work (Technical Debt Cleanup)

**Plan File:** `/Users/wangyue/.claude/plans/playful-mapping-sun.md`

**Goal:** Migrate logseek from using `OpsFileSystem` directly to using `EndpointConnector` abstraction.

**Current State:**
- Logseek still uses `create_entry_stream` which creates `OpsFileSystem` directly
- Need to use `EndpointConnector` implementations from `opsbox-resource`

**Migration Strategy:** Incremental replacement (see plan file for details)

**Estimated Time:** 16-24 hours (2-3 work days)

**Key Files to Modify:**
- `logseek/src/service/entry_stream.rs` - Add `create_entry_stream_v2`
- `logseek/src/service/searchable.rs` - Implement `SearchableFileSystem` for `EndpointConnector`
- `logseek/src/service/searchable.rs` - Update `create_search_provider` factory
- `logseek/Cargo.toml` - Update dependencies

**Known Issues:**
1. **Test `test_search_with_tar_gz_archive` is ignored**
   - Reason: Archive search doesn't send `Complete` event properly
   - Location: `logseek/src/service/search_executor.rs:2455`
   - Fix: Implement proper `Complete` event sending in archive search flow

---

## Technical Decisions

### 1. Newtype Pattern for Type Safety

Instead of using raw strings for paths, we use newtype wrappers:

```rust
pub struct ResourcePath(String);
pub struct ArchiveEntryPath(String);
pub struct EndpointReference { /* ... */ }
```

**Benefits:**
- Type safety - can't mix up different path types
- Self-documenting code
- Encapsulation of validation logic

### 2. Adapter Pattern for Legacy Integration

Instead of rewriting all existing code, we use adapters:

```rust
pub struct LocalEndpointConnector {
    inner: Arc<LocalOpsFS>,
}

impl EndpointConnector for LocalEndpointConnector {
    // Delegate to inner OpsFS
}
```

**Benefits:**
- Gradual migration
- Existing code continues to work
- Clear separation of concerns

### 3. Trait-Based Abstraction

The `EndpointConnector` trait allows polymorphic resource access:

```rust
async fn list(&self, path: &ResourcePath) -> Result<Vec<ResourceMetadata>, DomainError>;
```

**Benefits:**
- Easy to mock for testing
- Can swap implementations
- Supports dependency injection

---

## Testing Strategy

### Unit Tests

**By Module:**
- `opsbox-domain`: 71 tests (domain types, ORL parsing)
- `opsbox-resource`: 42 tests (connector implementations)
- `explorer`: 12 tests (service layer)
- `logseek`: 416 tests (search functionality, 1 ignored)

**Total:** 541+ unit tests passing

### E2E Tests

**Playwright Tests:** 63 passing

**Coverage:**
- Local file browsing and searching
- S3 file browsing and searching
- Agent discovery and file access
- Archive navigation (tar, tar.gz, zip)
- Path filtering and glob matching

---

## Known Technical Debt

### 1. ORL String Format Compatibility

**Issue:** Two ORL formats exist in the codebase:
- Old: `odfi://` (legacy, still in some places)
- New: `orl://` (DDD version)

**Action:** Eventually migrate all references to `orl://`

**Files to Check:**
- Frontend: `web/src/lib/modules/` (ORL parsing utilities)
- Backend: Search for `"odfi://"` references

### 2. Test `test_search_with_tar_gz_archive` Ignored

**Location:** `logseek/src/service/search_executor.rs:2455`

**Issue:** Archive search doesn't send `Complete` event, causing test to hang

**Workaround:** Test marked with `#[ignore]`

**Fix Required:** Implement proper event completion in archive search flow

### 3. Duplicate EntryStream Traits

**Issue:** Two `EntryStream` traits exist:
- `opsbox_core::odfs::fs::EntryStream`
- `opsbox_resource::stream::EntryStream`

**Action:** Eventually consolidate to a single trait

---

## Branch Status

**Current Branch:** `refactor/ddd-domain-model`

**Latest Commit:** `502687f` - fix: resolve agent and archive navigation issues

**Tracking:**
- Local: Up to date with origin
- Remote: `origin/refactor/ddd-domain-model`

**Merged From:** `main`

**Target Merge:** Pending completion of Phase 3-4

---

## Environment Variables

### Required for Testing

```bash
# Disable proxy detection for LLM tests (macOS)
export OPSBOX_NO_PROXY=1

# Or in CI
export CI=1
```

### LLM Configuration

```bash
export LLM_PROVIDER=ollama  # or openai
export OLLAMA_BASE_URL=http://127.0.0.1:11434
export OLLAMA_MODEL=qwen3:8b
```

---

## Build and Test Commands

### Backend

```bash
# Build
cd backend
cargo build --manifest-path Cargo.toml -p opsbox-server

# Test (all modules)
OPSBOX_NO_PROXY=1 cargo test --manifest-path Cargo.toml

# Test specific module
OPSBOX_NO_PROXY=1 cargo test -p opsbox-domain
OPSBOX_NO_PROXY=1 cargo test -p opsbox-resource
OPSBOX_NO_PROXY=1 cargo test -p explorer
OPSBOX_NO_PROXY=1 cargo test -p logseek

# Test with coverage
OPSBOX_NO_PROXY=1 cargo llvm-cov --workspace --lcov
```

### Frontend

```bash
cd web
pnpm install
pnpm dev        # Development server
pnpm build      # Production build
pnpm test:e2e  # E2E tests
```

---

## File Structure Reference

### Domain Layer (`opsbox-domain`)

```
backend/opsbox-domain/src/
├── lib.rs              # Main exports
├── resource/           # Resource bounded context
│   ├── mod.rs
│   ├── identifier.rs   # ResourceIdentifier
│   ├── endpoint.rs     # EndpointReference, EndpointType
│   ├── path.rs         # ResourcePath, ArchiveEntryPath
│   ├── metadata.rs     # ResourceMetadata
│   └── connector.rs    # EndpointConnector trait
└── Cargo.toml
```

### Resource Layer (`opsbox-resource`)

```
backend/opsbox-resource/src/
├── lib.rs
├── local/
│   ├── connector.rs    # LocalEndpointConnector
│   └── mod.rs
├── s3/
│   ├── connector.rs    # S3EndpointConnector
│   └── mod.rs
├── agent/
│   ├── connector.rs    # AgentEndpointConnector
│   └── mod.rs
├── archive/
│   ├── mod.rs          # ArchiveEndpointConnector
│   └── navigation.rs   # Archive navigation logic
├── discovery/
│   ├── agent.rs        # Agent discovery
│   ├── s3.rs           # S3 discovery
│   └── mod.rs
├── stream/
│   ├── mod.rs          # EntryStream adapters
│   ├── local.rs
│   ├── s3.rs
│   └── archive.rs
└── Cargo.toml
```

---

## Commit Message Conventions

All commit messages should be in English.

**Format:**
```
<type>(<scope>): <subject>

<body>
```

**Types:**
- `feat` - New feature
- `fix` - Bug fix
- `refactor` - Code refactoring
- `test` - Adding or updating tests
- `docs` - Documentation
- `chore` - Maintenance tasks

**Examples:**
```
feat(opsbox-resource): implement archive navigation support

fix: resolve agent and archive navigation issues

refactor(logseek): migrate application layer to DDD domain models
```

---

## Next Steps (Priority Order)

### High Priority - Completed ✅

1. ✅ **Fix `test_search_with_tar_gz_archive`** - Completed
2. ✅ **Complete Logseek EndpointConnector Migration** - Completed

### Medium Priority

3. **Consolidate EntryStream Traits**
   - Decide on single EntryStream location
   - Update all implementations
   - Remove deprecated trait
   - **Status:** Two traits exist (opsbox_core and opsbox_resource)

4. **Migrate ORL Format References**
   - Find all `"odfi://"` references
   - Replace with `"orl://"`
   - Update frontend ORL utilities
   - **Status:** Legacy format still present in some places

### Low Priority

5. **Code Quality**
   - Remove clippy warnings
   - Improve test coverage
   - Add more integration tests

### Completed Work Summary

| Phase | Task | Status |
|-------|------|--------|
| 1 | Domain Layer Creation | ✅ Complete |
| 2 | Resource Access Layer | ✅ Complete |
| 3 | Application Layer Migration | ✅ Complete |
| 4 | Logseek EndpointConnector Migration | ✅ Complete |
| 5 | Technical Debt Cleanup | 🔄 In Progress |

---

## Contact & Context

**Started:** February 1, 2026

**Branch:** `refactor/ddd-domain-model`

**Related Issues:**
- DDD migration plan
- E2E test failures after migration
- Agent and archive navigation bugs

**Key Learnings:**
- E2E tests caught integration issues that unit tests missed
- Agent search root returns absolute paths (not relative)
- Archive navigation requires careful path filtering logic
- Type-safe domain models prevent many classes of bugs

---

*Last Updated: February 2, 2026*
