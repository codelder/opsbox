# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

Project overview
- Monorepo with a Rust backend and a SvelteKit (Vite) frontend.
- Backend (server/): Cargo workspace with two crates:
  - api-gateway: Axum HTTP server that nests logseek APIs under /api/v1/logseek and serves the built SPA from embedded static assets.
  - logseek: Library exposing router() and domain logic for log search (local files and S3/MinIO), settings persistence, and an NL→Q helper via local Ollama.
- Frontend (ui/): SvelteKit app compiled to static assets directly into server/api-gateway/static using adapter-static with SPA fallback.

Toolchains and prerequisites
- Rust: pinned via rust-toolchain.toml to 1.90.0 with clippy and rustfmt components.
- Node: CI uses Node 20. Use pnpm via corepack. If you manage Node with nvm: nvm use 20.
- pnpm: enable through corepack (corepack enable; corepack prepare pnpm@latest --activate) or install pnpm globally.

Common commands
- Install frontend deps
  - corepack enable; corepack prepare pnpm@latest --activate
  - pnpm --dir ui install
- Run backend (dev)
  - cargo run --manifest-path server/Cargo.toml -p api-gateway --
  - Options (api-gateway):
    - --host 0.0.0.0, --port 4000, or --addr 0.0.0.0:4000
    - --log-level error|warn|info|debug|trace or -V/-VV for verbosity
    - Subcommands (macOS/Linux): start [--daemon] [--pid-file FILE], stop [--pid-file FILE] [--force]
  - Health check: curl http://127.0.0.1:4000/healthy
- Run frontend (dev)
  - pnpm --dir ui dev
  - Vite proxy forwards /api → http://127.0.0.1:4000
- Build frontend (outputs to server/api-gateway/static and will clear that directory)
  - node scripts/build-frontend.mjs
  - or: bash scripts/build-frontend.sh (Unix only)
- Build backend (release)
  - cargo build --manifest-path server/Cargo.toml -p api-gateway --release
- Lint and format
  - Rust format (check): cargo fmt --all -- --check
  - Rust format (write): cargo fmt --all
  - Rust lint: cargo clippy --workspace --all-targets -- -D warnings
  - Frontend format: pnpm --dir ui format
  - Frontend lint: pnpm --dir ui lint
- Tests
  - Rust (workspace): cargo test
  - Rust (lib only): cargo test -p logseek
  - Rust (single test): cargo test -p logseek <test_name>
  - Frontend (all unit tests): pnpm --dir ui test
  - Frontend (watch): pnpm --dir ui test:unit
  - Frontend (single test by name): pnpm --dir ui test:unit -- -t "name"
  - Frontend (single file): pnpm --dir ui vitest run path/to/file.test.ts

