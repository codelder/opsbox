# CLAUDE.md

This file gives coding agents a current, implementation-aligned overview of the OpsBox repository.

## Project Overview

OpsBox is a Rust + SvelteKit monorepo focused on log search, distributed resource browsing, and Agent management.

Current release line in this repo:

- backend workspace version: `0.2.0`
- frontend package version: `0.2.0`

## Platform Support

OpsBox targets:

- macOS
- Linux
- Windows

Keep cross-platform behavior in mind when editing:

- use `std::path::Path` / `PathBuf`
- avoid assuming `/tmp` or Unix-only semantics unless guarded
- use conditional compilation for daemon/service behavior

## Tech Stack

- Backend: Rust 2024, Axum, Tokio, SQLite, `tracing`
- Memory allocator: `mimalloc` globally in `opsbox-server`
- Frontend: SvelteKit 2.22, Svelte 5, TypeScript, Tailwind CSS 4, Vite 7
- Package manager: `pnpm@10.23.0`

## Workspace Layout

### Backend crates

| Crate | Purpose |
| --- | --- |
| `opsbox-server` | Main HTTP server, CLI, embedded frontend |
| `opsbox-core` | Shared infra: modules, errors, DB, logging, DFS/ORL, Agent/S3 primitives |
| `logseek` | Search, file viewing, S3 settings/profiles, LLM backends, planners, NL2Q |
| `explorer` | ORL-based resource listing and download |
| `agent-manager` | Agent registry, heartbeat, tags, log proxy |
| `opsbox-agent` | Standalone remote agent binary |
| `test-common` | Shared test helpers |

### Frontend

Key route files:

- `/` home query entry
- `/search`
- `/view`
- `/image-view`
- `/explorer`
- `/settings`
- `/prompt`

Feature modules live under:

- `web/src/lib/modules/logseek`
- `web/src/lib/modules/agent`
- `web/src/lib/modules/explorer`

## Runtime Topology

### `opsbox-server`

- default address: `0.0.0.0:4000`
- exposes:
  - `/healthy`
  - `/api/v1/log/*`
  - `/api/v1/logseek/*`
  - `/api/v1/agents/*`
  - `/api/v1/explorer/*`
- serves embedded frontend from `backend/opsbox-server/static`

### `opsbox-agent`

- binary name: `opsbox-agent`
- default listen port: `3976`
- registers to server at `/api/v1/agents/register`
- exposes:
  - `/health`
  - `/api/v1/info`
  - `/api/v1/paths`
  - `/api/v1/search`
  - `/api/v1/cancel/{task_id}` (currently returns `501`)
  - `/api/v1/list_files`
  - `/api/v1/file_raw`
  - `/api/v1/log/*`

## Build & Run

```bash
# setup
corepack enable
corepack prepare pnpm@10.23.0 --activate
pnpm --dir web install

# backend dev
cargo run --manifest-path backend/Cargo.toml -p opsbox-server

# frontend dev
pnpm --dir web dev

# standalone agent
cargo run --manifest-path backend/Cargo.toml -p opsbox-agent -- \
  --server-endpoint http://localhost:4000 \
  --search-roots /var/log,/tmp

# production build
pnpm --dir web build
cargo build --manifest-path backend/Cargo.toml -p opsbox-server --release
cargo build --manifest-path backend/Cargo.toml -p opsbox-agent --release

# selective module build
cargo build --manifest-path backend/Cargo.toml -p opsbox-server --no-default-features -F logseek,agent-manager
```

## Testing

```bash
# backend
cargo test --manifest-path backend/Cargo.toml

# backend coverage
cargo llvm-cov --manifest-path backend/Cargo.toml --workspace --lcov

# frontend unit
pnpm --dir web test

# frontend e2e
pnpm --dir web test:e2e
```

Notes:

- Some networked tests are easier to run with `OPSBOX_NO_PROXY=1`.
- Playwright config already exists in `web/playwright.config.ts`.

## API Map

### Server-side modules

