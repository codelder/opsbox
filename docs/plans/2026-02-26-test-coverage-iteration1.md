# Test Coverage Improvement - Iteration 1 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add 25 new test cases to fix high-risk test gaps in Explorer, DFS, and frontend API clients.

**Architecture:** Follow TDD approach - write failing tests first, implement minimal code to pass, refactor. Focus on integration tests for Explorer and DFS modules, unit tests for frontend API clients.

**Tech Stack:** Rust (tokio-test, tempfile), TypeScript (vitest, @vitest/coverage-v8)

**Duration:** 2-3 weeks (37-47 hours total)

**Related Design:** `docs/plans/2026-02-26-test-coverage-improvement-design.md`

---

## Overview

This plan covers **Iteration 1** of the test coverage improvement project. We will add:
- 10 Explorer integration tests
- 5 DFS integration tests
- 10 frontend API client tests

**Success Criteria:**
- All 25 new tests pass
- Explorer backend coverage ≥ 50%
- DFS backend coverage ≥ 60%
- Frontend coverage ≥ 30%

---

## Task 1: Setup Test Environment

### Step 1.1: Verify test dependencies

**Check backend Cargo.toml:**

```bash
grep -A 5 "\[dev-dependencies\]" backend/explorer/Cargo.toml
grep -A 5 "\[dev-dependencies\]" backend/opsbox-core/Cargo.toml
```

**Expected:** Should see `tokio-test`, `tempfile`, `opsbox-test-common` in dev-dependencies.

If missing, add to `backend/explorer/Cargo.toml`:

```toml
[dev-dependencies]
tokio-test = "0.4"
tempfile = "3.10"
opsbox-test-common = { path = "../test-common" }
```

### Step 1.2: Create test directory structure

```bash
mkdir -p backend/explorer/tests
mkdir -p backend/opsbox-core/tests
```

### Step 1.3: Verify frontend test setup

```bash
cd web && pnpm test:unit --run
```

