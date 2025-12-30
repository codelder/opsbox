# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

OpsBox is a modular log search and analysis platform built with Rust backend and SvelteKit frontend. It features a pluggable architecture where modules are automatically discovered and registered at compile time. The platform provides unified resource browsing across local files, S3/MinIO storage, and remote agents.

### Technology Stack

- **Backend**: Rust 2024 edition with `tracing` ecosystem for logging, `mimalloc` as global allocator
- **Frontend**: SvelteKit 2.22 with TypeScript, TailwindCSS 4.0 (`@tailwindcss/vite` plugin)
- **Database**: SQLite with automatic schema management
- **Build Tools**:
  - Rust: Cargo workspace
  - Node.js: pnpm 10.23.0, Vite 7.0
- **Font System**: Maple Mono NF CN (5 font weights, ~31MB embedded)
- **Version**: 0.1.1

### Core Architecture

- **Monorepo Structure**: Rust backend (`backend/`) + SvelteKit frontend (`web/`)
- **Modular Design**: Uses `opsbox-core` inventory system for automatic module discovery
- **ODFI Protocol**: Unified resource identifier scheme for cross-endpoint resource addressing
- **Current Modules**:
  - `logseek`: Log search module supporting local files, S3/MinIO, and archives
  - `explorer`: Distributed file/resource browser across Local, S3, and Agent endpoints
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
  - ODFI protocol (`odfi.rs`) - Unified resource identifier scheme
  - Filesystem utilities (`fs/`) - Archive streaming, compression detection
  - Storage abstraction (`storage/`) - S3 repository and utilities
  - Agent client (`agent/`) - HTTP client for agent communication

- **logseek**: Log search module with layered architecture:
  - API layer (`routes/`, `api/`)
  - Service layer (`service/`)
  - Repository layer (`repository/`)
  - Domain layer (`domain/`, `domain/source_planner/`)
  - Source planners (`planners/`)
  - Utilities (`utils/`)
  - Query parser (`query/`)
  - Agent integration (`agent/`)
  - Encoding support (`service/encoding.rs`) - GBK and multi-encoding detection
  - Byte-level regex search using `grep` crate

- **explorer**: Distributed resource browser module:
  - Resource listing API (`routes.rs`) - POST `/api/v1/explorer/list`
  - Unified browsing across Local, S3, and Agent endpoints
  - Archive navigation (tar, tar.gz, gzip, tgz)
  - Auto-detection of archive files
  - Content-based file type detection via MIME types
  - Hidden file counting and metadata
  - Directory child count tracking

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
  - `explorer/`: File explorer UI, API client, grid/list views
- **Routes**:
  - `/`: Home page
  - `/search`: Log search interface
  - `/view`: File viewer with FileHeader component
  - `/image-view`: Image viewing page
  - `/explorer`: Distributed file explorer
  - `/settings`: Settings page
  - `/prompt`: Prompt configuration
- **Vite dev server** with proxy to backend (`/api` → `http://127.0.0.1:4000`)
- **Built assets** output to `backend/opsbox-server/static`
- **Font system**: Maple Mono NF CN font family (5 weights embedded)
- **Performance optimizations**:
  - Virtual scrolling via `@tanstack/svelte-virtual`
  - Perfect Scrollbar integration
  - Chunked loading for large files (>1000 lines)
- **File download**: Integrated download functionality with backend cache support
- **UI Features**: macOS-style aesthetics, context menus, dark mode support

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

- **API Prefixes**: Each module has its own prefix (`/api/v1/logseek`, `/api/v1/agents`, `/api/v1/explorer`)
- **Database**: Single SQLite file (`$HOME/.opsbox/opsbox.db`) shared across modules
- **LLM Integration**: Configurable via environment variables (`LLM_PROVIDER`, `OLLAMA_BASE_URL`, etc.)
- **S3 Profiles**: Multiple S3 configurations managed via profiles API
- **Agent Communication**: HTTP-based with health monitoring and tags
- **ODFI Protocol**: Unified resource identifiers in format `odfi://[id]@[type][.server_addr]/[path]?entry=[entry_path]`
  - Local: `odfi://local/var/log/nginx/access.log`
  - Agent: `odfi://web-01@agent/app/logs/error.log`
  - S3 Archive: `odfi://prod@s3/logs/2023/10/data.tar.gz?entry=internal/service.log`

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
- **Font system**: Uses Maple Mono NF CN font family (5 weights: ExtraLight, Regular, Medium, SemiBold, Bold)
- **Large file handling**: Virtual scrolling and chunked loading implemented for files > 1000 lines
- **File download**: Full file download with backend cache support via `/view/download` endpoint
- **Archive support**: Tar, tar.gz, gzip, tgz with auto-detection and deep navigation
- **Encoding detection**: GBK and multi-encoding support using `chardetng` and `encoding_rs`
- **Testing configuration**: Dual test environments (browser + Node.js) with Playwright for browser tests
- **Development server**: Vite dev server configured for external access (0.0.0.0:5173)
- **Memory management**: `mimalloc` as global allocator with explicit memory collection on cache cleanup

## ODFI Protocol

The ODFI (OpsBox Distributed File Identifier) protocol provides a unified URI scheme for addressing resources across different storage backends.

### Format

```
odfi://[id]@[type][.server_addr]/[path]?entry=[entry_path]
```

### Components

- **id**: Resource identifier (e.g., S3 profile name, agent name, or "local")
- **type**: Endpoint type - `local`, `agent`, or `s3`
- **server_addr**: Optional server address with port (for agents)
- **path**: Resource path within the endpoint
- **entry**: Optional archive entry path for navigating inside archives

### Examples

```
odfi://local/var/log/nginx/access.log
odfi://web-01@agent.192.168.1.100:4001/app/logs/error.log
odfi://prod@s3/bucket/logs/2023/10/data.tar.gz?entry=internal/service.log
```

## Recent Updates

- **Explorer module**: New distributed file/resource browser supporting Local, S3, and Agent endpoints
- **ODFI protocol**: Unified resource identifier scheme for cross-endpoint addressing
- **Archive browsing**: Deep navigation into tar, tar.gz, gzip, tgz archives with auto-detection
- **S3 archive support**: Browse and view files inside S3-hosted archives
- **Encoding detection**: GBK and multi-encoding support with automatic detection
- **Byte-level search**: Fast regex search using `grep` crate with search cancellation support
- **Image viewer**: New `/image-view` route for viewing images
- **macOS-style UI**: Explorer with engraved folder icons and context menus
- **Memory optimization**: `mimalloc` allocator with explicit collection on cache cleanup
- **Content-based file detection**: MIME type detection using `infer` crate
- **Font migration**: Entire site migrated to Maple Mono NF CN font family (5 weights embedded)
- **TailwindCSS 4.0**: Upgraded with `@tailwindcss/vite` plugin and `@theme` directive
- **Virtual scrolling**: Performance optimization via `@tanstack/svelte-virtual`
- **File download functionality**: Full file download with backend cache support
- **Settings reorganization**: Moved settings and theme toggle to individual pages

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