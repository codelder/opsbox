# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

OpsBox is a modular log search and analysis platform built with Rust backend and SvelteKit frontend. It features a pluggable architecture where modules are automatically discovered and registered at compile time.

### Technology Stack

- **Backend**: Rust 2024 edition with `tracing` ecosystem for logging
- **Frontend**: SvelteKit 5 with TypeScript, TailwindCSS 4.0
- **Database**: SQLite with automatic schema management
- **Build Tools**:
  - Rust: Cargo workspace
  - Node.js: pnpm 10.23.0, Vite
- **Font System**: Maple Mono NF CN for optimal Chinese code display
- **Version**: 0.1.0-rc10

### Core Architecture

- **Monorepo Structure**: Rust backend (`backend/`) + SvelteKit frontend (`web/`)
- **Modular Design**: Uses `opsbox-core` inventory system for automatic module discovery
- **Current Modules**:
  - `logseek`: Log search module supporting local files and S3/MinIO
  - `agent-manager`: Agent registry and management module
- **Embedded Frontend**: Static assets compiled into binary via `rust-embed`

### Backend Structure (`backend/`)

#### Workspace Crates
- **opsbox-server**: Main binary entry point (`src/main.rs`)
  - CLI configuration (`config.rs`)
  - HTTP server composition (`server.rs`)
  - Logging setup (`logging.rs`)
  - Daemon support (`daemon.rs`, `daemon_windows.rs`)
  - Network initialization (`network.rs`)

- **opsbox-core**: Shared library providing:
  - Unified error handling (`error.rs`) - RFC 7807 Problem Details
  - Database management (`database.rs`) - SQLite pool with migrations
  - Module system (`module.rs`) - inventory-based registration
  - LLM abstraction (`llm/`) - Ollama/OpenAI clients
  - Standard responses (`response.rs`)
  - Middleware utilities (`middleware/`)
  - Logging configuration (`logging/`)

- **logseek**: Log search module with layered architecture:
  - API layer (`routes/`, `api/`)
  - Service layer (`service/`)
  - Repository layer (`repository/`)
  - Domain layer (`domain/`, `domain/source_planner/`)
  - Source planners (`planners/`)
  - Utilities (`utils/`)
  - Query parser (`query/`)
  - Agent integration (`agent/`)

- **agent-manager**: Agent management module:
  - Agent registry and health monitoring (`manager.rs`)
  - Tag-based agent organization
  - Database repository (`repository.rs`)
  - API endpoints (`routes.rs`) under `/api/v1/agents`
  - Data models (`models.rs`)

- **agent**: Standalone agent binary for remote log access

### Frontend Structure (`web/`)

- **SvelteKit SPA** with `adapter-static`
- **Modular architecture** under `src/lib/modules/`:
  - `logseek/`: Types, API clients, utilities, composables, components
  - `agent/`: Agent management APIs and composables
- **Vite dev server** with proxy to backend (`/api` → `http://127.0.0.1:4000`)
- **Built assets** output to `backend/opsbox-server/static`
- **Font system**: Maple Mono NF CN font family for optimal Chinese code display
- **Performance optimizations**: Virtual scrolling and chunked loading for large files
- **File download**: Integrated download functionality with backend cache support

## Development Guidelines

### When Making Changes

1. **Backend Changes**:
   - Follow layered architecture patterns (API → Service → Repository)
   - Use `opsbox-core::AppError` for unified error handling
   - Leverage module system for new features (implement `Module` trait)
   - Database migrations handled automatically via `init_schema()`

2. **Frontend Changes**:
   - Place new functionality in appropriate module directory
   - Use Svelte 5 Runes for state management
   - API clients should match backend endpoints exactly
   - Rebuild frontend after changes to see in embedded binary

3. **Cross-cutting Concerns**:
   - Logging: Use `tracing` crate with appropriate levels
   - Configuration: CLI args → Environment variables → Defaults
   - Database: SQLite with automatic schema management
   - Error handling: Consistent Problem Details responses

### Key Conventions

