# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

OpsBox is a modular log search and analysis platform built with Rust backend and SvelteKit frontend. It features a pluggable architecture where modules are automatically discovered and registered at compile time.

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

- **logseek**: Log search module with layered architecture:
  - API layer (`routes/`, `api/`)
  - Service layer (`service/`)
  - Repository layer (`repository/`)
  - Domain layer (`domain/`)
  - Utilities (`utils/`)
  - Query parser (`query/`)
  - Agent integration (`agent/`)

- **agent-manager**: Agent management module:
  - Agent registry and health monitoring
  - Tag-based agent organization
  - API endpoints under `/api/v1/agents`

- **agent**: Standalone agent binary for remote log access

### Frontend Structure (`web/`)

- **SvelteKit SPA** with `adapter-static`
- **Modular architecture** under `src/lib/modules/`:
  - `logseek/`: Types, API clients, utilities, composables, components
  - `agent/`: Agent management APIs and composables
- **Vite dev server** with proxy to backend (`/api` → `http://127.0.0.1:4000`)
- **Built assets** output to `backend/opsbox-server/static`

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
corepack prepare pnpm@10.17.1 --activate
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