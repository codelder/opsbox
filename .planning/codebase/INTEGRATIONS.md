# External Integrations

**Analysis Date:** 2026-03-13

## APIs & External Services

**LLM Providers (via `opsbox-core` LLM abstraction):**
- **Ollama** - Local LLM inference server for NL2Q (natural language to query) and source planning
  - SDK/Client: `reqwest` (HTTP), custom `OllamaClient` in `backend/opsbox-core/src/llm.rs`
  - Auth: None (local server)
  - Default endpoint: `http://127.0.0.1:11434`
  - Default model: `qwen3:8b`
  - Env vars: `OLLAMA_BASE_URL`, `OLLAMA_MODEL`, `OLLAMA_TIMEOUT_SECS`

- **OpenAI** - Cloud LLM inference for NL2Q and source planning
  - SDK/Client: `reqwest` (HTTP), custom `OpenAIClient` in `backend/opsbox-core/src/llm.rs`
  - Auth: API key via `OPENAI_API_KEY` env var
  - Default endpoint: `https://api.openai.com`
  - Default model: `gpt-4o-mini`
  - Env vars: `OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_MODEL`, `OPENAI_TIMEOUT_SECS`, `OPENAI_ORG`, `OPENAI_PROJECT`
  - Supports OpenAI-compatible APIs via `OPENAI_BASE_URL` override

- **LLM Abstraction Layer**: `LlmClient` trait with `DynLlmClient` type alias
  - LLM backends configurable via database (persistent across restarts)
  - LLM backends also configurable via environment variables (for initial setup)
  - Provider selection via `LLM_PROVIDER` env var or database setting

**S3 / Object Storage (via `aws-sdk-s3`):**
- **Amazon S3** - Primary cloud object storage
  - SDK/Client: `aws-sdk-s3 1.15` in `backend/opsbox-core/src/storage/s3.rs`
  - Auth: Access key + Secret key per S3 profile (stored in database)
  - Profile-based multi-endpoint support

- **MinIO** - S3-compatible self-hosted object storage
  - SDK/Client: Same `aws-sdk-s3` SDK (compatible API)
  - Auth: Access key + Secret key per S3 profile
  - Path-style addressing auto-detected for localhost/IP endpoints
  - Env vars: `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY` for proxy configuration

- **S3 Client Cache**: Global `S3_CLIENT_CACHE` keyed by `url|access_key` to avoid redundant client creation

**OpsBox Agent Protocol (HTTP-based):**
- **Agent Communication** - Remote log access and resource browsing
  - SDK/Client: `AgentClient` in `backend/opsbox-core/src/agent/client.rs`
  - Protocol: HTTP/JSON via `reqwest`
  - Auth: None (agents register with server, identified by agent_id)
  - Health check: `GET /health` on agent endpoint
  - Endpoints consumed by server:
    - Search: Agent exposes log search API
    - Explorer: Agent exposes resource listing API
    - Log config: `GET/PUT` for log level and retention
  - Proxy support: `OPSBOX_NO_PROXY=1` disables proxy for agent communication

## Data Storage

**Database:**
- **SQLite** (bundled, no system dependency)
  - Connection: `opsbox-core/src/database.rs` using `sqlx 0.8`
  - Pool: `SqlitePool` with configurable max connections and timeout
  - Journal mode: WAL (Write-Ahead Logging) for concurrent read performance
  - Synchronous mode: Normal (balanced durability/performance)
  - Busy timeout: 10 seconds
  - Default path: `$HOME/.opsbox/opsbox.db`
  - Env var override: `OPSBOX_DATABASE_URL` or `DATABASE_URL`
  - In-memory support: `:memory:` or `sqlite::memory:` for testing

**Database Schema (auto-managed via `init_schema()`):**
- Module tables created by each module's `init_schema()` method
- Tables include: S3 profiles, LLM backends, planner scripts, agent registry, search cache
- No migration system -- tables are created with `CREATE TABLE IF NOT EXISTS`

**File Storage:**
- Local filesystem: Direct file access for local log files
- S3/MinIO: Object storage via `aws-sdk-s3`
- Agent remote filesystem: HTTP proxy to remote agent's local filesystem

**Caching:**
- LRU cache in opsbox-core (`lru 0.16.2`) - in-memory caching for search results and view data
- S3 client cache - global HashMap keyed by endpoint+credentials
- No external cache service (Redis, Memcached, etc.)

## Authentication & Identity

**Auth Provider:**
- None / Custom lightweight approach
- No user authentication system implemented
- Agent registration uses simple agent_id identification
- S3 profiles store credentials encrypted in SQLite database
- LLM API keys stored via environment variables or database

## Monitoring & Observability

**Logging:**
- `tracing` ecosystem (tracing, tracing-subscriber, tracing-appender)
- Structured JSON logging support
- File-based logging with configurable directory and retention
- Dynamic log level adjustment via API (`PUT /api/v1/log/level`)
- Per-module log filtering via `RUST_LOG` env var

**Error Tracking:**
- RFC 7807 Problem Details format for API errors (`opsbox-core/src/error.rs`)
- No external error tracking service (Sentry, etc.)

**Health Check:**
- `GET /healthy` endpoint on server and agents (returns "ok")

## CI/CD & Deployment

**Build:**
- Cargo workspace build (`backend/Cargo.toml`)
- Frontend build outputs to `backend/opsbox-server/static` (embedded in binary)
- Optional module features: `logseek`, `agent-manager`, `explorer`
- Cross-compilation target for agent: `x86_64-unknown-linux-musl`

**Testing:**
- Backend: `cargo test` with `OPSBOX_NO_PROXY=1` for LLM tests
- Frontend unit: `pnpm test:unit` (Vitest)
- Frontend E2E: `pnpm test:e2e` (Playwright, starts both frontend and backend)

**Packaging:**
- RPM generation for agent binary (`cargo generate-rpm`)
- Single binary deployment (frontend embedded via `rust-embed`)

**No CI/CD pipeline config detected in repository** (no .github/workflows, no Jenkinsfile, no .gitlab-ci.yml)

## Environment Configuration

**Required env vars for production:**
- `LLM_PROVIDER` or database setting for LLM functionality
- `OPENAI_API_KEY` (if using OpenAI provider)
- Other vars have sensible defaults

**Required env vars for development/testing:**
- `OPSBOX_NO_PROXY=1` - Disable proxy for LLM tests on macOS
- `CI=1` - CI environment (also disables proxy)

**Proxy configuration (affects S3 and HTTP clients):**
- `HTTP_PROXY` / `http_proxy`
- `HTTPS_PROXY` / `https_proxy`
- `ALL_PROXY` / `all_proxy`
- `NO_PROXY` / `no_proxy`

## Webhooks & Callbacks

**Incoming:**
- Agent heartbeat: `POST /api/v1/agents/{agent_id}/heartbeat` - Agents check in periodically
- Agent registration: `POST /api/v1/agents/register` - New agents register themselves

**Outgoing:**
- None detected -- server does not send webhooks to external services

## Frontend Dev Server Proxy

**Vite dev server proxy:**
- `/api` requests proxied to backend at `http://127.0.0.1:4000` (configurable via `BACKEND_PORT`)
- Used during development only; production uses embedded static assets

---

*Integration audit: 2026-03-13*
