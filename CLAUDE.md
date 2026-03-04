# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

OpsBox is a modular log search and analysis platform built with Rust backend and SvelteKit frontend. It features a pluggable architecture where modules are automatically discovered and registered at compile time. The platform provides unified resource browsing across local files, S3/MinIO storage, and remote agents.

### Technology Stack

- **Backend**: Rust 2024 edition with `tracing` ecosystem for logging, `mimalloc` as global allocator
- **Frontend**: SvelteKit 2.22 with TypeScript, TailwindCSS 4.0 (`@tailwindcss/vite` plugin)
- **Database**: SQLite with automatic schema management
- **Build Tools**:
  - Rust: Cargo workspace (resolver v3)
  - Node.js: pnpm 10.23.0, Vite 7.0
- **Font System**: Maple Mono NF CN (5 font weights, ~31MB embedded)
- **Version**: 0.1.1

**Key Backend Dependencies:**
- `starlark = "0.13"` - Scriptable source planning
- `chrono-tz = "0.8"` - Timezone support (Beijing)
- `chardetng = "0.1"` - Character encoding detection
- `grep-regex = "0.1"` - Byte-level regex search (split into grep-regex, grep-searcher, grep-matcher)
- `grep-searcher = "0.1"` - Search functionality
- `grep-matcher = "0.1"` - Pattern matching
- `urlencoding = "2.1.3"` - URL encoding
- `fluent-uri = "0.4.1"` - RFC 3986 compliant URI parsing
- `async_zip = "0.0.18"` - Async ZIP archive support
- `tokio-tar = "0.3.1"` - Async TAR archive support
- `async-tar = "0.5.1"` - Additional TAR support
- `reqwest = "0.12"` - HTTP client (used for LLM and agent communication)
- `encoding_rs = "0.8"` - Text encoding conversion (GBK support)
- `lru = "0.16.2"` - LRU cache implementation
- `futures-lite = "2.6.1"` - Stream utilities
- `tokio-stream = "0.1.17"` - Stream handling
- `aws-sdk-s3 = "1.15"` - AWS S3 SDK

**Key Frontend Dependencies:**
- `@tanstack/svelte-virtual = "^3.13.12"` - Virtual scrolling
- `lucide-svelte = "^0.554.0"` - Icons
- `@tabler/icons-svelte = "3.35"` - Additional icon set
- `marked = "^17.0.0"` - Markdown rendering
- `bits-ui = "^2.14.4"` - UI components
- `mode-watcher = "^1.1.0"` - Dark mode watcher

### Core Architecture

- **Monorepo Structure**: Rust backend (`backend/`) + SvelteKit frontend (`web/`)
- **Modular Design**: Uses `opsbox-core` inventory system for automatic module discovery
- **ORL Protocol**: Unified resource identifier scheme for cross-endpoint resource addressing (evolved from ODFI, uses `orl://` scheme)
- **Current Modules**:
  - `logseek`: Log search module supporting local files, S3/MinIO, and archives
  - `explorer`: Distributed file/resource browser across Local, S3, and Agent endpoints with file download support
  - `agent-manager`: Agent registry and management module
- **Embedded Frontend**: Static assets compiled into binary via `rust-embed`

### Backend Structure (`backend/`)

#### Workspace Members
- `opsbox-server` - Main binary
- `opsbox-core` - Shared library (includes DFS subsystem)
- `logseek` - Log search module
- `agent-manager` - Agent management module
- `explorer` - Resource browser module
- `agent` - Standalone agent binary
- `test-common` - Shared test utilities

#### Workspace Crates
- **opsbox-server**: Main binary entry point (`src/main.rs`)
  - CLI options: `--host`, `--port`, `--addr`, `--daemon`, `--log-level`, `-v`, `--log-dir`, `--log-retention`, `--database-url`, `--io-max-concurrency`, `--io-timeout-sec`, `--io-max-retries`, `--server-id`
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
  - Filesystem utilities (`fs/`) - Archive streaming, compression detection
  - Storage abstraction (`storage/`) - S3 repository and utilities
  - Agent client (`agent/`) - HTTP client for agent communication
  - **DFS subsystem (`dfs/`)** - Distributed FileSystem abstraction layer including ORL parsing