- **API Prefixes**: Each module has its own prefix (`/api/v1/logseek`, `/api/v1/agents`)
- **Database**: Single SQLite file (`$HOME/.opsbox/opsbox.db`) shared across modules
- **LLM Integration**: Configurable via environment variables (`LLM_PROVIDER`, `OLLAMA_BASE_URL`, etc.)
- **S3 Profiles**: Multiple S3 configurations managed via profiles API
- **Agent Communication**: HTTP-based with health monitoring and tags

### Build and Test Commands

```bash
# Install dependencies
corepack enable
corepack prepare pnpm@10.23.0 --activate
pnpm --dir web install

# Run development
cargo run --manifest-path backend/Cargo.toml -p opsbox-server
pnpm --dir web dev

# Build production
pnpm --dir web build  # Builds to backend/opsbox-server/static
cargo build --manifest-path backend/Cargo.toml -p opsbox-server --release

# Testing
cargo test --manifest-path backend/Cargo.toml
pnpm --dir web test

# Frontend testing with specific environments
pnpm --dir web test:unit  # Run all tests
pnpm --dir web test:unit --run --project=client  # Browser tests only
pnpm --dir web test:unit --run --project=server  # Node.js tests only
```

### Configuration Priority

1. CLI flags (highest priority)
2. Environment variables
3. Database settings (for persistent config)
4. Default values (lowest priority)

### Important Notes

- **Frontend embedding**: After UI changes, rebuild backend to update embedded assets
- **Module registration**: New modules must implement `Module` trait and use `register_module!` macro
- **Database migrations**: Handled automatically, but schema changes require `init_schema` updates
- **Graceful shutdown**: Implemented for all modules via `cleanup()` method
- **Performance**: IO concurrency, timeouts, and retries configurable via environment variables
- **Font system**: Uses Maple Mono NF CN font family for optimal Chinese code display; font files are embedded in static assets
- **Large file handling**: Virtual scrolling and chunked loading implemented for files > 1000 lines
- **File download**: Full file download with backend cache support via `/view/download` endpoint
- **Gzip support**: Plain gzip files (non-tar) supported in directory and file targets
- **Testing configuration**: Dual test environments (browser + Node.js) with Playwright for browser tests
- **Development server**: Vite dev server configured for external access (0.0.0.0:5173)

## Recent Updates

- **Font migration**: Entire site migrated to Maple Mono NF CN font family for optimal Chinese code display
- **File download functionality**: Added full file download endpoint with backend cache support
- **Performance optimizations**: Implemented chunked loading and virtual scrolling for large files
- **Gzip file support**: Logseek module now supports plain gzip files in directory and file targets
- **Search text detection**: Improved search text detection with updated tests
- **Case-sensitive search rules**: Alignment of case-sensitive search rules between frontend and backend highlighting
- **UI improvements**: View page UI updated with sidebar removal and enhanced FileHeader design with color-coded metadata
- **Path parsing**: Refactored to use parseFileUrl utility
- **Settings reorganization**: Moved settings and theme toggle to individual pages for better UX
- **Font size control**: Added font size control (xs, sm, base, lg, xl) in view page

## Common Tasks

### Adding New Module
1. Create new crate in `backend/`
2. Implement `Module` trait with `name()`, `api_prefix()`, `configure()`, `init_schema()`, `router()`, `cleanup()`
3. Add to workspace in `backend/Cargo.toml`
4. Add optional dependency in `opsbox-server/Cargo.toml`
5. Add to default features if needed

### Adding API Endpoint
1. Backend: Add route in appropriate module's `routes/` directory
2. Frontend: Add corresponding API client in module's `api/` directory
3. Ensure proper error handling and response formatting

### Database Schema Changes
1. Update `init_schema()` function in module
2. Add migration logic if needed (though current system recreates tables)
3. Test with fresh database to ensure compatibility

### Configuration Changes
1. Add CLI argument in `opsbox-server/src/config.rs`
2. Add environment variable support in module's `configure()` method
3. Update documentation in README.md if user-facing