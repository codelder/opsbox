# CLAUDE.md

This file provides guidance to Claude Code when working with code in this repository.

## Project Overview

OpsBox is a modular log search and analysis platform built with Rust backend and SvelteKit frontend. It features a pluggable architecture where modules are automatically discovered and registered at compile time. The platform provides unified resource browsing across local files, S3/MinIO storage, and remote agents.

### Tech Stack

- **Backend**: Rust 2024 edition, `tracing` for logging, `mimalloc` as global allocator
- **Frontend**: SvelteKit 2.22 + TypeScript, TailwindCSS 4.0, Vite 7.0
- **Database**: SQLite with automatic schema management
- **Package Manager**: pnpm 10.23.0
- **Version**: 0.1.1

### Core Architecture

- **Monorepo**: `backend/` (Rust workspace) + `web/` (SvelteKit SPA)
- **Module System**: `opsbox-core` inventory-based automatic module discovery
- **ORL Protocol**: Unified resource identifier (`orl://[id]@[type]/[path]?entry=[entry]`)
- **Modules**: `logseek` (search), `explorer` (file browser), `agent-manager` (agent registry)

### Backend Workspace (`backend/`)

| Crate | Purpose |
|-------|---------|
| `opsbox-server` | Main binary, HTTP server, CLI |
| `opsbox-core` | Shared library: error handling, DB, module system, LLM, DFS subsystem |
| `logseek` | Log search: API → Service → Repository layers, Starlark source planning |
| `explorer` | Distributed file browser (Local/S3/Agent), archive navigation |
| `agent-manager` | Agent registry, health monitoring, tag management |
| `agent` | Standalone agent binary for remote log access |
| `test-common` | Shared test utilities |

### Frontend Structure (`web/`)

SvelteKit SPA with `adapter-static`. Key modules under `src/lib/modules/`:
- `logseek/` — API clients, composables, components for log search
- `explorer/` — File explorer UI, grid/list views
- `agent/` — Agent management APIs and composables

Routes: `/` (home), `/search`, `/view`, `/image-view`, `/explorer`, `/settings`, `/prompt`

## Build & Run

```bash
# Setup
corepack enable && corepack prepare pnpm@10.23.0 --activate
pnpm --dir web install

# Development
cargo run --manifest-path backend/Cargo.toml -p opsbox-server  # Backend on :4000
pnpm --dir web dev                                              # Frontend on :5173

# Production Build
pnpm --dir web build                                            # → backend/opsbox-server/static
cargo build --manifest-path backend/Cargo.toml -p opsbox-server --release

# Selective module build
cargo build -p opsbox-server --no-default-features -F logseek,agent-manager
```

## Testing

```bash
# Backend (requires OPSBOX_NO_PROXY=1 on macOS for LLM tests)
OPSBOX_NO_PROXY=1 cargo test --manifest-path backend/Cargo.toml

# Frontend unit tests
pnpm --dir web test:unit                    # All tests
pnpm --dir web test:unit --run --project=server  # Node.js only
pnpm --dir web test:unit --run --project=client  # Browser only

# E2E tests (Playwright)
pnpm --dir web test:e2e

# Coverage
OPSBOX_NO_PROXY=1 cargo llvm-cov --manifest-path backend/Cargo.toml --workspace --lcov
```

**Test counts**: ~1,000 backend tests, ~166 frontend unit tests, ~140 E2E tests

## Key Conventions

### Architecture Patterns
- **Backend**: API → Service → Repository layering; use `opsbox-core::AppError` for errors
- **Frontend**: Svelte 5 Runes for state; API clients match backend endpoints exactly
- **Module registration**: Implement `Module` trait + `register_module!` macro

### API Structure
Each module has its own prefix: `/api/v1/logseek`, `/api/v1/agents`, `/api/v1/explorer`

### ORL Protocol
```
orl://local/var/log/nginx/access.log
orl://web-01@agent.192.168.1.100:4001/app/logs/error.log
orl://prod@s3/bucket/logs/data.tar.gz?entry=internal/service.log
```

### Query Qualifiers
- `app:<name>` — Select planner script for intelligent source planning
- `dt:/fdt:/tdt:` — Date/time range filtering

### Configuration Priority
1. CLI flags (highest) → 2. Environment variables → 3. Database settings → 4. Defaults

### Key Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `OPSBOX_NO_PROXY=1` | Disable proxy for tests (macOS) | — |
| `LLM_PROVIDER` | `ollama` or `openai` | `ollama` |
| `OLLAMA_BASE_URL` | Ollama server URL | `http://127.0.0.1:11434` |
| `OPSBOX_DATABASE_URL` | Custom database path | `~/.opsbox/opsbox.db` |
| `LOGSEEK_IO_TIMEOUT_SEC` | IO timeout | 60 |
| `LOGSEEK_IO_MAX_CONCURRENCY` | Max concurrent IO | 12 |

## Common Tasks

### Adding New Module
1. Create crate in `backend/`, implement `Module` trait
2. Add to workspace in `backend/Cargo.toml`
3. Add dependency in `opsbox-server/Cargo.toml`

### Adding API Endpoint
1. Backend: Add route in module's `routes/`
2. Frontend: Add API client in module's `api/`

### Database Schema Changes
Update `init_schema()` in the module (current system recreates tables)

## Troubleshooting

| Problem | Solution |
|---------|----------|
| Backend tests: "NULL object" on macOS | Set `OPSBOX_NO_PROXY=1` |
| Frontend browser tests: port in use | `lsof -ti:63315 \| xargs kill -9` or use `--project=server` |
| Agent connection refused | Check agent's `host`/`listen_port` tags |
| Database locked | Ensure single server instance; delete `opsbox.db-wal/shm` if needed |
| S3 timeout | Set `OPSBOX_NO_PROXY=1` or `HTTP_PROXY` |
| High memory | Build with `--features mimalloc-collect` |

## Detailed Documentation

For more details, see:
- `.planning/codebase/` — Architecture, structure, stack, conventions analysis
- `.planning/research/` — E2E testing research and best practices
- `backend/opsbox-server/static/query-syntax.md` — LogSeek query syntax reference
- `backend/logseek/src/planners/README.md` — Starlark planner documentation

---
*Last updated: 2026-03-15*