- **logseek**: Log search module with layered architecture:
  - API layer (`routes/`, `api.rs`) - Dual layer pattern for backward compatibility
  - Service layer (`service/`) including:
    - `search.rs` - Search core module
    - `search/sink.rs` - Search result sink
    - `search_executor.rs` - Search orchestration
    - `search_runner.rs` - Search execution runner
    - `searchable.rs` - Searchable resource trait
    - `resource_orl.rs` - Resource ORL handling
    - `encoding.rs` - GBK and multi-encoding detection
    - `entry_stream.rs` - Archive streaming for 25KB+ files
    - `nl2q.rs` - Natural language to query conversion
    - `config.rs` - Source/Endpoint/Target models (includes ORL URL construction utilities)
  - Repository layer (`repository/`) including:
    - `cache.rs` - Search result caching
    - `llm.rs` - LLM backend management
    - `planners.rs` - Planner script persistence
    - `s3.rs` - S3 profile persistence
  - Domain layer (`domain/`) including:
    - `source_planner/` - Starlark runtime for intelligent source planning
  - Source planners (`planners/`)
  - Utilities (`utils/`)
  - Query parser (`query/`)
  - Agent integration (`agent/`)
  - Byte-level regex search using `grep` crate

- **explorer**: Distributed resource browser module:
  - Resource listing API (`routes.rs`) - POST `/api/v1/explorer/list`
  - File download API - POST `/api/v1/explorer/download`
  - Unified browsing across Local, S3, and Agent endpoints
  - Archive navigation (tar, tar.gz, gzip, tgz, zip)
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
  - `logseek/`:
    - `api/`: API clients (nl2q, planners, llm, search, view, settings)
    - `types/`: TypeScript type definitions
    - `composables/`: Svelte composables
    - `components/`: Svelte components
  - `agent/`: Agent management APIs and composables
  - `explorer/`: File explorer UI, API client, grid/list views
- **Routes**:
  - `/`: Home page
  - `/search`: Log search interface
  - `/search/SearchEmptyState.svelte`: Empty state component
  - `/search/SearchResultCard.svelte`: Search result card component
  - `/view`: File viewer with FileHeader component
  - `/image-view`: Image viewing page
  - `/explorer`: Distributed file explorer
  - `/settings`: Settings page
  - `/settings/PlannerManagement.svelte`: Planner script management UI
  - `/settings/LlmManagement.svelte`: LLM backend configuration UI
  - `/settings/ProfileManagement.svelte`: S3 profile management UI
  - `/settings/AgentManagement.svelte`: Agent management UI
  - `/settings/ServerLogSettings.svelte`: Server log settings UI
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

## Test Coverage

### Backend (Rust)
- **Total Tests**: 1,031 tests (99.7% pass rate)
- **Coverage Tool**: `cargo-llvm-cov`
- **Test Requirements**: Requires `OPSBOX_NO_PROXY=1` for LLM module tests
- **Coverage Status**: Comprehensive unit and integration tests across all modules
- **Estimated Coverage**: ~75-80% overall

#### Test Distribution by Module
| Module | Unit Tests | Integration Tests | Total |
|--------|------------|-------------------|-------|
| logseek | 413 | 55 | 468 |
| opsbox-core | 73 | 206 | 279 |
| agent | 10 | 144 | 154 |
| explorer | 17 | 9 | 26 |
| agent-manager | 11 | 11 | 22 |
| opsbox-server | 27 | - | 27 |
| test-common | 20 | - | 20 |

