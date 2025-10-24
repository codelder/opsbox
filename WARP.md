# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

Project overview
- Monorepo with a Rust backend and a SvelteKit (Vite) frontend.
- Backend (backend/): Cargo workspace with crates:
  - api-gateway: Main binary (named opsbox) serving as the HTTP server entry point. Dynamically composes modules discovered via opsbox-core's Module inventory (e.g., logseek, agent-manager).
  - opsbox-core: Shared library providing unified error handling (AppError), database management (SQLite pool), standard response formats, a pluggable Module system (inventory-based), and LLM abstraction (Ollama/OpenAI).
  - logseek: Module library exposing router() and init_schema() for log search over local agents and S3-compatible object stores, settings persistence (S3 profiles, LLM backends), and NL→Q using the unified LLM client.
  - agent: Standalone agent binary for live/local log access used by LogSeek (optional in deployments).
- Frontend (web/): SvelteKit app compiled to static assets directly into backend/api-gateway/static using adapter-static with SPA fallback.

Toolchains and prerequisites
- Rust: pinned via rust-toolchain.toml to 1.90.0 with clippy and rustfmt components.
- Node: prefer Node 20. Use pnpm via corepack. If you manage Node with nvm: nvm use 20.
- pnpm: enable through corepack (corepack enable; corepack prepare pnpm@latest --activate) or install pnpm globally.

Common commands
- Install frontend deps
  - corepack enable; corepack prepare pnpm@latest --activate
  - pnpm --dir web install
- Run backend (dev)
  - cargo run --manifest-path backend/Cargo.toml -p api-gateway --
  - Options (api-gateway):
    - --host/-H (default 127.0.0.1), --port/-P (default 4000), or --addr/-a HOST:PORT
    - --log-level error|warn|info|debug|trace or -v/-vv for verbosity
    - Subcommands (macOS/Linux): start [--daemon] [--pid-file FILE], stop [--pid-file FILE] [--force]
  - Health check: curl http://127.0.0.1:4000/healthy
- Run frontend (dev)
  - pnpm --dir web dev
  - Vite proxy forwards /api → http://127.0.0.1:4000
- Build frontend (outputs to backend/api-gateway/static and will clear that directory)
  - node scripts/build-frontend.mjs
  - or: bash scripts/build-frontend.sh (Unix only)
- Build backend (release)
  - cargo build --manifest-path backend/Cargo.toml -p api-gateway --release
- Lint and format
  - Rust format (check): cargo fmt --all -- --check
  - Rust format (write): cargo fmt --all
  - Rust lint: cargo clippy --workspace --all-targets -- -D warnings
  - Frontend format: pnpm --dir web format
  - Frontend lint: pnpm --dir web lint
- Tests
  - Rust (workspace): cargo test
  - Rust (lib only): cargo test -p logseek
  - Rust (single test): cargo test -p logseek <test_name>
  - Frontend (all unit tests): pnpm --dir web test
  - Frontend (watch): pnpm --dir web test:unit
  - Frontend (single test by name): pnpm --dir web test:unit -- -t "name"
  - Frontend (single file): pnpm --dir web exec vitest run path/to/file.test.ts

Key runtime configuration
- All application settings are persisted in a unified SQLite database (default file: ~/.opsbox/opsbox.db). Override path via --database-url CLI flag or OPSBOX_DATABASE_URL/DATABASE_URL environment variables (accepts a filesystem path or sqlite:// URL).
- Database structure:
  - Managed by opsbox-core with automatic migrations
  - LogSeek module tables include s3_profiles (stores default and named S3 profiles)
  - Each module registers its schema via init_schema() during startup
- Settings API (served by api-gateway under /api/v1/logseek):
  - GET /settings/s3?verify=true|false → returns current settings with configured flag and optional connection_error when verify=true
  - POST /settings/s3 with JSON { endpoint, bucket, access_key, secret_key }
    - Validates connectivity before persisting. Typical errors are returned as problem+json with Chinese titles/details.
  - S3 Profiles: GET/POST /profiles, DELETE /profiles/{name}
- LLM and NL→Q
  - NL→Q API: POST /nl2q with { nl: string } → returns { q: string }
  - LLM provider is selectable via env:
    - LLM_PROVIDER=ollama|openai
    - For Ollama: OLLAMA_BASE_URL (default http://127.0.0.1:11434), OLLAMA_MODEL (default qwen3:8b)
    - For OpenAI: OPENAI_API_KEY (required), OPENAI_MODEL (default gpt-4o-mini), OPENAI_BASE_URL (optional)
  - LLM backend settings endpoints (under /api/v1/logseek):
    - GET/POST /settings/llm/backends, DELETE /settings/llm/backends/{name}
    - GET/POST /settings/llm/default

Architecture (modular design)
- backend/api-gateway (main binary, output: opsbox)
  - Modular structure with clean separation of concerns:
    - main.rs: Entry point, CLI parsing, initialization orchestration
    - config.rs: Configuration management (CLI args, env vars, defaults)
    - logging.rs: Logging setup (RUST_LOG, --log-level, -v/-vv)
    - daemon.rs: Unix daemon support (start/stop with PID management)
    - server.rs: HTTP server composition (dynamic module router aggregation, CORS, embedded static, SPA fallback, graceful shutdown)
    - network.rs: Network environment sanity
  - Discovers modules via opsbox-core inventory and nests each module router at its api_prefix
  - Embeds SPA via rust-embed from backend/api-gateway/static
  - Database initialization: creates shared opsbox-core pool, runs module migrations

- backend/opsbox-core (shared library)
  - error.rs: Unified error handling with AppError enum (Database, Config, Internal, BadRequest, NotFound, ExternalService) and RFC 7807 responses
  - database.rs: SQLite pool (init_pool, health_check) and run_migration helper
  - response.rs: Standard success response helpers (ok, ok_with_message, created, no_content)
  - module.rs: Module trait, inventory-based registration, register_module! macro, get_all_modules()
  - llm/: Unified LLM client with Ollama/OpenAI implementations

- backend/logseek (module library)
  - Module interface:
    - router(db_pool) → Router: provides all LogSeek HTTP routes with database state
    - init_schema(db_pool) → Result<()>: registers module tables (e.g., s3_profiles)
  - routes/:
    - /search.ndjson: streamed search (NDJSON)
    - /view.cache.json: cached line ranges retrieval
    - /settings/s3 (GET/POST): S3 settings with optional verify
    - /profiles (GET/POST), /profiles/{name} (DELETE)
    - /settings/llm/*: LLM backend management
    - /nl2q: natural language → query
  - repository/: persistence for settings, LLM backends, and in-memory cache
  - utils/: renderer, storage (S3), tuning (concurrency/timeouts)
  - query/: GitHub-like query language parser

- Frontend (web)
  - SvelteKit SPA built with adapter-static to backend/api-gateway/static (pages+assets) and fallback index.html
  - Vite dev server proxies /api to api-gateway
  - Vitest projects for browser (Svelte components) and node (server utilities)
  - Modular architecture under web/src/lib/modules/:
    - logseek/: types, api (search, settings, nl2q, view), utils (highlight.ts), composables (useSearch, useSettings, useStreamReader), components
    - agent/: agent management APIs and composables

Conventions and notes
- Align with CI toolchain versions when possible: Rust 1.90.0 and Node 20.
- Frontend build will delete and repopulate backend/api-gateway/static. Rebuild frontend whenever UI changes must be reflected in the embedded binary.
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
