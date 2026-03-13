# Codebase Structure

**Analysis Date:** 2026-03-13

## Directory Layout

```
opsbox/
├── backend/                    # Rust backend workspace
│   ├── Cargo.toml              # Workspace definition (resolver v3)
│   ├── Cargo.lock
│   ├── rustfmt.toml            # Rust formatting config
│   ├── opsbox-server/          # Main binary crate
│   ├── opsbox-core/            # Shared library crate
│   ├── logseek/                # Log search module
│   ├── explorer/               # File explorer module
│   ├── agent-manager/          # Agent management module
│   ├── agent/                  # Standalone agent binary
│   └── test-common/            # Shared test utilities
├── web/                        # SvelteKit frontend
│   ├── package.json
│   ├── vite.config.ts
│   ├── vitest.config.ts
│   ├── src/
│   │   ├── routes/             # SvelteKit routes (SPA)
│   │   └── lib/                # Shared library code
│   └── coverage/               # Test coverage reports
├── docs/                       # Documentation
├── scripts/                    # Build/run/test scripts
├── .planning/codebase/         # GSD codebase analysis docs
├── CLAUDE.md                   # Project instructions for Claude Code
└── README.md
```

## Directory Purposes

**backend/opsbox-server/ (Main Binary):**
- Purpose: Application entry point, CLI, server composition, daemon support
- Contains: main.rs, config.rs, server.rs, logging.rs, daemon.rs
- Key files: `src/main.rs`, `src/server.rs`, `src/config.rs`
- Static assets: `static/` directory (embedded via rust-embed, frontend build output)

**backend/opsbox-core/ (Shared Library):**
- Purpose: Cross-cutting infrastructure for all modules
- Contains: Module trait, database, error types, DFS subsystem, LLM abstraction, middleware, storage
- Key files: `src/lib.rs`, `src/module.rs`, `src/error.rs`, `src/database.rs`
- DFS subsystem: `src/dfs/` (endpoint, filesystem, orl_parser, path, resource, impls/)

**backend/logseek/ (Log Search Module):**
- Purpose: Log search functionality across local files, S3, and agents
- Contains: API routes, service layer, repository layer, domain models, query parser, source planners
- Key files: `src/lib.rs` (Module impl), `src/routes.rs`, `src/service/search_executor.rs`
- Layers: `routes/`, `service/`, `repository/`, `domain/`, `query/`, `utils/`, `planners/`

**backend/explorer/ (File Explorer Module):**
- Purpose: Distributed resource browsing across Local, S3, and Agent endpoints
- Contains: Resource listing, file download, archive navigation
- Key files: `src/lib.rs` (Module impl), `src/service/mod.rs` (ExplorerService)
- Subdirs: `api/`, `domain/`, `fs/`, `service/`

**backend/agent-manager/ (Agent Management Module):**
- Purpose: Agent registry, health monitoring, tag management
- Contains: Agent CRUD, heartbeat, tag management, log proxy
- Key files: `src/lib.rs` (Module impl), `src/manager.rs`, `src/routes.rs`

**backend/agent/ (Standalone Agent Binary):**
- Purpose: Remote agent for distributed log access
- Contains: Agent server, file serving, registration with main server
- Key files: `src/main.rs`

**backend/test-common/ (Test Utilities):**
- Purpose: Shared test helpers, database setup, archive generators
- Contains: Database test utilities, archive generation helpers
- Key files: `src/lib.rs`, `src/database.rs`

**web/src/routes/ (SvelteKit Pages):**
- Purpose: Application pages following SvelteKit file-based routing
- Routes: `/` (home), `/search`, `/view`, `/image-view`, `/explorer`, `/settings`, `/prompt`
- Co-located components: `SearchEmptyState.svelte`, `SearchResultCard.svelte`, `FileHeader.svelte`, settings management components

**web/src/lib/ (Frontend Library):**
- Purpose: Shared frontend code organized by backend module
- `modules/logseek/`: API clients, composables, types, utils for logseek
- `modules/explorer/`: API client, types, utils for explorer
- `modules/agent/`: API clients, composables, types for agent management
- `components/ui/`: Reusable UI components (alert, badge, button, card, input, label, switch, tabs, context-menu, separator)