### Frontend (TypeScript/Svelte)
- **Total Tests**: 95 tests (100% pass rate)
- **Server Tests**: 55 passing (Node.js environment)
- **Browser Tests**: 40 passing (Chromium environment)
- **Coverage Thresholds**: Set to 70% lines/functions/statements, 60% branches
- **Current Coverage**: 14.85% overall

#### High Coverage Areas (>80%)
- **ORL Utils**: 92.77% lines, 81.08% branches
- **Highlight Utils**: 83.33% lines, 83.33% branches
- **Explorer API**: 88.57% lines
- **UI Components**: 84-100% (alert, badge, button, card, input, label, switch)

**Note**: Overall frontend coverage is low due to untested route components and composables. Key utilities and API clients have excellent coverage (>80%).

### Recent Test Additions (2026-02-27)

**Iteration 1 - High Risk Areas:**
- **Explorer Integration Tests**: 9 new tests (local files, agent files, archive navigation)
- **DFS Integration Tests**: 5 new tests (archive combinations)
- **Frontend API Client Tests**: 8 new tests (Explorer API)
- **UI Component Tests**: 21 new tests (Agent Management, Server Log Settings)

**Coverage Improvements:**
- Explorer: 0 → 9 integration tests
- DFS (opsbox-core): 0 → 5 integration tests
- Frontend: 55 → 95 tests (+40 tests)

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
- **LLM Integration**: Configurable via environment variables (`LLM_PROVIDER`, `OLLAMA_BASE_URL`, etc.) and database-persistent backends
  - **Proxy Detection**: `reqwest` automatically detects system proxy settings; use `OPSBOX_NO_PROXY=1` to disable (required for testing on macOS)
- **S3 Profiles**: Multiple S3 configurations managed via profiles API
- **Agent Communication**: HTTP-based with health monitoring and tags
- **Query Qualifiers**:
  - `app:<appname>` - Select planner script by application name for intelligent source planning
  - `dt:/fdt:/tdt:` - Date/time directives for time-range filtering in queries
- **ORL Protocol**: Unified resource identifiers in format `orl://[id]@[type][.server_addr]/[path]?entry=[entry_path]` (evolved from ODFI)
  - Local: `orl://local/var/log/nginx/access.log`
  - Agent: `orl://web-01@agent/app/logs/error.log`
  - S3 Archive: `orl://prod@s3/logs/2023/10/data.tar.gz?entry=internal/service.log`

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

# Testing (backend requires OPSBOX_NO_PROXY=1 for LLM tests)
OPSBOX_NO_PROXY=1 cargo test --manifest-path backend/Cargo.toml
pnpm --dir web test

# Frontend testing with specific environments
pnpm --dir web test:unit  # Run all tests
pnpm --dir web test:unit --run --project=client  # Browser tests only
pnpm --dir web test:unit --run --project=server  # Node.js tests only

# Backend code coverage (requires cargo-llvm-cov)
OPSBOX_NO_PROXY=1 cargo llvm-cov --manifest-path backend/Cargo.toml --workspace --lcov
```

### Build Options and Feature Flags

**Module Features:**
```bash
# Default build (includes all modules)
cargo build --manifest-path backend/Cargo.toml -p opsbox-server

# Build specific modules only
cargo build -p opsbox-server --no-default-features -F logseek,agent-manager

# Available features:
# - logseek: Log search module (with mimalloc-collect for memory collection)
# - agent-manager: Agent registry and management
# - explorer: Distributed file/resource browser
```

**LogSeek Sub-features:**
```bash
# Enable mimalloc memory collection on cache cleanup
cargo build -p logseek --features mimalloc-collect

