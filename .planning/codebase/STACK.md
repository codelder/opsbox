# Technology Stack

**Analysis Date:** 2026-03-13

## Languages

**Primary:**
- Rust (Edition 2024) - Backend: server, modules (logseek, explorer, agent-manager), agent binary, shared core
- TypeScript (strict mode) - Frontend: SvelteKit components, composables, API clients, utilities

**Secondary:**
- JavaScript (ESM) - Frontend configuration (vite.config.ts, svelte.config.js)
- Starlark 0.13 - Scriptable source planning in logseek module

## Runtime

**Backend Runtime:**
- Tokio 1.x (full features) - Async runtime for Axum HTTP server, IO operations, S3 clients
- mimalloc 0.1 - Global allocator (MiMalloc) for performance-critical memory management

**Frontend Runtime:**
- Browser (SPA) - SvelteKit with adapter-static, fallback to index.html
- Node.js - Server-side tests via Vitest

## Package Managers

**Rust:**
- Cargo (workspace resolver v3)
- Workspace root: `backend/Cargo.toml`
- 7 workspace members: opsbox-server, opsbox-core, logseek, agent, agent-manager, explorer, test-common

**Node.js:**
- pnpm 10.23.0 (via corepack)
- Lockfile: `web/pnpm-lock.yaml` (present)

## Frameworks

**Backend Web:**
- Axum 0.8 - HTTP framework (JSON support, tower integration)
- tower 0.5 / tower-http 0.6 - Middleware stack (CORS, tracing, static files)

**Frontend:**
- SvelteKit 2.22 with Svelte 5 - SPA framework
- adapter-static 3.x - Static output to `backend/opsbox-server/static`
- Vite 7.0 - Build tool / dev server

**CSS/Styling:**
- TailwindCSS 4.0 via `@tailwindcss/vite` plugin
- tailwind-variants 3.x - Component variant utilities
- tailwind-merge 3.x - Class merging
- clsx 2.x - Conditional class composition

**Testing:**
- Backend: cargo test (built-in), cargo-llvm-cov for coverage
- Frontend unit: Vitest 3.2 (browser + server projects, Playwright provider for browser)
- Frontend E2E: Playwright 1.57 (Chromium)

**Linting/Formatting:**
- Frontend: ESLint 9.x + Prettier 3.7 + prettier-plugin-svelte + prettier-plugin-tailwindcss

## Key Dependencies

**Backend - Core Infrastructure:**
- `sqlx 0.8` (SQLite, runtime-tokio-rustls) - Database access
- `libsqlite3-sys 0.30` (bundled) - Embedded SQLite (no system dependency)
- `reqwest 0.12` (rustls-tls, JSON, stream) - HTTP client for LLM, agents, external APIs
- `serde 1.0` / `serde_json 1.0` - Serialization
- `thiserror 2` - Error types
- `inventory 0.3` - Compile-time module registration

**Backend - Storage:**
- `aws-sdk-s3 1.15` - S3/MinIO client for object storage
- `async_zip 0.0.18` - Async ZIP archive support
- `tokio-tar 0.3.1` / `async-tar 0.5.1` / `tar 0.4` - Async TAR archive support
- `async-compression 0.4` (gzip) - Compression/decompression streams
- `flate2 1.x` - Synchronous gzip compression

**Backend - Text/Search:**
- `grep-regex 0.1` / `grep-searcher 0.1` / `grep-matcher 0.1` - Byte-level regex search
- `encoding_rs 0.8` - Text encoding conversion (GBK support)
- `chardetng 0.1` - Character encoding detection
- `fancy-regex 0.11` - Advanced regex with lookaround
- `aho-corasick 1.1` - Multi-pattern string matching
- `globset 0.4` - Glob pattern matching
- `memchr 2.7` - Fast byte searching

**Backend - Time/Date:**
- `chrono 0.4` (clock feature) - Date/time handling
- `chrono-tz 0.8` - Timezone support (Beijing timezone)

