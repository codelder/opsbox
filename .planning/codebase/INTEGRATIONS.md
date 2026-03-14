# External Integrations

**Analysis Date:** 2026-03-13

## APIs & External Services

**LLM Providers:**
- Ollama (local) - Natural language to query, source planning
  - SDK/Client: Custom reqwest HTTP client in `backend/opsbox-core/src/llm.rs`
  - Auth: `OLLAMA_BASE_URL` (default `http://127.0.0.1:11434`), default model `qwen3:8b`
- OpenAI-compatible APIs - Natural language to query, source planning
  - SDK/Client: Custom reqwest HTTP client in `backend/opsbox-core/src/llm.rs`
  - Auth: `OPENAI_API_KEY` required, default model `gpt-4o-mini`

**S3/MinIO Storage:**
- AWS S3 SDK (aws-sdk-s3 1.15) - Object storage access
  - SDK/Client: `backend/opsbox-core/src/storage/s3.rs`
  - Auth: Profile-based credentials stored in SQLite database

**Agent Communication:**
- Custom reqwest client in `backend/opsbox-core/src/agent/client.rs`
- Agent self-registration via POST
- Health monitoring via heartbeat

## Data Storage

**Databases:**
- SQLite (via sqlx 0.8 with bundled libsqlite3-sys)
  - Connection: `OPSBOX_DATABASE_URL` / `DATABASE_URL` (default: `$HOME/.opsbox/opsbox.db`)
  - Client: sqlx with WAL journal mode
  - Schema: Auto-managed per module via `init_schema()`

**File Storage:**
- Local filesystem (via jwalk for efficient traversal)
- S3/MinIO buckets (via aws-sdk-s3)
- Archive files (tar, tar.gz, gzip, tgz, zip via async-tar/async-zip)
- Embedded static assets (via rust-embed)

**Caching:**
- LRU in-memory cache (lru 0.16.2) - Search results
- SQLite cache table - Persistent search result caching
- S3 client connection pool

## Authentication & Identity

**Auth Provider:**
- None currently implemented (internal tool assumption)
- Agent trust via self-registration
- S3 credential-based profiles stored in database

## Monitoring & Observability

**Error Tracking:**
- None (internal tool)

**Logs:**
- `tracing` ecosystem with file appender
- Configurable log levels and retention
- JSON structured output

## CI/CD & Deployment

**Hosting:**
- Self-hosted single binary deployment

**CI Pipeline:**
- GitHub Actions (implied from test badges)

## Environment Configuration

**Required env vars:**
- `OPSBOX_DATABASE_URL` - Custom database path
- `LLM_PROVIDER` - LLM provider selection
- `OLLAMA_BASE_URL` - Ollama server URL
- `OPENAI_API_KEY` - OpenAI API key (if using OpenAI)
- `OPSBOX_NO_PROXY=1` - Disable proxy for tests

**Secrets location:**
- Environment variables for LLM API keys
- SQLite database for S3 profile credentials

## Webhooks & Callbacks

**Incoming:**
- Agent heartbeat/registration endpoints (`/api/v1/agents/...`)

**Outgoing:**
- LLM chat completions (Ollama/OpenAI)
- Agent log configuration proxy

---

*Integration audit: 2026-03-13*