Key runtime configuration
- MinIO/S3 settings are persisted in a local SQLite DB (default file: ./logseek_settings.db). Override path via LOGSEARCH_SETTINGS_DB (accepts a filesystem path or sqlite:// URL).
- Settings API (served by api-gateway under /api/v1/logseek):
  - GET /settings/minio → returns current settings plus configured + connection_error flags
  - POST /settings/minio with JSON { endpoint, bucket, access_key, secret_key }
    - Validates connectivity before persisting. Typical errors are returned as problem+json with Chinese titles/details.
- NL→Q (natural language to query) API: POST /nl2q with { nl: string }
  - Depends on a local Ollama instance. Defaults (from README):
    - OLLAMA_BASE_URL=http://127.0.0.1:11434
    - OLLAMA_MODEL=qwen3:8b
  - Returns { q: string }.

Logsearch architecture (big picture)
- server/api-gateway (binary)
  - Composes the HTTP app: health endpoint, mounts logseek::router() at /api/v1/logseek, and serves SPA via embedded assets (rust-embed). Provides SPA fallback so deep-linked routes resolve to index.html. Offers daemonization (Unix) and adjustable logging via env_logger.
- server/logseek (library)
  - routes.rs: Defines HTTP endpoints:
    - /stream: markdown stream from local example; demonstration endpoint.
    - /stream.ndjson: streams local filesystem results as NDJSON; includes a per-request session id (X-Logsearch-SID) and caches highlighted line slices for later retrieval via /view.cache.json.
    - /stream.s3.ndjson: streams S3/MinIO search results as NDJSON over a configurable date range and bucket set; uses S3ReaderProvider.
    - /view.cache.json: returns cached line ranges for a file within a session, including line numbers and keywords.
    - /settings/minio (GET/POST): persistence and validation of MinIO connectivity.
    - /nl2q (POST): calls local Ollama to derive query strings from natural language.
  - search.rs: Core search execution over text inputs with context windowing, boolean term evaluation (AND/OR/NOT, parentheses), regex support (including lookarounds via fancy-regex when needed), and text/binary heuristic filtering. Supports two async search modes:
    - Directory traversal: concurrent file scanning with backpressure (Semaphore + JoinSet), path filtering, and per-file context merging.
    - Streamed archive scanning: AsyncRead over gzip+tar via async-compression + async-tar; adapts futures AsyncRead to tokio via compat.
  - storage.rs: ReaderProvider abstraction and implementations for local fs and S3/MinIO objects; error types mapped to friendly messages used in HTTP layer.
  - renderer.rs: Renders markdown blocks and JSON chunks with highlighted ranges; used by streaming endpoints.
  - simple_cache.rs: In-memory cache keyed by session id (SID) for keywords and line slices; supports view cache API.
  - query module: Parser for the GitHub-like query language (literals, phrases, regex, groups, negation, precedence) producing Search spec with path filtering and highlight terms.
  - settings.rs: SQLite-backed settings store using sqlx with bundled libsqlite3; provides ensure_store(), load/save/verify helpers; returns AppError variants mapped to RFC7807 problem details with Chinese messages.
  - nl2q.rs: Thin client around ollama-rs to transform natural language into query strings; honors OLLAMA_BASE_URL and OLLAMA_MODEL.
- Frontend (ui)
  - SvelteKit SPA built with adapter-static to server/api-gateway/static (pages+assets) and fallback index.html; Vite dev server proxies /api to api-gateway. Vitest projects for browser (Svelte components) and node (server utilities).

Conventions and notes
- Align with CI toolchain versions when possible: Rust 1.90.0 and Node 20.
- Frontend build will delete and repopulate server/api-gateway/static. Rebuild frontend whenever UI changes must be reflected in the embedded binary.
- api-gateway embeds static assets at compile time; after changing UI, you must rebuild the backend to ship updated assets.

Performance benchmarking (NDJSON)
- Purpose: measure end-to-end throughput and observe the adaptive CPU guard while streaming NDJSON from S3/MinIO.
- Script: scripts/bench-ndjson.sh
  - What it does:
    - Restarts api-gateway with given CPU concurrency limit
    - Runs a 120s test at CPU=16 and exports adaptive logs to a CSV in $HOME
    - Runs 30s tests at CPU=8,12,16 and prints a Markdown summary (lines, duration, avg tput)
  - Usage (macOS/Linux):
    - bash scripts/bench-ndjson.sh
    - JEMALLOC_AGGRESSIVE=1 bash scripts/bench-ndjson.sh  # enable jemalloc aggressive reclaim preset
  - Tunables via env vars:
    - QUERY_JSON (default: {"q":"error fdt:20250816 tdt:20250819"})
    - ADDR (default: 127.0.0.1:4000)
    - WORKER_THREADS (default: 16)
    - S3_MAX_CONC (default: 12)
    - STREAM_CH_CAP (default: 256)
    - MINIO_TIMEOUT (default: 60)
    - MINIO_RETRIES (default: 5)
    - CPU_SERIES (default: 8,12,16)
    - JEMALLOC_AGGRESSIVE: if set to 1/true/yes and MALLOC_CONF is unset, applies MALLOC_CONF=background_thread:true,dirty_decay_ms:0,muzzy_decay_ms:0
    - MALLOC_CONF: if set, takes precedence for jemalloc tuning (e.g., background_thread:true,dirty_decay_ms:100,muzzy_decay_ms:100)
    - BIN_PATH, LOG_PATH to override binary/log locations
- Output:
  - CSV: ~/adaptive_120s_cpu16.csv (columns: time_iso,target,effective,err_rate_percent,tp_per_s)
  - Markdown table printed to terminal summarizing lines/duration/avg throughput
- For local debugging of API without embedding UI, run frontend dev server with proxy and run api-gateway in dev.
