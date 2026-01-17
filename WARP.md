# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

Project overview
- Monorepo with a Rust backend and a SvelteKit (Vite) frontend.
- Backend (backend/): Cargo workspace with crates:
  - opsbox-server (dir: opsbox-server): Main binary serving as the HTTP server entry point. Dynamically composes modules discovered via opsbox-core's Module inventory (e.g., logseek, agent-manager, explorer).
  - opsbox-core: Shared library providing unified error handling (AppError), database management (SQLite pool), standard response formats, a pluggable Module system (inventory-based), LLM abstraction (Ollama/OpenAI), and ORL protocol (`orl://`) for unified resource addressing.
  - logseek: Module library exposing router() and init_schema() for log search over local agents and S3-compatible object stores, settings persistence (S3 profiles, LLM backends), planners, and NL→Q using the unified LLM client.
  - explorer: Module library for distributed resource browsing across Local, S3, and Agent endpoints with archive navigation and file download support (API prefix /api/v1/explorer).
  - agent-manager: Module library for agent registry/health/tags (API prefix /api/v1/agents); auto-registered via inventory.
  - opsbox-agent (dir: agent): Standalone agent binary for live/local log access used by LogSeek (optional in deployments).
- Frontend (web/): SvelteKit app compiled to static assets directly into backend/opsbox-server/static using adapter-static with SPA fallback.

Toolchains and prerequisites
- Rust: pinned via rust-toolchain.toml to 1.90.0 with clippy and rustfmt components.
- Node: prefer Node 22. Use pnpm via corepack. If you manage Node with nvm: nvm use 22.
- pnpm: enable through corepack (corepack enable; corepack prepare pnpm@10.23.0 --activate) or install pnpm globally.

Common commands
- Set repo root (useful when not in project root)
  - ROOT=$(git rev-parse --show-toplevel)
- Install frontend deps
  - corepack enable; corepack prepare pnpm@10.17.1 --activate
  - pnpm --dir $ROOT/web install
- Run backend (dev)
  - cargo run --manifest-path $ROOT/backend/Cargo.toml -p opsbox-server --
  - Options (opsbox-server):
    - --host/-H (default 127.0.0.1), --port/-P (default 4000), or --addr/-a HOST:PORT
    - --log-level error|warn|info|debug|trace or -v/-vv for verbosity
    - Subcommands (macOS/Linux): start [--daemon] [--pid-file FILE], stop [--pid-file FILE] [--force]
  - Health check: curl http://127.0.0.1:4000/healthy
- Run frontend (dev)
  - pnpm --dir $ROOT/web dev
  - Vite proxy forwards /api → http://127.0.0.1:4000
- Build frontend (outputs to backend/opsbox-server/static and will clear that directory)
  - pnpm --dir $ROOT/web build
  - Note: This will clear $ROOT/backend/opsbox-server/static before building
- Build backend (release)
  - cargo build --manifest-path $ROOT/backend/Cargo.toml -p opsbox-server --release         # bin: opsbox-server
  - cargo build --manifest-path $ROOT/backend/Cargo.toml -p opsbox-agent --release   # bin: opsbox-agent
- Lint and format
  - Rust format (check): cargo fmt --manifest-path $ROOT/backend/Cargo.toml --all -- --check
  - Rust format (write): cargo fmt --manifest-path $ROOT/backend/Cargo.toml --all
  - Rust lint: cargo clippy --manifest-path $ROOT/backend/Cargo.toml --workspace --all-targets -- -D warnings
  - Frontend format: pnpm --dir $ROOT/web format
  - Frontend lint: pnpm --dir $ROOT/web lint
- Tests
  - Rust (workspace): cargo test --manifest-path $ROOT/backend/Cargo.toml
  - Rust (lib only): cargo test --manifest-path $ROOT/backend/Cargo.toml -p logseek
  - Rust (single test): cargo test --manifest-path $ROOT/backend/Cargo.toml -p logseek <test_name>
  - Frontend (all unit tests): pnpm --dir $ROOT/web test
  - Frontend (watch): pnpm --dir $ROOT/web test:unit
  - Frontend (single test by name): pnpm --dir $ROOT/web test:unit -- -t "name"
  - Frontend (single file): pnpm --dir $ROOT/web exec vitest run path/to/file.test.ts

