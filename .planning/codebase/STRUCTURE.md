# Codebase Structure

**Analysis Date:** 2026-03-13

## Directory Layout

```
opsboard/
├── backend/                # Rust backend workspace
│   ├── Cargo.toml          # Workspace definition (7 crates)
│   ├── opsbox-server/      # Main binary (entry point)
│   ├── opsbox-core/        # Shared library (DFS, errors, DB, modules)
│   ├── logseek/            # Log search module
│   ├── explorer/           # Distributed file browser module
│   ├── agent-manager/      # Agent registry module
│   ├── agent/              # Standalone agent binary
│   └── test-common/        # Shared test utilities
├── web/                    # SvelteKit frontend (SPA)
│   ├── src/
│   │   ├── routes/         # Page routes (/, /search, /view, /explorer, /settings)
│   │   ├── lib/
│   │   │   ├── modules/    # Feature modules (logseek, explorer, agent)
│   │   │   ├── components/ # Shared UI components
│   │   │   └── utils/      # Shared utilities
│   │   └── app.html        # SPA shell
│   └── package.json
├── docs/                   # Documentation
├── scripts/                # Build/deploy scripts
└── CLAUDE.md               # Project instructions
```

## Directory Purposes

**`backend/opsbox-server/`:**
- Purpose: Main application binary, server composition, CLI
- Contains: Entry point, config, server setup, embedded assets, daemon support
- Key files: `src/main.rs`, `src/server.rs`, `src/config.rs`

**`backend/opsbox-core/`:**
- Purpose: Shared library consumed by all modules
- Contains: Error types, DB management, Module trait, DFS subsystem, LLM abstraction
- Key files: `src/lib.rs`, `src/module.rs`, `src/error.rs`, `src/dfs/mod.rs`

**`backend/logseek/`:**
- Purpose: Log search module with layered architecture
- Contains: Search engine, query parser, Starlark planners, encoding detection
- Key files: `src/lib.rs`, `src/routes.rs` + `src/routes/` subdirectory, `src/service/search_executor.rs`, `src/service/entry_stream.rs`

**`backend/explorer/`:**
- Purpose: Distributed resource browser across Local/S3/Agent
- Contains: Resource listing, archive navigation, download support
- Key files: `src/lib.rs`, `src/service/mod.rs`, `src/api.rs`

**`backend/agent-manager/`:**
- Purpose: Agent registry, health monitoring, tag management
- Contains: Agent CRUD, heartbeat, tag operations, log proxy
- Key files: `src/lib.rs`, `src/manager.rs`, `src/routes.rs`

**`backend/agent/`:**
- Purpose: Standalone agent binary for remote log access
- Contains: Agent server, file exploration, search proxy
- Key files: `src/main.rs`, `src/server.rs`, `src/routes.rs`

**`web/src/routes/`:**
- Purpose: SvelteKit page routes
- Contains: `/` (home), `/search`, `/view`, `/explorer`, `/settings`, `/image-view`, `/prompt`
- Key files: `+page.svelte`, `+layout.svelte` in each route directory

**`web/src/lib/modules/`:**
- Purpose: Frontend feature modules mirroring backend
- Contains: API clients, types, composables per backend module
- Key files: `logseek/api/*.ts`, `explorer/api.ts`, `agent/api/agents.ts`, `agent/api/logs.ts`, `agent/api/config.ts`

## Key File Locations

**Entry Points:**
- `backend/opsbox-server/src/main.rs`: Binary entry point, CLI parsing, async runtime
- `web/src/routes/+page.svelte`: SPA home page

**Configuration:**
- `backend/opsbox-server/src/config.rs`: CLI args via clap, AppConfig struct
- `backend/Cargo.toml`: Workspace member and dependency definitions
- `web/vite.config.ts`: Frontend build config with backend proxy

**Core Logic:**
- `backend/opsbox-core/src/module.rs`: Module trait and registration macro
- `backend/opsbox-core/src/dfs/`: Distributed filesystem abstraction
- `backend/logseek/src/service/search_executor.rs`: Search orchestration (85KB)
- `backend/logseek/src/service/search.rs`: Search core (68KB)

**Testing:**
- Backend: Inline `#[cfg(test)]` modules in each source file
- Frontend: `*.test.ts` files co-located with source, `vitest.config.ts`

## Naming Conventions

**Files:**
- Rust: `snake_case.rs` (e.g., `search_executor.rs`, `orl_parser.rs`)
- TypeScript: `camelCase.ts` or `kebab-case.ts` (e.g., `search.ts`, `llm.test.ts`)
- Svelte: `PascalCase.svelte` (e.g., `SearchResultCard.svelte`)

**Directories:**
- Rust: `snake_case` (e.g., `source_planner/`, `search/`)
- Frontend: `camelCase` or `kebab-case` (e.g., `composables/`, `agent/`)

**Traits:**
- PascalCase with descriptive names (e.g., `OpbxFileSystem`, `Module`, `Searchable`)

**Error Types:**
- Module-level enums (e.g., `ServiceError`, `FsError`, `RepositoryError`)

## Where to Add New Code

**New Feature (module):**
- Primary code: `backend/<module-name>/src/`
- Tests: Inline `#[cfg(test)]` modules + `tests/` for integration
- Frontend: `web/src/lib/modules/<module-name>/`

**New API Endpoint:**
- Backend route: `backend/<module>/src/routes/` or `src/api/routes.rs`
- Frontend client: `web/src/lib/modules/<module>/api/<endpoint>.ts`

**New DFS Backend:**
- Implementation: `backend/opsbox-core/src/dfs/impls/<backend>.rs`
- Export: Add to `backend/opsbox-core/src/dfs/impls/mod.rs`

**Utilities:**
- Shared backend: `backend/opsbox-core/src/` (e.g., `fs.rs`, `storage.rs`)
- Shared frontend: `web/src/lib/utils/`

## Special Directories

**`backend/opsbox-server/static/`:**
- Purpose: Embedded frontend assets (built output)
- Generated: Yes (by `pnpm --dir web build`)
- Committed: No (in .gitignore)

**`backend/target/`:**
- Purpose: Rust build artifacts
- Generated: Yes
- Committed: No

**`web/node_modules/`:**
- Purpose: Frontend dependencies
- Generated: Yes (by `pnpm install`)
- Committed: No

**`backend/opsbox-core/src/dfs/impls/`:**
- Purpose: DFS backend implementations
- Contains: `local.rs`, `s3.rs`, `agent.rs`, `archive.rs`
- Pattern: Each implements `OpbxFileSystem` trait

**`backend/logseek/planners/`:**
- Purpose: Default Starlark planner scripts
- Contains: Built-in planner configurations
- Committed: Yes

---

*Structure analysis: 2026-03-13*
*Last updated: 2026-03-15*
