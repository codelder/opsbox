# Coding Conventions

**Analysis Date:** 2026-03-13

## Naming Patterns

**Files (Rust):**
- snake_case for all `.rs` files: `search_executor.rs`, `entry_stream.rs`, `orl_parser.rs`
- Test files co-located as modules: `search_tests.rs` inside `search.rs`, or integration tests in `tests/` directory
- Module structure: `mod.rs` for directory modules, `lib.rs` for crate root

**Files (SvelteKit/TypeScript):**
- PascalCase for Svelte components: `AgentManagement.svelte`, `ServerLogSettings.svelte`
- snake_case or camelCase for utility files: `orl.test.ts`, `highlight.test.ts`
- API clients: `search.ts`, `view.ts`, `profiles.ts`
- Test files: `{name}.test.ts` or `{name}.svelte.test.ts`

**Functions (Rust):**
- snake_case: `view_cache_json`, `should_process_path`, `process_content`
- Constructors: `new()`, `new_with_encoding()`
- Test functions: `test_{function_name}_{scenario}` (e.g., `test_process_content_no_match`)

**Functions (TypeScript):**
- camelCase: `startSearch`, `extractSessionId`, `fetchViewCache`

**Variables:**
- Rust: snake_case throughout
- TypeScript: camelCase

**Types (Rust):**
- PascalCase: `SearchProcessor`, `AppError`, `LogSeekApiError`, `TestError`
- Error enums: `{Layer}Error` pattern (e.g., `ServiceError`, `RepositoryError`, `SearchError`)

**Types (TypeScript):**
- PascalCase interfaces/types: `ViewParams`, `SearchBody`, `AgentInfo`

## Code Style

**Formatting:**
- Rust: Standard `rustfmt` (2024 edition)
- Prettier config (shared root `.prettierrc`):
  - `useTabs: false`, `tabWidth: 2`, `printWidth: 120`
  - `singleQuote: true`, `trailingComma: "none"`
- Svelte: Uses `prettier-plugin-svelte` and `prettier-plugin-tailwindcss`
- Tailwind: Configured via `tailwindStylesheet: "./src/app.css"`

**Linting (Rust):**
- No explicit clippy config found, default rustc lints apply
- `thiserror` for error derivation

**Linting (TypeScript/Svelte):**
- ESLint with `typescript-eslint` and `eslint-plugin-svelte`
- `eslint-config-prettier` integration
- Svelte-specific: `svelte/no-at-html-tags` disabled (controlled HTML rendering)
- TypeScript `no-undef` disabled (handled by TS)

## Import Organization

**Rust:**
1. Standard library imports (`std::`)
2. External crate imports (alphabetical)
3. Internal crate imports (`crate::`, `opsbox_core::`)

Example from `backend/logseek/src/routes/view.rs`:
```rust
use axum::{
  body::Body,
  extract::{Query, State},
  http::{HeaderValue, Response as HttpResponse, header::CONTENT_TYPE},
};
use opsbox_core::SqlitePool;
use opsbox_core::dfs::{Location, OrlParser};
use serde::{Deserialize, Serialize};
use tracing::debug;
```

**TypeScript/Svelte:**
1. External packages
2. SvelteKit internal (`$lib/`)
3. Local relative imports

## Error Handling

**Rust - Layered Error Architecture:**

1. **Core errors** (`opsbox-core::error::AppError`):
   - Unified enum: `Database`, `Config`, `Internal`, `BadRequest`, `NotFound`, `ExternalService`
   - Implements `IntoResponse` with RFC 7807 Problem Details format
   - Builder methods: `AppError::config()`, `AppError::internal()`, `AppError::bad_request()`, etc.

2. **Service errors** (`logseek::service::ServiceError`):
   - Variants: `ConfigError`, `ProcessingError`, `SearchFailed`, `IoError`, `ChannelClosed`, `Repository`
   - Converts to `AppError` via `From` trait

3. **Repository errors** (`logseek::repository::RepositoryError`):
   - Variants: `NotFound`, `QueryFailed`, `StorageError`, `CacheFailed`, `Database`