# Enable network-dependent tests
cargo test -p logseek --features network-tests
```

### Configuration Priority

1. CLI flags (highest priority)
2. Environment variables
3. Database settings (for persistent config)
4. Default values (lowest priority)

### Environment Variables

**Testing:**
- `OPSBOX_NO_PROXY=1` - Disable `reqwest` system proxy detection (required for LLM tests on macOS)
- `CI=1` - CI environment indicator (also disables proxy detection)

**LLM Configuration:**
- `LLM_PROVIDER` - LLM provider (`ollama` or `openai`, default: `ollama`)
- `OLLAMA_BASE_URL` - Ollama server URL (default: `http://127.0.0.1:11434`)
- `OLLAMA_MODEL` - Default Ollama model (default: `qwen3:8b`)
- `OLLAMA_TIMEOUT_SECS` - Ollama request timeout in seconds
- `OPENAI_API_KEY` - OpenAI API key (required for OpenAI provider)
- `OPENAI_BASE_URL` - OpenAI-compatible API URL (default: `https://api.openai.com`)
- `OPENAI_MODEL` - Default OpenAI model (default: `gpt-4o-mini`)
- `OPENAI_TIMEOUT_SECS` - OpenAI request timeout in seconds
- `OPENAI_ORG` - OpenAI organization ID
- `OPENAI_PROJECT` - OpenAI project ID

**IO & Network Configuration:**
- `LOGSEEK_IO_TIMEOUT_SEC` - IO timeout in seconds (default: 60)
- `LOGSEEK_IO_MAX_RETRIES` - Maximum IO retry attempts (default: 5)
- `LOGSEEK_IO_MAX_CONCURRENCY` - Maximum IO concurrency (default: 12)

**S3 Proxy Configuration:**
- `HTTP_PROXY` / `http_proxy` - HTTP proxy URL
- `HTTPS_PROXY` / `https_proxy` - HTTPS proxy URL
- `ALL_PROXY` / `all_proxy` - All traffic proxy URL
- `NO_PROXY` / `no_proxy` - Comma-separated list of proxy bypass hosts

**Database & Server:**
- `OPSBOX_DATABASE_URL` / `DATABASE_URL` - Custom database path
- `LOGSEEK_SERVER_ID` - Unique server identifier
- `LOG_DIR` - Custom log directory path
- `LOG_RETENTION` - Log retention period in days

**Development Server:**
- `VITE_HOST` - Vite dev server host (default: `0.0.0.0`)
- `BACKEND_PORT` - Backend API port for proxy (default: `4000`)

**Claude Code Integration:**
- `CLAUDECODE` / `CLAUDE_CODE_ENTRYPOINT` - Detects running in Claude Code (affects sandbox behavior)

### Important Notes

- **LLM Testing**: Backend tests require `OPSBOX_NO_PROXY=1` environment variable to prevent `reqwest` from accessing macOS System Configuration (causes NULL object errors in test environment)
- **Frontend embedding**: After UI changes, rebuild backend to update embedded assets
- **Module registration**: New modules must implement `Module` trait and use `register_module!` macro
- **Database migrations**: Handled automatically, but schema changes require `init_schema` updates
- **Graceful shutdown**: Implemented for all modules via `cleanup()` method
- **Performance**: IO concurrency, timeouts, and retries configurable via environment variables
- **Font system**: Uses Maple Mono NF CN font family (5 weights: ExtraLight, Regular, Medium, SemiBold, Bold)
- **Large file handling**: Virtual scrolling and chunked loading implemented for files > 1000 lines
- **File download**: Full file download with backend cache support via `/view/download` endpoint
- **Archive support**: Tar, tar.gz, gzip, tgz, zip with auto-detection and deep navigation
- **Encoding detection**: GBK and multi-encoding support using `chardetng` and `encoding_rs`
- **Testing configuration**: Dual test environments (browser + Node.js) with Playwright for browser tests
- **Development server**: Vite dev server configured for external access (0.0.0.0:5173)
- **Memory management**: `mimalloc` as global allocator with explicit memory collection on cache cleanup
- **ORL protocol**: Use `orl://` scheme for resource identifiers (migrated from `odfi://` with backward compatibility)

## ORL Protocol (OpsBox Resource Locator)

The ORL protocol provides a unified URI scheme for addressing resources across different storage backends. It evolved from the earlier ODFI (OpsBox Distributed File Identifier) protocol and uses the `orl://` scheme.

