# Technology Stack

**Analysis Date:** 2026-03-13

## Languages

**Primary:**
- Rust 2024 edition - Backend (all server logic, modules, DFS)
- TypeScript 5.x + Svelte 5 - Frontend (SvelteKit SPA)
- Starlark 0.13 - Planner DSL for intelligent source planning

## Runtime

**Environment:**
- Tokio 1.x - Backend async runtime
- Node.js - Frontend build/dev

**Package Manager:**
- Cargo (workspace resolver v3) - Rust
- pnpm 10.23.0 - Node.js

**Lockfile:**
- `Cargo.lock` - present
- `pnpm-lock.yaml` - present

## Frameworks

**Core:**
- Axum 0.8.4 - HTTP server framework (backend)
- Tower 0.5/0.6 - Middleware and service abstractions
- SvelteKit 2.22 (SPA via adapter-static) - Frontend framework
- TailwindCSS 4.0 - CSS utility framework (`@tailwindcss/vite` plugin)

**Testing:**
- Vitest 3.2.3 - Frontend testing (dual browser+node environments)
- Playwright 1.57 (Chromium) - Browser-based tests
- cargo test + cargo-llvm-cov - Backend testing and coverage

**Build/Dev:**
- Vite 7.0 - Frontend bundler and dev server
- rust-embed - Embeds frontend assets into Rust binary
- corepack - Node.js package manager management

## Key Dependencies

**Critical Backend:**
- sqlx 0.8 (SQLite bundled via libsqlite3-sys) - Database access
- reqwest 0.12 (rustls-tls) - HTTP client for LLM/agent communication
- aws-sdk-s3 1.15 - S3/MinIO integration
- starlark 0.13 - Scriptable source planning
- tracing 0.1 - Structured logging ecosystem
- inventory 0.3 - Compile-time module discovery
- grep-regex/grep-searcher/grep-matcher - Byte-level regex search
- encoding_rs + chardetng - Character encoding detection (GBK support)
- lru 0.16.2 - LRU cache implementation
- mimalloc - Global allocator with memory collection

**Critical Frontend:**
- @tanstack/svelte-virtual 3.13 - Virtual scrolling for large lists
- lucide-svelte 0.554 + @tabler/icons-svelte 3.35 - Icon sets
- marked 17.0 - Markdown rendering
- bits-ui 2.14.4 - UI component primitives
- mode-watcher 1.1 - Dark mode support

**Infrastructure:**
- async-tar/tokio-tar/async-zip - Archive support (tar, tar.gz, zip)
- chrono-tz 0.8 - Timezone support (Beijing)
- fluent-uri 0.4.1 - RFC 3986 compliant URI parsing
- urlencoding 2.1.3 - URL encoding

## Configuration

**Priority:** CLI flags > env vars > DB settings > defaults

**Key Environment Variables:**
- `OPSBOX_DATABASE_URL` / `DATABASE_URL` - Custom database path
- `LLM_PROVIDER` - LLM provider (`ollama` or `openai`)
- `OLLAMA_BASE_URL` - Ollama server URL (default: `http://127.0.0.1:11434`)
- `OPENAI_API_KEY` - OpenAI API key
- `OPSBOX_NO_PROXY=1` - Disable proxy for tests on macOS

## Platform Requirements

**Development:**
- Rust 2024 toolchain
- Node.js with corepack
- pnpm 10.23.0

**Production:**
- Single binary deployment (frontend embedded via rust-embed)
- Bundled SQLite (no external database required)
- Default data directory: `$HOME/.opsbox/`
- Unix daemon + Windows service support

---

*Stack analysis: 2026-03-13*