Key runtime configuration
- All application settings are persisted in a unified SQLite database (default file: $HOME/.opsbox/opsbox.db). Override path via --database-url CLI flag or OPSBOX_DATABASE_URL/DATABASE_URL environment variables (accepts a filesystem path or sqlite:// URL).
- Database structure:
  - Managed by opsbox-core with automatic migrations
  - LogSeek module tables include s3_profiles (stores default and named S3 profiles)
  - Each module registers its schema via init_schema() during startup
- Settings API (served by opsbox-server under /api/v1/logseek):
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
    - GET /settings/llm/backends/{name}/models, POST /settings/llm/models
    - GET/POST /settings/llm/default

Architecture (modular design)
- backend/opsbox-server (main binary, output: opsbox-server)
  - Modular structure with clean separation of concerns:
    - main.rs: Entry point, CLI parsing, initialization orchestration
    - config.rs: Configuration management (CLI args, env vars, defaults)
    - logging.rs: Logging setup (RUST_LOG, --log-level, -v/-vv)
    - daemon.rs: Unix daemon support (start/stop with PID management)
    - server.rs: HTTP server composition (dynamic module router aggregation, CORS, embedded static, SPA fallback, graceful shutdown)
    - network.rs: Network environment sanity
  - Discovers modules via opsbox-core inventory and nests each module router at its api_prefix
  - Embeds SPA via rust-embed from backend/opsbox-server/static
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
    - /settings/llm/backends (GET/POST), /settings/llm/backends/{name} (DELETE)
    - /settings/llm/backends/{name}/models (GET), /settings/llm/models (POST)
    - /settings/llm/default (GET/POST)
    - /settings/planners/scripts (GET/POST), /settings/planners/scripts/{app} (GET/DELETE)
    - /settings/planners/test (POST), /settings/planners/readme (GET)
    - /nl2q: natural language → query
  - repository/: persistence for settings, LLM backends, planners, and in-memory cache
  - utils/: renderer, storage (S3), tuning (concurrency/timeouts)
  - query/: GitHub-like query language parser

- Frontend (web)
  - SvelteKit SPA built with adapter-static to backend/opsbox-server/static (pages+assets) and fallback index.html
  - Vite dev server proxies /api to opsbox-server
  - Vitest projects for browser (Svelte components) and node (server utilities)
  - Modular architecture under web/src/lib/modules/:
    - logseek/: types, api (search, settings, nl2q, view), utils (highlight.ts), composables (useSearch, useSettings, useStreamReader), components
    - agent/: agent management APIs and composables

Conventions and notes
- Align with CI toolchain versions when possible: Rust 1.90.0 and Node 22.
- Frontend build will delete and repopulate backend/opsbox-server/static. Rebuild frontend whenever UI changes must be reflected in the embedded binary.
- opsbox-server embeds static assets at compile time; after changing UI, you must rebuild the backend to ship updated assets.

Performance benchmarking (NDJSON)
- Purpose: measure end-to-end throughput and observe the adaptive CPU guard while streaming NDJSON from S3/MinIO.
- Script: scripts/test/bench-ndjson.sh
  - What it does:
    - Restarts opsbox-server with a given IO/S3 concurrency limit
    - Runs a long test at conc=16 (default LONG_SECS=120) and optionally exports adaptive logs to a CSV in $HOME
    - Runs shorter tests at conc=8,12,16 (default SHORT_SECS=30) and prints a Markdown summary (lines, duration, avg tput)
  - Usage (macOS/Linux):
    - bash $ROOT/scripts/test/bench-ndjson.sh
    - JEMALLOC_AGGRESSIVE=1 bash $ROOT/scripts/test/bench-ndjson.sh  # enable jemalloc aggressive reclaim preset
  - Tunables via env vars (see scripts/README.md and the script header for details):
    - QUERY_JSON (default: {"q":"error fdt:20250816 tdt:20250822"})
    - ADDR (default: 127.0.0.1:4000)
    - S3_MAX_CONC (default: 12)
    - S3_TIMEOUT (default: 60)
    - S3_RETRIES (default: 5)
    - CONC_SERIES (default: 8,12,16)
    - LONG_SECS (default: 120)
    - SHORT_SECS (default: 30)
    - JEMALLOC_AGGRESSIVE: if set to 1/true/yes and MALLOC_CONF is unset, applies MALLOC_CONF=background_thread:true,dirty_decay_ms:0,muzzy_decay_ms:0
    - MALLOC_CONF: if set, takes precedence for jemalloc tuning (e.g., background_thread:true,dirty_decay_ms:100,muzzy_decay_ms:100)
    - BIN_PATH, LOG_PATH to override binary/log locations
- Output:
  - CSV: ~/adaptive_${LONG_SECS}s_conc16.csv by default (e.g., ~/adaptive_120s_conc16.csv; columns: time_iso,target,effective,err_rate_percent,tp_per_s)
  - Markdown table printed to terminal summarizing lines/duration/avg throughput
- For local debugging of API without embedding UI, run frontend dev server with proxy and run opsbox-server in dev.
- The streaming API path is /api/v1/logseek/search.ndjson. The opsbox-server CLI exposes IO tuning flags --io-max-concurrency, --io-timeout-sec, and --io-max-retries; use these (or corresponding env vars LOGSEEK_IO_MAX_CONCURRENCY/LOGSEEK_IO_TIMEOUT_SEC/LOGSEEK_IO_MAX_RETRIES) when running opsbox-server directly.
