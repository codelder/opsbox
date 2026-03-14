# Architecture

**Analysis Date:** 2026-03-13

## Pattern Overview

**Overall:** Modular Monolith with Compile-Time Module Registration

**Key Characteristics:**
- Inventory-based automatic module discovery at compile time via `register_module!` macro
- Layered architecture within each module (API -> Service -> Repository -> Domain)
- Unified error handling via RFC 7807 Problem Details
- SPA frontend embedded into the Rust binary via `rust-embed`
- Shared SQLite database with per-module schema management

## Layers

**API Layer:**
- Purpose: HTTP route handlers and request/response formatting
- Location: `backend/logseek/src/routes/`, `backend/explorer/src/api/`, `backend/agent-manager/src/routes.rs`
- Contains: Axum route definitions, request validation, response serialization
- Depends on: Service layer, opsbox-core error types
- Used by: Axum router in `backend/opsbox-server/src/server.rs`

**Service Layer:**
- Purpose: Business logic orchestration
- Location: `backend/logseek/src/service/`, `backend/explorer/src/service/`
- Contains: Search execution, resource listing, encoding detection
- Depends on: Repository layer, DFS subsystem, opsbox-core
- Used by: API layer route handlers

**Repository Layer:**
- Purpose: Data access and persistence
- Location: `backend/logseek/src/repository/`, `backend/agent-manager/src/repository.rs`
- Contains: Cache management, database CRUD, S3 profile persistence
- Depends on: sqlx, SQLite pool, opsbox-core database utilities
- Used by: Service layer

**Domain Layer:**
- Purpose: Core business models and rules
- Location: `backend/logseek/src/domain/`, `backend/explorer/src/domain.rs`
- Contains: Source planner configuration, resource type definitions
- Depends on: Internal abstractions only
- Used by: Service layer

**DFS Subsystem (Cross-cutting):**
- Purpose: Unified distributed filesystem abstraction across Local/S3/Agent
- Location: `backend/opsbox-core/src/dfs/`
- Contains: Filesystem trait (`OpbxFileSystem`), ORL parser, backend implementations
- Depends on: tokio, async-trait
- Used by: LogSeek search, Explorer resource listing

## Data Flow

**Search Flow:**

1. Frontend sends `POST /api/v1/logseek/search.ndjson` with query
2. API layer (`routes/`) validates request and invokes service
3. Service layer (`search_executor.rs`) parses query, determines sources via planner scripts
4. SearchExecutor creates `EntryStream` for each target resource
5. DFS subsystem resolves ORL identifiers to concrete storage backends
6. Byte-level regex search via `grep-searcher` streams results as NDJSON
7. Results cached in SQLite for subsequent `view.cache.json` requests

**Explorer Flow:**

1. Frontend sends `POST /api/v1/explorer/list` with ORL path
2. `ExplorerService` resolves ORL to determine endpoint type (Local/S3/Agent)
3. DFS subsystem `OpbxFileSystem` trait dispatches to appropriate implementation
4. Archive files are auto-detected and navigable via `?entry=` parameter
5. Results returned as `ResourceItem` list with metadata

**State Management:**
- Module-level state via `OnceCell` (e.g., global `AgentManager`)
- Search state via LRU cache (`backend/logseek/src/repository/cache.rs`)
- Database state via SQLite WAL mode with connection pooling

## Key Abstractions

**Module Trait:**
- Purpose: Plugin interface for automatic module registration
- Examples: `backend/opsbox-core/src/module.rs`
- Pattern: Trait + inventory crate + `register_module!` macro

**OpbxFileSystem Trait:**
- Purpose: Unified filesystem access across storage backends
- Examples: `backend/opsbox-core/src/dfs/filesystem.rs`
- Pattern: Strategy pattern with async trait implementations

**ORL (OpsBox Resource Locator):**
- Purpose: Unified resource addressing scheme
- Examples: `backend/opsbox-core/src/dfs/orl_parser.rs`
- Pattern: URI parsing with `orl://[id]@[type].[addr]/[path]?entry=[path]`

**Searchable Trait:**
- Purpose: Abstracts different search target types
- Examples: `backend/logseek/src/service/searchable.rs`, `backend/opsbox-core/src/dfs/searchable.rs`
- Pattern: Polymorphic search interface

**AppError:**
- Purpose: Unified error type with RFC 7807 responses
- Examples: `backend/opsbox-core/src/error.rs`
- Pattern: Enum error with `IntoResponse` implementation

## Entry Points

**Main Binary:**
- Location: `backend/opsbox-server/src/main.rs`
- Triggers: CLI invocation
- Responsibilities: CLI parsing, logging init, module discovery, database init, server startup

**Module Registration:**
- Location: Each module's `lib.rs` (e.g., `backend/logseek/src/lib.rs`)
- Triggers: Compile-time via `inventory::submit!`
- Responsibilities: Register module factory for automatic discovery

**Server Router:**
- Location: `backend/opsbox-server/src/server.rs`
- Triggers: `async_main()` after module init
- Responsibilities: Compose Axum router from all registered modules, serve SPA

**Frontend SPA:**
- Location: `web/src/routes/+page.svelte`
- Triggers: Browser navigation
- Responsibilities: UI rendering, API communication

## Error Handling

**Strategy:** Layered error types with unified conversion to `AppError`

**Patterns:**
- Module-level error enums (e.g., `ServiceError`, `RepositoryError`, `FsError`)
- `From` trait implementations for error conversion up the stack
- `AppError` implements `IntoResponse` with RFC 7807 Problem Details format
- HTTP status codes mapped by error variant (400/404/500/502)

## Cross-Cutting Concerns

**Logging:** `tracing` ecosystem with configurable levels, JSON output, file rotation
**Validation:** Request validation at API layer using serde deserialization
**Authentication:** None currently implemented (internal tool assumption)

---

*Architecture analysis: 2026-03-13*