### Format

```
orl://[id]@[type][.server_addr]/[path]?entry=[entry_path]
```

### Components

- **id**: Resource identifier (e.g., S3 profile name, agent name, or "local")
- **type**: Endpoint type - `local`, `agent`, or `s3`
- **server_addr**: Optional server address with port (for agents)
- **path**: Resource path within the endpoint
- **entry**: Optional archive entry path for navigating inside archives

### Examples

```
orl://local/var/log/nginx/access.log
orl://web-01@agent.192.168.1.100:4001/app/logs/error.log
orl://prod@s3/bucket/logs/2023/10/data.tar.gz?entry=internal/service.log
```

**Note**: The legacy `odfi://` scheme may still appear in some parts of the codebase for compatibility, but new code should use `orl://`.

## DFS Subsystem (Distributed FileSystem)

The DFS (Distributed FileSystem) subsystem is a unified abstraction layer for accessing resources across different storage backends. It provides a consistent interface for local files, S3 buckets, and remote agents.

**Location:** `backend/opsbox-core/src/dfs/`

**Components:**
- `endpoint.rs` - Endpoint abstraction (Local, S3, Agent)
- `filesystem.rs` - Filesystem trait definition
- `orl_parser.rs` - ORL URL parser
- `path.rs` - Path manipulation utilities
- `resource.rs` - Resource abstraction
- `searchable.rs` - Searchable resource trait
- `impls/` - Backend implementations:
  - `local.rs` - Local filesystem access
  - `s3.rs` - S3 object storage
  - `agent.rs` - Remote agent access
  - `archive.rs` - Archive file access (tar, zip, etc.)

**Features:**
- Unified resource access across Local/S3/Agent endpoints
- Archive deep navigation (tar, tar.gz, gzip, tgz, zip)
- ORL-based resource addressing
- Content-type detection

## API Endpoints

### LogSeek Module (`/api/v1/logseek`)

**Search:**
- `POST /api/v1/logseek/search.ndjson` - Stream search results in NDJSON format
- `DELETE /api/v1/logseek/search/session/{sid}` - Delete/cancel search session

**View:**
- `GET /api/v1/logseek/view.cache.json` - Get cached view data
- `GET /api/v1/logseek/view/download` - Download file
- `GET /api/v1/logseek/view/raw` - View raw file content
- `GET /api/v1/logseek/view.files.json` - List files in directory/archive

**S3 Profiles:**
- `GET /api/v1/logseek/profiles` - List S3 profiles
- `POST /api/v1/logseek/profiles` - Create/update S3 profile
- `DELETE /api/v1/logseek/profiles/{name}` - Delete S3 profile

**S3 Settings:**
- `GET /api/v1/logseek/settings/s3` - Get S3 settings
- `POST /api/v1/logseek/settings/s3` - Update S3 settings

**LLM Backends:**
- `GET /api/v1/logseek/settings/llm/backends` - List LLM backends
- `POST /api/v1/logseek/settings/llm/backends` - Create LLM backend
- `DELETE /api/v1/logseek/settings/llm/backends/{name}` - Delete LLM backend
- `GET /api/v1/logseek/settings/llm/backends/{name}/models` - List models for backend

**LLM Models:**
- `POST /api/v1/logseek/settings/llm/models` - Add/remove available models

**LLM Default:**
- `GET /api/v1/logseek/settings/llm/default` - Get default LLM backend
- `POST /api/v1/logseek/settings/llm/default` - Set default LLM backend

**Planner Scripts:**
- `GET /api/v1/logseek/settings/planners/scripts` - List planner scripts
- `POST /api/v1/logseek/settings/planners/scripts` - Save planner script
- `GET /api/v1/logseek/settings/planners/scripts/{app}` - Get script by app name
- `DELETE /api/v1/logseek/settings/planners/scripts/{app}` - Delete script
- `POST /api/v1/logseek/settings/planners/test` - Test planner script
- `GET /api/v1/logseek/settings/planners/readme` - Get planner documentation
- `GET /api/v1/logseek/settings/planners/default` - Get default planner
- `POST /api/v1/logseek/settings/planners/default` - Set default planner