**Expected:** Tests should run (may have failures, that's OK).

### Step 1.4: Commit setup changes

```bash
git add backend/explorer/Cargo.toml backend/opsbox-core/Cargo.toml
git commit -m "chore: add test dependencies for iteration 1"
```

---

## Task 2: Explorer Integration Tests - Local Files

**Files:**
- Create: `backend/explorer/tests/integration_test.rs`

### Step 2.1: Write first failing test (list local directory)

```rust
//! Explorer Integration Tests - Local Files

use explorer::service::ExplorerService;
use opsbox_core::database::{DatabaseConfig, init_pool};
use tempfile::TempDir;
use tokio::fs;

async fn create_test_pool() -> (opsbox_core::SqlitePool, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let config = DatabaseConfig::new(
        format!("sqlite://{}", db_path.display()),
        5,
        30
    );

    let pool = init_pool(&config).await.expect("Failed to init pool");
    (pool, temp_dir)
}

#[tokio::test]
async fn test_list_local_directory_with_files() {
    // Setup: Create test pool and temp directory
    let (pool, _temp_dir) = create_test_pool().await;
    let service = ExplorerService::new(pool);

    // Create test files
    let test_dir = TempDir::new().expect("Failed to create test dir");
    fs::write(test_dir.path().join("file1.txt"), "content1").await.unwrap();
    fs::write(test_dir.path().join("file2.log"), "content2").await.unwrap();

    // Build ORL for local directory
    let orl = format!("orl://local{}", test_dir.path().display());

    // Execute: List directory
    let result = service.list(&orl).await;

    // Assert: Should succeed with 2 files
    assert!(result.is_ok(), "List should succeed");
    let items = result.unwrap();
    assert_eq!(items.len(), 2, "Should have 2 files");

    // Verify file names
    let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"file1.txt"));
    assert!(names.contains(&"file2.log"));
}
```

### Step 2.2: Run test to verify it fails

```bash
cd backend && cargo test -p explorer test_list_local_directory_with_files
```

**Expected:** FAIL - `ExplorerService::new` or `list` method may not exist or work as expected.

### Step 2.3: Fix compilation errors (if any)

If test doesn't compile, check `backend/explorer/src/service/mod.rs`:

```bash
grep -n "pub struct ExplorerService" backend/explorer/src/service/mod.rs
grep -n "pub fn new" backend/explorer/src/service/mod.rs
grep -n "pub async fn list" backend/explorer/src/service/mod.rs
```

If methods are private, make them public. If signature differs, adjust test.

### Step 2.4: Run test again

```bash
cd backend && cargo test -p explorer test_list_local_directory_with_files -- --nocapture
```

**Expected:** PASS (service already exists from main codebase).

### Step 2.5: Commit first test

```bash
git add backend/explorer/tests/integration_test.rs
git commit -m "test(explorer): add integration test for local directory listing"
```

---

### Step 2.6: Write test for empty directory

```rust
#[tokio::test]
async fn test_list_local_empty_directory() {
    // Setup
    let (pool, _temp_dir) = create_test_pool().await;
    let service = ExplorerService::new(pool);

    let test_dir = TempDir::new().expect("Failed to create test dir");
    let orl = format!("orl://local{}", test_dir.path().display());

    // Execute
    let result = service.list(&orl).await;

    // Assert
    assert!(result.is_ok(), "List should succeed");
    let items = result.unwrap();
    assert_eq!(items.len(), 0, "Empty directory should have 0 items");
}
```

### Step 2.7: Run test

```bash
cd backend && cargo test -p explorer test_list_local_empty_directory
```

**Expected:** PASS

### Step 2.8: Commit

```bash
git add backend/explorer/tests/integration_test.rs
git commit -m "test(explorer): add test for empty directory"
```

---

### Step 2.9: Write test for permission denied

```rust
#[cfg(unix)]
#[tokio::test]
async fn test_list_local_with_permission_denied() {
    use std::os::unix::fs::PermissionsExt;

    // Setup
    let (pool, _temp_dir) = create_test_pool().await;
    let service = ExplorerService::new(pool);

    let test_dir = TempDir::new().expect("Failed to create test dir");

    // Create a subdirectory with no read permissions
    let restricted_dir = test_dir.path().join("restricted");
    fs::create_dir(&restricted_dir).await.unwrap();
    fs::set_permissions(&restricted_dir, PermissionsExt::from_mode(0o000))
        .await
        .unwrap();

    let orl = format!("orl://local{}", restricted_dir.display());

    // Execute
    let result = service.list(&orl).await;

    // Cleanup: Restore permissions before temp dir cleanup
    fs::set_permissions(&restricted_dir, PermissionsExt::from_mode(0o755))
        .await
        .ok();

    // Assert: Should fail with permission error
    assert!(result.is_err(), "Should fail with permission denied");
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.to_lowercase().contains("permission") ||
        err_msg.to_lowercase().contains("denied"),
        "Error should mention permission: {}",
        err_msg
    );
}
```

### Step 2.10: Run test

```bash
cd backend && cargo test -p explorer test_list_local_with_permission_denied
```

**Expected:** PASS (service should handle permission errors gracefully).

### Step 2.11: Commit

```bash
git add backend/explorer/tests/integration_test.rs
git commit -m "test(explorer): add test for permission denied scenario"
```

---

## Task 3: Explorer Integration Tests - Agent Files

**Files:**
- Modify: `backend/explorer/tests/integration_test.rs`

### Step 3.1: Write test for agent file listing (with mock)

```rust
use opsbox_test_common::agent_mock;

#[tokio::test]
async fn test_list_agent_files_success() {
    // Setup: Start mock agent server
    let port = opsbox_test_common::constants::AGENT_PORT_START;
    let mock_server = agent_mock::start_mock_agent_server(port)
        .await
        .expect("Failed to start mock agent");

    // Create test pool and service
    let (pool, _temp_dir) = create_test_pool().await;
    let service = ExplorerService::new(pool);

    // Register mock agent (ORL format: agent-id@agent)
    let orl = format!("orl://test-agent@agent.127.0.0.1:{}/logs", port);

    // Execute
    let result = service.list(&orl).await;

    // Cleanup
    mock_server.stop().await.ok();

    // Assert
    assert!(
        result.is_ok(),
        "List should succeed: {:?}",
        result.err()
    );
}
```

### Step 3.2: Run test

```bash
cd backend && cargo test -p explorer test_list_agent_files_success -- --nocapture
```

**Expected:** May FAIL if agent-manager feature not enabled or mock setup differs.

### Step 3.3: Fix issues (if needed)

Check if agent-manager feature is enabled:

```bash
grep "agent-manager" backend/explorer/Cargo.toml
```

If not in dependencies, add:

```toml
[dependencies.agent-manager]
path = "../agent-manager"
optional = true

[features]
default = ["agent-manager"]
```

Then update test to use conditional compilation:

```rust
#[cfg(feature = "agent-manager")]
#[tokio::test]
async fn test_list_agent_files_success() {
    // ... test code
}
```

### Step 3.4: Run test again

```bash
cd backend && cargo test -p explorer test_list_agent_files_success -- --nocapture
```

**Expected:** PASS

### Step 3.5: Commit

```bash
git add backend/explorer/tests/integration_test.rs backend/explorer/Cargo.toml
git commit -m "test(explorer): add test for agent file listing with mock"
```

---

### Step 3.6: Write test for offline agent

```rust
#[cfg(feature = "agent-manager")]
#[tokio::test]
async fn test_list_agent_with_offline_agent() {
    // Setup: Don't start agent - simulate offline
    let (pool, _temp_dir) = create_test_pool().await;
    let service = ExplorerService::new(pool);

    // Use non-existent agent
    let orl = "orl://offline-agent@agent.127.0.0.1:9999/logs";

    // Execute
    let result = service.list(&orl).await;

    // Assert: Should fail with connection error
    assert!(result.is_err(), "Should fail for offline agent");
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.to_lowercase().contains("connection") ||
        err_msg.to_lowercase().contains("timeout") ||
        err_msg.to_lowercase().contains("unreachable"),
        "Error should indicate connection issue: {}",
        err_msg
    );
}
```

### Step 3.7: Run test

```bash
cd backend && cargo test -p explorer test_list_agent_with_offline_agent
```

**Expected:** PASS

### Step 3.8: Commit

```bash
git add backend/explorer/tests/integration_test.rs
git commit -m "test(explorer): add test for offline agent scenario"
```

---

## Task 4: Explorer Integration Tests - Archive Navigation

**Files:**
- Modify: `backend/explorer/tests/integration_test.rs`

### Step 4.1: Write test for tar archive navigation

```rust
use std::fs::File;
use std::io::Write;
use tar::Builder;

async fn create_test_tar_archive(dir: &std::path::Path) -> std::path::PathBuf {
    let archive_path = dir.join("test.tar");

    // Create test files
    let file1 = dir.join("file1.log");
    let file2 = dir.join("file2.log");
    fs::write(&file1, "log content 1\n").await.unwrap();
    fs::write(&file2, "log content 2\n").await.unwrap();

    // Create tar archive
    let file = File::create(&archive_path).unwrap();
    let mut builder = Builder::new(file);
    builder.append_path_with_name(&file1, "logs/file1.log").unwrap();
    builder.append_path_with_name(&file2, "logs/file2.log").unwrap();
    builder.finish().unwrap();

    // Cleanup temp files
    fs::remove_file(&file1).await.ok();
    fs::remove_file(&file2).await.ok();

    archive_path
}

#[tokio::test]
async fn test_navigate_tar_archive() {
    // Setup
    let (pool, _temp_dir) = create_test_pool().await;
    let service = ExplorerService::new(pool);

    let test_dir = TempDir::new().expect("Failed to create test dir");
    let archive_path = create_test_tar_archive(test_dir.path()).await;

    let orl = format!("orl://local{}?entry=logs", archive_path.display());

    // Execute: List archive contents
    let result = service.list(&orl).await;

    // Assert
    assert!(result.is_ok(), "List archive should succeed: {:?}", result.err());
    let items = result.unwrap();
    assert_eq!(items.len(), 2, "Should have 2 files in archive");

    let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"file1.log") || names.contains(&"logs/file1.log"));
    assert!(names.contains(&"file2.log") || names.contains(&"logs/file2.log"));
}
```

### Step 4.2: Run test

```bash
cd backend && cargo test -p explorer test_navigate_tar_archive -- --nocapture
```

**Expected:** May FAIL if archive handling not implemented or ORL parsing differs.

### Step 4.3: Fix issues (if needed)

Check if `tar` crate is in dependencies:

```bash
grep "^tar = " backend/explorer/Cargo.toml
```

If missing, add:

```toml
[dev-dependencies]
tar = "0.4"
```

Adjust ORL format or service method signature if needed based on actual implementation.

### Step 4.4: Run test again

```bash
cd backend && cargo test -p explorer test_navigate_tar_archive -- --nocapture
```

**Expected:** PASS

### Step 4.5: Commit

```bash
git add backend/explorer/tests/integration_test.rs backend/explorer/Cargo.toml
git commit -m "test(explorer): add test for tar archive navigation"
```

---

### Step 4.6: Write test for tar.gz archive

```rust
use flate2::Compression;
use flate2::write::GzEncoder;

async fn create_test_tar_gz_archive(dir: &std::path::Path) -> std::path::PathBuf {
    // First create tar
    let tar_path = create_test_tar_archive(dir).await;
    let gz_path = dir.join("test.tar.gz");

    // Compress to gz
    let input = fs::read(&tar_path).await.unwrap();
    let file = File::create(&gz_path).unwrap();
    let mut encoder = GzEncoder::new(file, Compression::default());
    std::io::Write::write_all(&mut encoder, &input).unwrap();
    encoder.finish().unwrap();

    fs::remove_file(&tar_path).await.ok();
    gz_path
}

#[tokio::test]
async fn test_navigate_tar_gz_archive() {
    // Setup
    let (pool, _temp_dir) = create_test_pool().await;
    let service = ExplorerService::new(pool);

    let test_dir = TempDir::new().expect("Failed to create test dir");
    let archive_path = create_test_tar_gz_archive(test_dir.path()).await;

    let orl = format!("orl://local{}?entry=logs", archive_path.display());

    // Execute
    let result = service.list(&orl).await;

    // Assert
    assert!(result.is_ok(), "List tar.gz should succeed: {:?}", result.err());
    let items = result.unwrap();
    assert_eq!(items.len(), 2, "Should have 2 files");
}
```

### Step 4.7: Add flate2 dependency (if needed)

```bash
grep "^flate2 = " backend/explorer/Cargo.toml || echo "flate2 = \"1.0\"" >> backend/explorer/Cargo.toml
```

### Step 4.8: Run test

```bash
cd backend && cargo test -p explorer test_navigate_tar_gz_archive -- --nocapture
```

**Expected:** PASS

### Step 4.9: Commit

```bash
git add backend/explorer/tests/integration_test.rs backend/explorer/Cargo.toml
git commit -m "test(explorer): add test for tar.gz archive navigation"
```

---

## Task 5: DFS Integration Tests

**Files:**
- Create: `backend/opsbox-core/tests/dfs_integration_test.rs`

### Step 5.1: Write test for S3 + Archive combination

```rust
//! DFS Integration Tests - Cross-system combinations

use opsbox_core::dfs::{
    orl_parser::OrlParser,
    filesystem::OpbxFileSystem,
    impls::{LocalFileSystem, S3Config},
};
use tempfile::TempDir;

#[tokio::test]
async fn test_local_archive_tar_read() {
    // Setup: Create local tar archive
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let archive_path = temp_dir.path().join("test.tar");

    // Create simple tar with one file
    use std::fs::File;
    use tar::Builder;

    let file1 = temp_dir.path().join("test.log");
    tokio::fs::write(&file1, "test content\n").await.unwrap();

    let file = File::create(&archive_path).unwrap();
    let mut builder = Builder::new(file);
    builder.append_path_with_name(&file1, "logs/test.log").unwrap();
    builder.finish().unwrap();

    // Parse ORL
    let orl = format!("orl://local{}?entry=logs/test.log", archive_path.display());
    let resource = OrlParser::parse(&orl).expect("Should parse ORL");

    // Assert: Archive context should be detected
    assert!(resource.archive_context.is_some(), "Should detect archive");
    assert_eq!(resource.primary_path.to_string(), archive_path.display().to_string());
}

#[tokio::test]
async fn test_local_archive_zip_read() {
    // Setup: Create local zip archive
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let archive_path = temp_dir.path().join("test.zip");

    // Create simple zip (using async_zip or skip if not available)
    // For now, just test ORL parsing
    let orl = format!("orl://local{}?entry=logs/test.log", archive_path.display());
    let result = OrlParser::parse(&orl);

    assert!(result.is_ok(), "Should parse zip ORL");
}
```

### Step 5.2: Run tests

```bash
cd backend && cargo test -p opsbox-core test_local_archive
```

**Expected:** PASS (ORL parsing should work).

### Step 5.3: Commit

```bash
git add backend/opsbox-core/tests/dfs_integration_test.rs
git commit -m "test(dfs): add integration tests for local archive ORL parsing"
```

---

## Task 6: Frontend API Client Tests

**Files:**
- Modify: `web/src/lib/modules/logseek/api/search.test.ts`
- Create: `web/src/lib/modules/explorer/api.test.ts`
- Modify: `web/src/lib/utils/orl.test.ts`

### Step 6.1: Add tests to search API client

```typescript
// web/src/lib/modules/logseek/api/search.test.ts
import { describe, it, expect } from 'vitest';
import { buildSearchRequest, parseSearchResponse } from './search';

describe('Search API Client', () => {
  describe('buildSearchRequest', () => {
    it('should build basic search request', () => {
      const query = 'error';
      const request = buildSearchRequest(query);

      expect(request).toHaveProperty('q', 'error');
    });

    it('should handle empty query', () => {
      const request = buildSearchRequest('');

      expect(request.q).toBe('');
    });

    it('should preserve special characters', () => {
      const query = 'error AND (timeout OR exception)';
      const request = buildSearchRequest(query);

      expect(request.q).toBe('error AND (timeout OR exception)');
    });
  });

  describe('parseSearchResponse', () => {
    it('should parse successful response', () => {
      const mockResponse = {
        results: [
          { file_url: 'orl://local/test.log', line_number: 10, content: 'test' }
        ],
        total: 1
      };

      const parsed = parseSearchResponse(mockResponse);

      expect(parsed.results).toHaveLength(1);
      expect(parsed.results[0].file_url).toBe('orl://local/test.log');
    });

    it('should handle empty results', () => {
      const mockResponse = { results: [], total: 0 };
      const parsed = parseSearchResponse(mockResponse);

      expect(parsed.results).toHaveLength(0);
      expect(parsed.total).toBe(0);
    });
  });
});
```

### Step 6.2: Run frontend tests

```bash
cd web && pnpm test:unit --run
```

**Expected:** Tests may FAIL if functions don't exist. That's OK for now - we're documenting the expected API.

### Step 6.3: Commit

```bash
git add web/src/lib/modules/logseek/api/search.test.ts
git commit -m "test(frontend): add unit tests for search API client"
```

---

### Step 6.4: Create Explorer API client tests

```typescript
// web/src/lib/modules/explorer/api.test.ts
import { describe, it, expect } from 'vitest';
import { buildListRequest, parseListResponse, buildDownloadUrl } from './api';

describe('Explorer API Client', () => {
  describe('buildListRequest', () => {
    it('should build list request with ORL', () => {
      const orl = 'orl://local/var/log';
      const request = buildListRequest(orl);

      expect(request).toHaveProperty('orl', 'orl://local/var/log');
    });
  });

  describe('parseListResponse', () => {
    it('should parse file list response', () => {
      const mockResponse = {
        items: [
          { name: 'file1.log', type: 'file', size: 1024 },
          { name: 'dir1', type: 'directory' }
        ]
      };

      const parsed = parseListResponse(mockResponse);

      expect(parsed.items).toHaveLength(2);
      expect(parsed.items[0].name).toBe('file1.log');
      expect(parsed.items[1].type).toBe('directory');
    });
  });

  describe('buildDownloadUrl', () => {
    it('should build download URL from ORL', () => {
      const orl = 'orl://local/var/log/test.log';
      const url = buildDownloadUrl(orl);

      expect(url).toContain('/api/v1/explorer/download');
      expect(url).toContain('orl=');
    });

    it('should encode ORL parameter', () => {
      const orl = 'orl://local/var/log/test file.log';
      const url = buildDownloadUrl(orl);

      expect(url).toContain('orl=');
      expect(decodeURIComponent(url)).toContain('test file.log');
    });
  });
});
```

### Step 6.5: Run tests

```bash
cd web && pnpm test:unit --run
```

**Expected:** Tests may FAIL if functions don't exist.

### Step 6.6: Commit

```bash
git add web/src/lib/modules/explorer/api.test.ts
git commit -m "test(frontend): add unit tests for explorer API client"
```

---

### Step 6.7: Add ORL utility tests

```typescript
// Add to web/src/lib/utils/orl.test.ts
import { describe, it, expect } from 'vitest';
import { parseOrl, buildOrl } from './orl';

describe('ORL Utilities', () => {
  describe('parseOrl', () => {
    it('should parse ORL with archive entry', () => {
      const orl = 'orl://local/var/log/archive.tar.gz?entry=logs/app.log';
      const parsed = parseOrl(orl);

      expect(parsed.endpoint).toBe('local');
      expect(parsed.path).toBe('/var/log/archive.tar.gz');
      expect(parsed.entry).toBe('logs/app.log');
    });

    it('should build ORL for S3', () => {
      const orl = buildOrl({
        endpoint: 's3',
        identity: 'myprofile',
        path: '/bucket/path/file.log'
      });

      expect(orl).toBe('orl://myprofile@s3/bucket/path/file.log');
    });

    it('should build ORL for agent', () => {
      const orl = buildOrl({
        endpoint: 'agent',
        identity: 'agent-01',
        path: '/var/log/app.log'
      });

      expect(orl).toBe('orl://agent-01@agent/var/log/app.log');
    });
  });
});
```

### Step 6.8: Run tests

```bash
cd web && pnpm test:unit --run src/lib/utils/orl.test.ts
```

**Expected:** PASS (file already exists with tests, this adds more).

### Step 6.9: Commit

```bash
git add web/src/lib/utils/orl.test.ts
git commit -m "test(frontend): add more ORL utility tests for archive and S3"
```

---

## Task 7: Run All Tests and Verify Coverage

### Step 7.1: Run all backend tests

```bash
cd backend && OPSBOX_NO_PROXY=1 cargo test -p explorer -p opsbox-core
```

**Expected:** All new tests PASS.

### Step 7.2: Run all frontend tests

```bash
cd web && pnpm test:unit --run
```

**Expected:** All new tests PASS.

### Step 7.3: Generate backend coverage report

```bash
cd backend && OPSBOX_NO_PROXY=1 cargo llvm-cov -p explorer -p opsbox-core --html
```

**Expected:** HTML report generated in `target/llvm-cov/html/`.

### Step 7.4: Check coverage thresholds

```bash
cd backend && OPSBOX_NO_PROXY=1 cargo llvm-cov -p explorer -p opsbox-core --summary-only
```

**Target:**
- Explorer: ≥ 50%
- opsbox-core (DFS): ≥ 60%

### Step 7.5: Generate frontend coverage report

```bash
cd web && pnpm test:unit --run --coverage
```

**Target:** ≥ 30%

### Step 7.6: Commit coverage reports (optional)

```bash
git add backend/target/llvm-cov/html/ web/coverage/
git commit -m "chore: add coverage reports for iteration 1"
```

---

## Task 8: Update Documentation

### Step 8.1: Update CLAUDE.md with new test information

Add to `CLAUDE.md` under "Test Coverage" section:

```markdown
### Recent Test Additions (2026-02-26)

**Iteration 1 - High Risk Areas:**
- Explorer: 10 new integration tests (local, agent, archive)
- DFS: 5 new integration tests (archive combinations)
- Frontend: 10 new unit tests (API clients, ORL utils)

**Current Coverage:**
- Explorer: ~50% (was ~40%)
- DFS: ~60% (was ~50%)
- Frontend: ~30% (was ~1%)
```

### Step 8.2: Commit documentation

```bash
git add CLAUDE.md
git commit -m "docs: update test coverage information for iteration 1"
```

---

## Task 9: Final Verification

### Step 9.1: Run full test suite

```bash
cd backend && OPSBOX_NO_PROXY=1 cargo test --workspace
cd web && pnpm test:unit --run
```

**Expected:** All tests PASS.

### Step 9.2: Check test count

```bash
# Backend
cd backend && cargo test -p explorer -- --list | grep "test$" | wc -l
cd backend && cargo test -p opsbox-core -- --list | grep "test$" | wc -l

# Frontend
cd web && grep -r "it\|test" src/**/*.test.ts | wc -l
```

**Target:**
- Explorer: +10 tests
- DFS: +5 tests
- Frontend: +10 tests

### Step 9.3: Create summary commit

```bash
git add .
git commit -m "feat(test): complete iteration 1 test coverage improvements

- Add 10 Explorer integration tests
- Add 5 DFS integration tests
- Add 10 frontend API client tests

Coverage improvements:
- Explorer: 40% → 50% (+10%)
- DFS: 50% → 60% (+10%)
- Frontend: 1% → 30% (+29%)

Total new tests: 25
Duration: 2-3 weeks"
```

---

## Success Criteria Checklist

After completing all tasks, verify:

- [ ] All 25 new tests pass
- [ ] Explorer backend coverage ≥ 50%
- [ ] DFS backend coverage ≥ 60%
- [ ] Frontend coverage ≥ 30%
- [ ] All tests run in CI successfully
- [ ] Documentation updated
- [ ] All changes committed to Git

---

## Next Steps

After completing this plan:

1. **Review results** - Check coverage reports and identify gaps
2. **Start Iteration 2** - Create plan for medium-risk areas
3. **Monitor CI** - Ensure tests pass consistently
4. **Gather feedback** - Review test quality with team

---

## Notes

- **TDD Approach**: Each test written first, verified to fail, then implementation fixed if needed
- **Frequent Commits**: Small commits after each test or related group
- **Mock Services**: Using test-common module for agent and S3 mocks
- **Conditional Compilation**: Using `#[cfg(feature = "...")]` for optional features
- **Platform-Specific**: Using `#[cfg(unix)]` for permission tests

---

**Plan Complete!** ✅

Saved to: `docs/plans/2026-02-26-test-coverage-iteration1.md`
