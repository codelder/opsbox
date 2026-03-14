# Codebase Concerns

**Analysis Date:** 2026-03-13

## Tech Debt

**Large Service Files:**
- Issue: Key service files are very large (search_executor.rs ~85KB, search.rs ~68KB)
- Files: `backend/logseek/src/service/search_executor.rs`, `backend/logseek/src/service/search.rs`
- Impact: Hard to navigate, understand, and maintain; potential merge conflicts
- Fix approach: Extract sub-modules for distinct responsibilities (query parsing, result handling, session management)

**Legacy ODFI Compatibility:**
- Issue: ORL protocol evolved from ODFI but backward compatibility code may remain
- Files: `backend/opsbox-core/src/dfs/orl_parser.rs`
- Impact: Code complexity, potential confusion about which protocol to use
- Fix approach: Audit and remove ODFI-only code paths if all clients migrated

## Security Considerations

**No Authentication:**
- Risk: Application assumes internal-only access with no user authentication
- Files: `backend/opsbox-server/src/server.rs`
- Current mitigation: Internal tool assumption, network-level access control
- Recommendations: Add optional authentication layer if exposing to untrusted networks

**S3 Credential Storage:**
- Risk: S3 profile credentials stored in SQLite database
- Files: `backend/logseek/src/repository/s3.rs`
- Current mitigation: Database file permissions, single-user deployment
- Recommendations: Consider OS keychain integration for production deployments

## Performance Bottlenecks

**Search Execution:**
- Problem: Large file searches may block on I/O
- Files: `backend/logseek/src/service/search_executor.rs`
- Cause: Synchronous file reads in some code paths
- Improvement path: Ensure all I/O paths use async, add streaming for very large files

**Frontend Coverage:**
- Problem: Overall frontend test coverage is low (14.85%)
- Files: `web/src/`
- Cause: Many route components and composables lack tests
- Improvement path: Prioritize testing API clients and composables first

## Fragile Areas

**Module Registration:**
- Files: `backend/opsbox-core/src/module.rs`, each module's `lib.rs`
- Why fragile: Compile-time inventory registration, errors only surface at build time
- Safe modification: Add new modules by following existing patterns exactly
- Test coverage: Module discovery tested, but edge cases with conflicting names not covered

**Search Session Management:**
- Files: `backend/logseek/src/service/search_executor.rs`, `backend/logseek/src/repository/cache.rs`
- Why fragile: Complex session lifecycle with concurrent access, LRU eviction
- Safe modification: Use session ID as key, ensure proper cleanup on completion/cancellation
- Test coverage: Basic flows covered, race conditions not thoroughly tested

## Test Coverage Gaps

**Frontend Components:**
- What's not tested: Most route-level components, complex composables
- Files: `web/src/routes/**/*.svelte`, `web/src/lib/modules/*/composables/`
- Risk: UI regressions, broken state management
- Priority: Medium (backend is well-tested, frontend is UI layer)

**Agent Integration:**
- What's not tested: End-to-end agent communication with real agent instances
- Files: `backend/opsbox-core/src/agent/`, `backend/agent-manager/src/`
- Risk: Agent protocol changes may break integration
- Priority: Low (unit tests cover most logic, integration requires running agents)

---

*Concerns audit: 2026-03-13*
