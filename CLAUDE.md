# CLAUDE.md

This file provides guidance for Claude Code when working with code in this repository.

## Project Overview

OpsBox is a modular log search and analysis platform built with Rust backend and SvelteKit frontend. It features a pluggable architecture where modules are automatically discovered at compile time, with unified resource browsing across local files, S3/MinIO, and remote agents.

### Technology Stack
- **Backend**: Rust 2024 edition, `tracing` ecosystem, `mimalloc` allocator
- **Frontend**: SvelteKit 2.22, TypeScript, TailwindCSS 4.0, Vite 7.0
- **Database**: SQLite with automatic schema management
- **Build**: Cargo workspace (resolver v3), pnpm 10.23.0

### Core Architecture
- **Monorepo**: Rust backend (`backend/`) + SvelteKit frontend (`web/`)
- **Modules**: `logseek` (log search), `explorer` (file browser), `agent-manager` (agent registry), `agent` (standalone binary)
- **ORL Protocol**: Unified resource identifiers (`orl://[id]@[type][.server_addr]/[path]?entry=[entry_path]`)
- **DFS**: Distributed File System abstraction in `opsbox-core/src/dfs/`

## Backend Structure (`backend/`)

### Workspace Members
- `opsbox-server` - Main binary (CLI: `--io-timeout-sec`, `--io-max-retries`, `--bind`, `--port`, `--log-dir`, `--retention-days`)
- `opsbox-core` - Shared library (error handling, database, DFS/ODFS, LLM, storage, agent client)
- `logseek` - Log search with Starlark source planning, byte-level regex, archive streaming
- `explorer` - Resource browser (Local/S3/Agent), archive navigation, file download
- `agent-manager` - Agent registry, health monitoring, tag management
- `agent` - Standalone agent binary
- `test-common` - Shared test utilities

### Key Dependencies
`starlark`, `grep-regex`, `aws-sdk-s3`, `async_zip`, `tokio-tar`, `reqwest`, `chardetng`, `encoding_rs`, `lru`

## Frontend Structure (`web/`)

- **SvelteKit SPA** with `adapter-static`, embedded into binary via `rust-embed`
- **Modules**: `src/lib/modules/{logseek,agent,explorer}/` with API clients, types, composables
- **Routes**: `/` (home), `/search`, `/view`, `/explorer`, `/settings`, `/prompt`, `/image-view`
- **UI**: `@tanstack/svelte-virtual`, `bits-ui`, `lucide-svelte`, Maple Mono NF CN font

## Test Coverage
- **Backend**: ~1000 passing tests (`OPSBOX_NO_PROXY=1 cargo test --manifest-path backend/Cargo.toml`)
- **Frontend**: 79 passing tests (`pnpm --dir web test`)

## Build Commands

```bash
# Development
cargo run --manifest-path backend/Cargo.toml -p opsbox-server
pnpm --dir web dev

# Production
pnpm --dir web build && cargo build --manifest-path backend/Cargo.toml -p opsbox-server --release

# Testing
OPSBOX_NO_PROXY=1 cargo test --manifest-path backend/Cargo.toml
pnpm --dir web test
```

## Key Conventions

- **API Prefixes**: `/api/v1/logseek`, `/api/v1/agents`, `/api/v1/explorer`
- **Database**: `$HOME/.opsbox/opsbox.db`
- **Error Handling**: `opsbox-core::AppError` with RFC 7807 Problem Details
- **Module System**: Implement `Module` trait, use `register_module!` macro
- **LLM**: Database-persistent backends (Ollama/OpenAI), use `OPSBOX_NO_PROXY=1` for tests

## Environment Variables

| Variable | Description |
|----------|-------------|
| `OPSBOX_NO_PROXY=1` | Disable proxy (required for tests on macOS) |
| `LLM_PROVIDER` | `ollama` or `openai` |
| `OLLAMA_BASE_URL` | Ollama server URL |
| `OPENAI_API_KEY` | OpenAI API key |
| `OPSBOX_IO_TIMEOUT_SEC` | IO timeout (default: 30) |

## ORL Protocol

```
orl://local/var/log/nginx/access.log
orl://web-01@agent.192.168.1.100:4001/app/logs/error.log
orl://prod@s3/bucket/logs/data.tar.gz?entry=internal/service.log
```

## API Endpoints

### LogSeek (`/api/v1/logseek`)
- `POST /search.ndjson` - Stream search results
- `DELETE /search/session/{sid}` - Cancel search
- `GET /view/{download,raw,cache.json,files.json}` - File operations
- `GET/POST /profiles` - S3 profile management
- `GET/POST /settings/llm/*` - LLM backend config
- `GET/POST /settings/planners/*` - Planner scripts
- `POST /nl2q` - Natural language to query

### Agent Manager (`/api/v1/agents`)
- `POST /register`, `GET /`, `GET/{id}`, `DELETE/{id}` - CRUD
- `POST /{id}/heartbeat` - Heartbeat
- `GET/POST /{id}/tags` - Tag management

### Explorer (`/api/v1/explorer`)
- `POST /list` - List resources
- `GET /download?orl=...` - Download file

## Source Planning (Starlark)

Context variables: `CLEANED_QUERY`, `TODAY`, `DATE_RANGE`, `DATES`, `AGENTS`, `S3_PROFILES`

Query qualifier: `app:<appname>` selects planner script

## Troubleshooting

| Issue | Solution |
|-------|----------|
| LLM tests fail with NULL object | `OPSBOX_NO_PROXY=1` |
| Database locked | Ensure single instance, delete wal/shm files |
| S3 timeout | `OPSBOX_NO_PROXY=1` or set proxy |
| High memory | Build with `--features mimalloc-collect` |