**NL2Q (Natural Language to Query):**
- `POST /api/v1/logseek/nl2q` - Convert natural language to query syntax

### Agent Manager Module (`/api/v1/agents`)

**Agent Registry:**
- `POST /api/v1/agents/register` - Register new agent
- `GET /api/v1/agents/` - List all agents
- `GET /api/v1/agents/tags` - List all tags
- `GET /api/v1/agents/{agent_id}` - Get agent details
- `DELETE /api/v1/agents/{agent_id}` - Remove agent

**Heartbeat:**
- `POST /api/v1/agents/{agent_id}/heartbeat` - Agent heartbeat

**Tag Management:**
- `GET /api/v1/agents/{agent_id}/tags` - Get agent tags
- `POST /api/v1/agents/{agent_id}/tags` - Set agent tags
- `POST /api/v1/agents/{agent_id}/tags/add` - Add tags
- `DELETE /api/v1/agents/{agent_id}/tags/remove` - Remove tags
- `DELETE /api/v1/agents/{agent_id}/tags/clear` - Clear all tags

**Log Configuration (Agent Proxy):**
- `GET /api/v1/agents/{agent_id}/log/config` - Get agent log config
- `PUT /api/v1/agents/{agent_id}/log/level` - Set agent log level
- `PUT /api/v1/agents/{agent_id}/log/retention` - Set agent log retention

### Explorer Module (`/api/v1/explorer`)

- `POST /api/v1/explorer/list` - List resources (Local/S3/Agent)
- `GET /api/v1/explorer/download?orl=...` - Download file via query parameter
- POST method is not supported (use GET with query param)

### System Log Routes (`/api/v1/log`)

- `GET /api/v1/log/config` - Get log configuration
- `PUT /api/v1/log/level` - Set log level
- `PUT /api/v1/log/retention` - Set log retention

### Health Check

- `GET /healthy` - Health check endpoint (returns "ok")

## Source Planning with Starlark

LogSeek supports scriptable source planning using Starlark for intelligent log source selection.

**Location:** `backend/logseek/src/domain/source_planner/`

**Features:**
- Script-based source configuration with injected context variables
- Dynamic date range parsing with `dt:/fdt:/tdt:` directives
- Agent tag filtering capabilities
- S3 profile integration

**Context Variables Available to Scripts:**
- `CLEANED_QUERY`: Query with date directives removed
- `TODAY`: Current date in YYYY-MM-DD format (Beijing timezone)
- `DATE_RANGE`: Dict with `start` and `end` dates
- `DATES`: List of daily objects with `iso`, `yyyymmdd`, `next_yyyymmdd`
- `AGENTS`: List of online agents with their tags
- `S3_PROFILES`: List of configured S3 profiles (non-sensitive fields)

**Query Qualifier:**
- `app:<appname>` - Select planner script by application name

## Natural Language to Query (NL2Q)

Convert natural language queries to LogSeek query syntax using LLM.

**Endpoint:** `POST /api/v1/logseek/nl2q`

**Request:**
```json
{"nl": "查找最近的错误日志"}
```

**Response:**
```json
{"q": "error AND level:error"}
```

**Features:**
- System prompt with query syntax guide
- Automatic cleanup of LLM thinking tags
- Support for Ollama and OpenAI providers
- Database-persistent LLM backend configuration

## Recent Updates

