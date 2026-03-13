# Architecture

**Analysis Date:** 2026-03-13

## Pattern Overview

**Overall:** Modular monolith with plugin-based module discovery, layered architecture per module, and a shared core library.

**Key Characteristics:**
- Compile-time module registration via `inventory` crate -- modules self-register at link time without explicit wiring
- Layered architecture within each module: API (routes) -> Service -> Repository, with a Domain layer for core models
- Unified error handling via `AppError` with RFC 7807 Problem Details responses
- Single SQLite database shared across all modules with automatic schema initialization
- Embedded SPA frontend compiled into the binary via `rust-embed`

## Layers

**opsbox-core (Shared Foundation):**
- Purpose: Provides cross-cutting infrastructure used by all modules
- Location: `backend/opsbox-core/src/`
- Contains: Module trait, database pool, error types, DFS subsystem, LLM abstraction, middleware, logging, filesystem utilities, S3 repository, agent client
- Depends on: axum, sqlx, tokio, tracing, aws-sdk-s3
- Used by: All modules (logseek, explorer, agent-manager, opsbox-server)

**Module Layer (logseek, explorer, agent-manager):**
- Purpose: Feature-specific business logic, each implementing the `Module` trait
- Location: `backend/logseek/src/`, `backend/explorer/src/`, `backend/agent-manager/src/`
- Contains: Own routes, service, repository, domain sub-layers
- Depends on: opsbox-core, axum
- Used by: opsbox-server (discovered automatically at startup)

**opsbox-server (Composition Root):**
- Purpose: Application entry point, CLI parsing, server startup, module orchestration
- Location: `backend/opsbox-server/src/`
- Contains: main.rs, config.rs, server.rs, logging.rs, daemon support
- Depends on: opsbox-core, all modules
- Used by: End users (binary entry point)

**Frontend (SvelteKit SPA):**
- Purpose: User interface with modular API clients matching backend module structure
- Location: `web/src/`
- Contains: Routes, components, API clients, composables, type definitions
- Depends on: Vite, SvelteKit, TailwindCSS 4.0
- Built into: `backend/opsbox-server/static/` (embedded in binary)

## Data Flow

**Request Flow (HTTP):**

1. Client request arrives at Axum router in `opsbox-server/src/server.rs`
2. Request is routed to the appropriate module based on API prefix (e.g., `/api/v1/logseek/*` -> logseek module)
3. Module's route handler processes the request (API layer)
4. Handler calls into Service layer for business logic
5. Service layer may call Repository layer for database/storage access
6. Response flows back through the layers as `Result<T, AppError>`
7. `AppError` implements `IntoResponse` for automatic RFC 7807 Problem Details formatting

**Module Discovery Flow:**

1. Each module crate uses `register_module!` macro which calls `inventory::submit!`
2. At startup, `opsbox-server/src/main.rs` explicitly references optional crate dependencies (`extern crate logseek`)
3. This forces the linker to include the crate, triggering `inventory::submit!`
4. `opsbox-core::get_all_modules()` iterates all registered `ModuleFactory` entries
5. Server calls `configure()`, `init_schema()`, then `router()` on each module

**Search Flow (LogSeek):**

1. POST `/api/v1/logseek/search.ndjson` with ORL resources and query
2. `SearchExecutor` orchestrates parallel searches across local files, S3, and agents
3. Results stream back as NDJSON via tokio channels
4. Frontend `useStreamReader` composable consumes the stream in real-time

## Key Abstractions

**Module Trait (`opsbox-core/src/module.rs`):**
- Purpose: Defines the contract all pluggable modules must implement
- Methods: `name()`, `api_prefix()`, `configure()`, `init_schema()`, `router()`, `cleanup()`
- Pattern: Trait object (`Arc<dyn Module>`) collected via inventory

**DFS Subsystem (`opsbox-core/src/dfs/`):**
- Purpose: Unified abstraction for accessing resources across Local, S3, and Agent endpoints
- Key types: `OpbxFileSystem` trait, `Resource`, `OrlParser`, `Endpoint`
- Implementations: `LocalFileSystem`, `S3Storage`, `AgentProxyFS`, `ArchiveFileSystem`

**ORL Protocol (`opsbox-core/src/dfs/orl_parser.rs`):**
- Purpose: Unified resource identifier scheme (`orl://[id]@[type].[addr]/[path]?entry=[entry]`)
- Enables cross-endpoint resource addressing for local files, S3 objects, agent files, and archive entries

**AppError (`opsbox-core/src/error.rs`):**
- Purpose: Unified error type for all modules with automatic HTTP response generation
- Variants: Database, Config, Internal, BadRequest, NotFound, ExternalService
- Pattern: Implements `IntoResponse` for RFC 7807 Problem Details

**Searchable Trait (`logseek/src/service/searchable.rs`):**
- Purpose: Abstraction for searchable resources across different backends
- Used by SearchExecutor to query local files, S3, and agents uniformly

## Entry Points

**Binary Entry (`backend/opsbox-server/src/main.rs`):**
- Location: `backend/opsbox-server/src/main.rs`
- Triggers: CLI invocation or daemon start
- Responsibilities: Parse CLI args, init logging, init database, discover modules, configure modules, init schemas, start HTTP server

**Module Registration (each module's `lib.rs`):**
- Location: e.g., `backend/logseek/src/lib.rs`
- Triggers: Link time (inventory mechanism)
- Responsibilities: Register module factory with inventory, implement Module trait

**HTTP Server (`backend/opsbox-server/src/server.rs`):**
- Location: `backend/opsbox-server/src/server.rs`
- Triggers: After all modules are initialized
- Responsibilities: Build Axum router by nesting all module routers, serve embedded static assets with SPA fallback, graceful shutdown

## Error Handling

**Strategy:** Unified error type (`AppError`) in opsbox-core, with module-specific error types that convert to `AppError`.

**Patterns:**
- Each module defines its own error enum (e.g., `ServiceError`, `RepositoryError`) in its service/repository layer
- Module errors convert to `AppError` via `From` implementations (see `logseek/src/lib.rs`)
- `AppError` converts to HTTP responses with RFC 7807 Problem Details format
- 5xx errors are logged at `error` level, 4xx at `warn` level

## Cross-Cutting Concerns

**Logging:** `tracing` ecosystem with `tracing-subscriber`, configurable via CLI args and API endpoints (`/api/v1/log/*`)
**Validation:** Axum extractors for request validation, custom validation in service layer
**Authentication:** Not implemented (single-user tool)
**Database:** SQLite via `sqlx` with automatic schema initialization per module, shared pool
**Memory:** `mimalloc` global allocator with explicit collection on cache cleanup

---

*Architecture analysis: 2026-03-13*