4. **API errors** (`logseek::api::error::LogSeekApiError`):
   - Aggregates all layer errors
   - Maps to HTTP status codes with Problem Details JSON
   - Uses `#[error(transparent)]` for delegation

Error conversion chain: Repository -> Service -> AppError -> LogSeekApiError

**TypeScript:**
- API errors thrown as `Error` with HTTP status message
- Error response parsing with JSON fallback to status text

## Logging

**Framework:** `tracing` ecosystem (Rust)

**Patterns:**
- Debug: `tracing::debug!("🔍 Server查找缓存: sid={}, file_url={}", ...)`
- Info: `tracing::info!("[{}] {}", error_type, error_msg)`
- Warn: `tracing::warn!("❌ Server缓存未命中: ...")`
- Error: `tracing::error!("[LogSeek API] [{}] {}", title, detail)`
- Structured logging with key-value pairs
- Chinese log messages are common in this codebase

## Comments

**When to Comment:**
- Module-level doc comments with section headers: `// === API Layer ===`
- Bilingual comments: English doc comments, Chinese inline comments
- Code organization markers: `// 1. 解析 ORL`, `// 2. 检查缓存`

**TSDoc/Rustdoc:**
- Module docs at top of `lib.rs`: `//! OpsBox 核心共享库`
- Function docs in Chinese: `/// 查看缓存中的文件内容`
- Test descriptions in Chinese: `/// 测试中的响应体最大读取大小（1MB）`

## Function Design

**Size:** Functions generally kept focused; complex handlers may be 100-200 lines

**Parameters:**
- Extract parameters via Axum extractors: `State(pool): State<SqlitePool>`, `Query(params): Query<ViewParams>`
- Struct-based params with `#[derive(Deserialize)]`

**Return Values:**
- Result type aliases: `pub type Result<T> = std::result::Result<T, LogSeekApiError>`
- HTTP responses built with `HttpResponse::builder().status(200).header(...).body(...)`

## Module Design

**Rust Module Structure (logseek example):**
```
logseek/
├── src/
│   ├── lib.rs          # Module root, trait impl, router export
│   ├── api.rs          # API layer documentation
│   ├── api/
│   │   ├── error.rs    # API error types
│   │   └── models.rs   # Request/Response models
│   ├── routes/         # HTTP route handlers
│   ├── service/        # Business logic
│   ├── repository/     # Data access
│   ├── domain/         # Core domain models
│   └── utils/          # Shared utilities
└── tests/              # Integration tests
```

**SvelteKit Module Structure:**
```
web/src/lib/modules/
├── logseek/
│   ├── api/            # API clients
│   ├── types/          # TypeScript types
│   ├── composables/    # Svelte composables
│   └── components/     # UI components
├── agent/              # Agent module
└── explorer/           # Explorer module
```

## API Layer Conventions

**Route handlers:**
- Dual layer pattern: `routes.rs` for backward compatibility, `routes/` directory for organized handlers
- Return `Result<HttpResponse<Body>, LogSeekApiError>`
- JSON responses use `serde_json::json!()` macro

**Request models:**
- Derive `Debug, Clone, Deserialize`
- Named `*Params` for query params, `*Body` for request body

**Response models:**
- Derive `Debug, Clone, Serialize`
- Named `*Response` or `*Out`

## Frontend Svelte 5 Conventions

**Component Props:**
- Use `$props()` rune with destructuring
- `ref = $bindable(null)` for element refs
- Spread `...restProps` for HTML attributes

**State Management:**
- Svelte 5 Runes (`$state`, `$derived`, `$effect`)
- Composables pattern for reusable logic

## Error Response Format

All API errors return RFC 7807 Problem Details:
```json
{
  "type": "about:blank",
  "title": "Error Title",
  "detail": "Detailed error message",
  "status": 400
}
```

Content-Type: `application/problem+json; charset=utf-8`

---

*Convention analysis: 2026-03-13*