- **DFS Subsystem**: New Distributed FileSystem abstraction layer in `opsbox-core` for unified resource access across Local/S3/Agent endpoints
- **Test Infrastructure**: 950 passing backend tests with `OPSBOX_NO_PROXY=1` requirement for LLM module
- **LLM Test Fix**: Fixed `reqwest` proxy detection issues in test environment (macOS System Configuration access failures)
- **Starlark Source Planning**: Scriptable source planning with intelligent log source selection using Starlark scripts
- **NL2Q (Natural Language to Query)**: Convert natural language queries to LogSeek syntax using LLM
- **Search Session Management**: Support for cancelling running searches via session IDs
- **Starlark Runtime**: Context variables (CLEANED_QUERY, TODAY, DATE_RANGE, AGENTS, S3_PROFILES) injected into scripts
- **Planner Script Management UI**: Full CRUD interface for managing planner scripts
- **LLM Backend Management**: Database-persistent LLM backend configuration with Ollama/OpenAI support
- **Agent Tag Management**: Full tag CRUD operations (add/remove/clear) for agent organization
- **System Log Routes**: API endpoints for configuring server log level and retention
- **Explorer module**: Distributed file/resource browser supporting Local, S3, and Agent endpoints
- **ORL protocol**: Unified resource identifier scheme for cross-endpoint addressing (evolved from ODFI, uses `orl://` scheme)
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
- **SearchExecutor refactor**: Overhauled SearchExecutor and simplified EntryStream creation for better performance
- **Relative path glob filtering**: Support for relative path glob patterns in search queries
- **Enhanced archive support**: Added async ZIP archive support with `async_zip` and `tokio-tar` dependencies
- **Test infrastructure improvements**: Full E2E test suite fixes, increased timeouts, and test coverage analysis
- **ORL protocol migration**: Transition from ODFI (`odfi://`) to ORL (`orl://`) scheme with backward compatibility
- **Explorer file download**: Complete file download implementation for the distributed resource browser
- **Performance optimizations**: Memory management improvements and search cancellation enhancements

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

## Troubleshooting

### Backend Tests Fail with "Attempted to create a NULL object"

**Symptom:** LLM tests fail with `system-configuration` library errors on macOS

**Cause:** `reqwest` tries to access macOS System Configuration for proxy detection in test environment

**Solution:** Set `OPSBOX_NO_PROXY=1` environment variable
```bash
OPSBOX_NO_PROXY=1 cargo test --manifest-path backend/Cargo.toml
```

### Frontend Browser Tests Fail with "EPERM: operation not permitted"

**Symptom:** Vitest browser tests fail with port permission errors

**Cause:** Port 63315 (or similar) is already in use or requires elevated permissions

**Solution:**
1. Kill processes using the port: `lsof -ti:63315 | xargs kill -9`
2. Or run server tests only: `pnpm test:unit --run --project=server`

### Agent Connection Refused

**Symptom:** Cannot connect to registered agents

**Cause:** Agent's `host` and `listen_port` tags not set correctly

**Solution:**
1. Check agent registration includes correct IP/port
2. Verify agent's `listen_port` tag matches actual listening port
3. Check network connectivity between server and agent

### Database Locked Errors

**Symptom:** SQLite database is locked errors

**Cause:** Multiple processes trying to write to database simultaneously

**Solution:**
1. Ensure only one opsbox-server instance is running
2. Check for orphaned connections: `lsof | grep opsbox.db`
3. Delete wal/shm files if needed: `rm ~/.opsbox/opsbox.db-wal ~/.opsbox/opsbox.db-shm`

### S3 Connection Timeout

**Symptom:** S3 operations timeout

**Cause:** Network issues or incorrect proxy configuration

**Solution:**
1. Disable proxy: `OPSBOX_NO_PROXY=1`
2. Or set correct proxy: `HTTP_PROXY=http://proxy:8080`
3. Increase timeout: `OPSBOX_IO_TIMEOUT_SEC=60`

### High Memory Usage

**Symptom:** Memory usage grows over time

**Cause:** LRU cache not being cleaned up

**Solution:**
1. Build with mimalloc collection: `cargo build --features mimalloc-collect`
2. Manually trigger cache cleanup via API
3. Adjust cache size in source code if needed