**Backend - Misc:**
- `lru 0.16.2` - LRU cache implementation
- `once_cell 1.20` - Lazy static initialization
- `num_cpus 1.x` - CPU core detection for concurrency tuning
- `starlark 0.13` - Scriptable source planning runtime
- `fluent-uri 0.4.1` - RFC 3986 URI parsing (ORL protocol)
- `rust-embed 8` - Embed frontend static assets in binary
- `clap 4` - CLI argument parsing
- `urlencoding 2.1.3` - URL encoding utilities

**Backend - Platform-specific:**
- Unix: `daemonize 0.5`, `nix 0.30` (signal handling)
- Windows: `windows-service 0.8`

**Frontend - UI:**
- `bits-ui 2.14` - Headless UI components
- `lucide-svelte 0.554` / `@tabler/icons-svelte 3.35` - Icon libraries
- `@tanstack/svelte-virtual 3.13` - Virtual scrolling for large lists
- `marked 17` - Markdown rendering
- `mode-watcher 1.1` - Dark mode support
- `uri-js 4.4` - URI parsing/manipulation (ORL protocol on frontend)

## Configuration

**Environment Variables (backend runtime):**
- `LLM_PROVIDER` - LLM provider selection (ollama/openai)
- `OLLAMA_BASE_URL`, `OLLAMA_MODEL`, `OLLAMA_TIMEOUT_SECS` - Ollama config
- `OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_MODEL`, `OPENAI_TIMEOUT_SECS`, `OPENAI_ORG`, `OPENAI_PROJECT` - OpenAI config
- `OPSBOX_DATABASE_URL` / `DATABASE_URL` - SQLite database path (default: `$HOME/.opsbox/opsbox.db`)
- `OPSBOX_NO_PROXY` - Disable proxy detection (required for LLM tests on macOS)
- `OPSBOX_IO_TIMEOUT_SEC` / `LOGSEEK_IO_TIMEOUT_SEC` - IO timeout (default: 60s, range: 5-300s)
- `OPSBOX_IO_MAX_RETRIES` - IO retry count (default: 5)
- `OPSBOX_IO_MAX_CONCURRENCY` - IO concurrency (default: 12)
- `LOGSEEK_SERVER_ID` - Server identifier
- `RUST_LOG` - Log level filter (tracing)
- `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` / `NO_PROXY` - Proxy configuration
- `CI` - CI environment flag (disables proxy, adjusts timeouts)

**CLI Arguments (opsbox-server):**
- `--host`, `--port`, `--addr` - Network binding
- `--daemon` - Run as daemon (Unix)
- `--log-level`, `-v` - Log verbosity
- `--log-dir`, `--log-retention` - Log file management
- `--database-url` - Database path override
- `--io-max-concurrency`, `--io-timeout-sec`, `--io-max-retries` - IO tuning
- `--server-id` - Server identification

**Build Config:**
- `backend/Cargo.toml` - Workspace configuration
- `web/vite.config.ts` - Vite + TailwindCSS + Vitest
- `web/svelte.config.js` - SvelteKit + adapter-static
- `web/tsconfig.json` - TypeScript (strict, bundler module resolution)
- `web/.prettierrc` - Prettier (single quotes, no trailing commas, 120 width)
- `web/playwright.config.ts` - E2E testing

## Platform Requirements

**Development:**
- Rust toolchain (Edition 2024 compatible)
- Node.js + corepack (for pnpm 10.23.0)
- SQLite (bundled via libsqlite3-sys, no system install needed)
- For LLM tests: `OPSBOX_NO_PROXY=1` required on macOS

**Production:**
- Single binary deployment (frontend embedded via rust-embed)
- SQLite database at `$HOME/.opsbox/opsbox.db`
- Unix: daemon mode supported
- Windows: Windows Service support
- Cross-compilation target: `x86_64-unknown-linux-musl` (agent binary)

---

*Stack analysis: 2026-03-13*