## Key File Locations

**Entry Points:**
- `backend/opsbox-server/src/main.rs`: Binary entry point
- `web/src/routes/+page.svelte`: Frontend entry page

**Configuration:**
- `backend/Cargo.toml`: Rust workspace definition
- `backend/opsbox-server/src/config.rs`: CLI argument parsing
- `web/vite.config.ts`: Frontend build config
- `web/vitest.config.ts`: Frontend test config
- `backend/rustfmt.toml`: Rust formatting rules

**Core Logic:**
- `backend/opsbox-core/src/module.rs`: Module trait and registration
- `backend/opsbox-core/src/dfs/mod.rs`: DFS subsystem exports
- `backend/logseek/src/service/search_executor.rs`: Search orchestration
- `backend/explorer/src/service/mod.rs`: Explorer service

**Testing:**
- `backend/logseek/tests/`: Integration tests for logseek
- `backend/explorer/tests/`: Integration tests for explorer
- `backend/opsbox-core/tests/`: Integration tests for core
- `web/src/lib/modules/**/*.test.ts`: Frontend unit tests

## Naming Conventions

**Files (Rust):**
- Module files: `mod.rs` or module-name.rs
- Test files: Co-located in `tests/` for integration, `#[cfg(test)]` for unit
- Snake case: `search_executor.rs`, `orl_parser.rs`

**Files (TypeScript/Svelte):**
- Components: PascalCase.svelte (`SearchResultCard.svelte`, `FileHeader.svelte`)
- API clients: camelCase.ts (`search.ts`, `view.ts`, `agents.ts`)
- Composables: camelCase with `use` prefix (`useSearch.svelte.ts`, `useAgents.svelte.ts`)
- Tests: co-located `.test.ts` suffix

**Directories (Rust):**
- Snake case: `source_planner/`, `search_executor/`

**Directories (Frontend):**
- Kebab-case for routes: `image-view/`, `search/`
- camelCase for lib: `modules/`, `composables/`

## Where to Add New Code

**New Backend Module:**
1. Create crate in `backend/new-module/`
2. Add to workspace members in `backend/Cargo.toml`
3. Implement `Module` trait in `src/lib.rs` with `register_module!`
4. Add optional dependency in `backend/opsbox-server/Cargo.toml`
5. Add `extern crate` in `backend/opsbox-server/src/main.rs`
6. Add feature flag in server's Cargo.toml features

**New API Endpoint:**
- Backend: Add route handler in module's `routes/` directory, register in module's `router()` function
- Frontend: Add API client in `web/src/lib/modules/<module>/api/`, add route page in `web/src/routes/`

**New Frontend Module:**
1. Create directory under `web/src/lib/modules/<module-name>/`
2. Add `api/`, `types/`, `composables/` subdirectories as needed
3. Add route pages under `web/src/routes/`

**Shared Utilities:**
- Backend: Add to `backend/opsbox-core/src/` appropriate subdirectory
- Frontend: Add to `web/src/lib/utils/`

## Special Directories

**backend/opsbox-server/static/:**
- Purpose: Frontend build output embedded into binary
- Generated: Yes (by `pnpm --dir web build`)
- Committed: Yes (built artifacts for release binary)

**backend/opsbox-core/src/dfs/impls/:**
- Purpose: DFS backend implementations (Local, S3, Agent, Archive)
- Generated: No
- Committed: Yes

**web/.svelte-kit/:**
- Purpose: SvelteKit generated files
- Generated: Yes
- Committed: No (in .gitignore)

**backend/target/ and web/node_modules/:**
- Purpose: Build artifacts and dependencies
- Generated: Yes
- Committed: No

**.planning/codebase/:**
- Purpose: GSD codebase analysis documents
- Generated: Yes (by GSD commands)
- Committed: Optional

---

*Structure analysis: 2026-03-13*
