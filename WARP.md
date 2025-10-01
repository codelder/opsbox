# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

Project overview
- Monorepo with a Rust backend and a SvelteKit (Vite) frontend.
- Backend (server/): Cargo workspace with three crates:
  - api-gateway: Main binary (named opsbox) serving as the HTTP server entry point. Composes modular functionality from opsbox-core and logseek.
  - opsbox-core: Shared library providing unified error handling (AppError), database management (SQLite pool), and standard response formats.
  - logseek: Module library exposing router() and domain logic for log search (local files and S3/MinIO), settings persistence, and an NL→Q helper via local Ollama.
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
- All application settings are persisted in a unified SQLite database (default file: ./opsbox.db). Override path via --database-url CLI flag or DATABASE_URL environment variable (accepts a filesystem path or sqlite:// URL).
- Database structure:
  - Managed by opsbox-core with automatic migrations
  - LogSeek module tables: logseek_minio_config, logseek_settings (prefixed for isolation)
  - Each module registers its schema via init_schema() during startup
- Settings API (served by api-gateway under /api/v1/logseek):
  - GET /settings/minio → returns current settings plus configured + connection_error flags
  - POST /settings/minio with JSON { endpoint, bucket, access_key, secret_key }
    - Validates connectivity before persisting. Typical errors are returned as problem+json with Chinese titles/details.
- NL→Q (natural language to query) API: POST /nl2q with { nl: string }
  - Depends on a local Ollama instance. Defaults (from README):
    - OLLAMA_BASE_URL=http://127.0.0.1:11434
    - OLLAMA_MODEL=qwen3:8b
  - Returns { q: string }.

Architecture (modular design)
- server/api-gateway (main binary, output: opsbox)
  - Modular structure with clean separation of concerns:
    - main.rs: Entry point, CLI parsing, initialization orchestration
    - config.rs: Configuration management (CLI args, env vars, defaults)
    - logging.rs: Logging setup (RUST_LOG, --log-level, -V/-VV/-VVV)
    - daemon.rs: Unix daemon support (start/stop with PID management)
    - server.rs: HTTP server composition (router aggregation, CORS, static assets, graceful shutdown)
  - Composes functionality: health endpoint, mounts logseek::router() at /api/v1/logseek, serves SPA via embedded assets (rust-embed)
  - Database initialization: creates shared opsbox-core pool, runs module migrations
  - Configuration tuning: applies runtime parameters to modules (LogSeek concurrency, timeouts, etc.)

- server/opsbox-core (shared library)
  - error.rs: Unified error handling with AppError enum (Database, Config, Internal, BadRequest, NotFound, ExternalService)
    - Automatic logging based on severity
    - RFC 7807 Problem Details format for HTTP responses
  - database.rs: SQLite connection pool management
    - DatabaseConfig for connection parameters
    - init_pool() for pool creation with configurable limits
    - health_check() for connectivity validation
    - run_migration() helper for module schema registration
  - response.rs: Standard success response wrappers (ok, ok_message, created, no_content)
  - middleware/: Placeholder for future shared middleware (auth, metrics, etc.)

- server/logseek (module library)
  - Module interface:
    - router(db_pool) → Router: provides all LogSeek HTTP routes with database state
    - init_schema(db_pool) → Result<()>: registers logseek_ prefixed tables
  - routes.rs: HTTP endpoint definitions using opsbox-core types
    - /stream: markdown stream from local example; demonstration endpoint
    - /stream.ndjson: streams local filesystem results as NDJSON; includes a per-request session id (X-Logsearch-SID) and caches highlighted line slices
    - /stream.s3.ndjson: streams S3/MinIO search results as NDJSON over a configurable date range and bucket set; uses S3ReaderProvider
    - /view.cache.json: returns cached line ranges for a file within a session, including line numbers and keywords
    - /settings/minio (GET/POST): persistence and validation of MinIO connectivity using shared database pool
    - /nl2q (POST): calls local Ollama to derive query strings from natural language
  - search.rs: Core search execution with context windowing, boolean term evaluation (AND/OR/NOT, parentheses), regex support (including lookarounds via fancy-regex)
    - Directory traversal: concurrent file scanning with backpressure (Semaphore + JoinSet), path filtering
    - Streamed archive scanning: AsyncRead over gzip+tar via async-compression + async-tar; adapts futures AsyncRead to tokio via compat
  - storage.rs: ReaderProvider abstraction for local fs and S3/MinIO objects; error types mapped to opsbox-core::AppError
  - renderer.rs: Renders markdown blocks and JSON chunks with highlighted ranges for streaming endpoints
  - simple_cache.rs: In-memory cache keyed by session id (SID) for keywords and line slices
  - query module: Parser for GitHub-like query language (literals, phrases, regex, groups, negation, precedence) producing Search spec
  - settings.rs: Settings persistence using shared database pool
    - init_schema(): creates logseek_minio_config and logseek_settings tables with timestamps
    - load/save/verify functions accepting pool reference
    - Uses opsbox_core::AppError for unified error handling
  - nl2q.rs: Thin client around ollama-rs to transform natural language into query strings; honors OLLAMA_BASE_URL and OLLAMA_MODEL

- Frontend (ui)
  - SvelteKit SPA built with adapter-static to server/api-gateway/static (pages+assets) and fallback index.html
  - Vite dev server proxies /api to api-gateway
  - Vitest projects for browser (Svelte components) and node (server utilities)
  - Modular LogSeek architecture (ui/src/lib/modules/logseek/):
    - types/: Centralized TypeScript type definitions for API contracts, UI states, and utilities
    - api/: API client layer encapsulating all backend calls (search, settings, nl2q, view)
      - Unified error handling with RFC 7807 Problem Details support
      - Type-safe request/response handling
      - Chinese error messages for user-facing errors
    - utils/: Reusable utilities for text processing
      - highlight.ts: HTML escaping, keyword highlighting with <mark> tags, smart line truncation
    - composables/: Svelte 5 Runes-style state management
      - useStreamReader.svelte.ts: NDJSON stream batch reading with buffer management
      - useSearch.svelte.ts: Search state and lifecycle (start/cancel/loadMore)
      - useSettings.svelte.ts: MinIO settings CRUD with connection validation
    - components/: Placeholder for reusable UI components (future expansion)
  - Pages refactored to use modular APIs:
    - routes/+page.svelte: Home page uses convertNaturalLanguage() for NL→Q
    - routes/settings/+page.svelte: Settings page uses useSettings() composable
    - routes/search/+page.svelte: Search page uses modular types, APIs, and highlight utilities
    - routes/view/+page.svelte: View page uses fetchViewCache() and text processing utils
  - Benefits of modular frontend:
    - Clear separation: API client, state management, utilities, and UI
    - Type safety: Centralized TypeScript definitions prevent inconsistencies
    - Reusability: API and utilities shared across pages, reducing duplication
    - Maintainability: Each layer can be tested and modified independently
    - Modern patterns: Svelte 5 Runes for reactive state, composable logic for state encapsulation

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