- `logseek` prefix: `/api/v1/logseek`
  - search: `/search.ndjson`
  - view: `/view.cache.json`, `/view/raw`, `/view/download`, `/view.files.json`
  - S3 settings: `/settings/s3`
  - LLM backends: `/settings/llm/*`
  - planners: `/settings/planners/*`
  - profiles: `/profiles`
  - NL2Q: `/nl2q`
- `agent-manager` prefix: `/api/v1/agents`
  - register, list, heartbeat, tags, agent log proxy
- `explorer` prefix: `/api/v1/explorer`
  - `/list`
  - `/download`
- server logs prefix: `/api/v1/log`

## Data & Protocol Conventions

### ORL

OpsBox currently uses `orl://` as the unified resource locator.

Examples:

```text
orl://local/var/log/nginx/access.log
orl://web-01@agent/var/log/app.log
orl://web-01@10.0.0.8:3976@agent/var/log/app.log
orl://default@s3/my-bucket/path/to/file.log
orl://prod:my-bucket@s3/path/to/file.log
orl://local/var/log/archive.tar.gz?entry=inner/file.log
orl://local/var/log/?glob=*.log
```

Important:

- S3 ORL accepts both old and new bucket placement:
  - old: bucket in path
  - new: bucket in endpoint identity (`profile:bucket@s3`)
- frontend and explorer are already ORL-based

### Search / Agent payloads

- Agent search request includes:
  - `task_id`
  - `query`
  - `context_lines`
  - `path_filter`
  - `path_includes`
  - `path_excludes`
  - `target`
  - `encoding`

### Status serialization

`AgentStatus` serializes as tagged JSON:

```json
{ "type": "Online" }
```

## Configuration

Priority generally follows:

1. CLI flags
2. environment variables
3. persisted DB settings where applicable
4. defaults

Key variables:

| Variable | Purpose | Default |
| --- | --- | --- |
| `OPSBOX_DATABASE_URL` | Server DB path override | `~/.opsbox/opsbox.db` |
| `LOGSEEK_IO_MAX_CONCURRENCY` | Shared IO concurrency | `12` |
| `LOGSEEK_IO_TIMEOUT_SEC` | Shared IO timeout | `60` |
| `LOGSEEK_IO_MAX_RETRIES` | Shared IO retry count | `5` |
| `LOGSEEK_SERVER_ID` | Server ID for generated URLs | unset |
| `PUBLIC_API_BASE` | Frontend logseek base | `/api/v1/logseek` |
| `PUBLIC_AGENTS_API_BASE` | Frontend agent base | `/api/v1/agents` |
| `OPSBOX_NO_PROXY` | Disable proxy for reqwest clients | unset |

## Coding Conventions

- backend layering in `logseek`: routes -> service -> repository/domain/utils
- module registration uses `opsbox_core::register_module!`
- `opsbox-server/src/main.rs` must explicitly `extern crate` optional modules so inventory registration survives release linking
- frontend API wrappers should mirror backend routes closely
- use ORL terminology, not the older ODFI/FileUrl naming, unless updating historical code/comments

## Common Tasks

### Add a new server module

1. Create a crate under `backend/`
2. Implement `opsbox_core::Module`
3. Register with `opsbox_core::register_module!`
4. Add it as an optional dependency in `backend/opsbox-server/Cargo.toml`
5. Explicitly reference the crate in `backend/opsbox-server/src/main.rs`

### Add a frontend-backed API

1. Add backend route in the module router
2. Add frontend API client under the corresponding `web/src/lib/modules/*/api`
3. Expose it from that module's `index.ts`
4. Cover with unit tests and, if user-visible, E2E

## Useful References

- `docs/README.md`
- `docs/architecture/architecture.md`
- `docs/guides/query-syntax.md`
- `docs/guides/frontend-development.md`
- `docs/modules/agent-api-spec.md`
- `backend/logseek/src/planners/README.md`
- `web/static/query-syntax.md`

Last updated: 2026-03-20
