# Coding Conventions

**Analysis Date:** 2026-03-13

## Naming Patterns

**Files (Rust):**
- Snake_case for all file names (`search_executor.rs`, `entry_stream.rs`)
- Module files: `mod.rs` for directory modules; `lib.rs` for crate root
- Test files: `*_tests.rs` (submodule), `*_integration.rs` (integration), `*_test.rs` (test)
- Config files: `config.rs`, `routes.rs`, `models.rs`, `repository.rs`

**Files (TypeScript/Svelte):**
- PascalCase for Svelte components (`AgentManagement.svelte`, `SearchResultCard.svelte`)
- camelCase for utility files (`highlight.ts`, `orl.ts`, `utils.ts`)
- Test files: `*.test.ts`, `*.svelte.test.ts` (co-located with source)
- Module index: `index.ts` barrel exports per module directory

**Functions (Rust):**
- Snake_case: `start_search`, `extract_session_id`, `build_router`
- Constructor pattern: `::new()` for primary constructors
- Builder methods: `with_*` pattern for optional configuration (`with_path_filter`)
- Async functions: plain names without `_async` suffix

**Functions (TypeScript):**
- camelCase: `startSearch`, `extractSessionId`, `getApiBase`
- Composables: `use*` prefix (`useSearch`, `useStreamReader`, `useLlmBackends`)

**Variables:**
- Rust: snake_case (`db_pool`, `io_max_concurrency`, `error_type`)
- TypeScript: camelCase (`query`, `sessionId`, `hasMore`)

**Types (Rust):**
- PascalCase: `AppError`, `SearchError`, `CompactLines`, `SessionData`
- Error enums: `*Error` suffix (`ServiceError`, `RepositoryError`, `SearchError`)
- Trait: `Module`, `OpbxFileSystem`, `Streamable`

**Types (TypeScript):**
- PascalCase interfaces: `SearchJsonResult`, `ViewParams`, `ApiProblem`
- Union types: `KeywordInfo`, `LlmProviderType`

## Code Style

**Formatting (Rust):**
- Tool: `rustfmt` with `backend/rustfmt.toml`
- 2-space indentation (`tab_spaces = 2`)
- 120-character line width (`max_width = 120`)
- No tabs (`hard_tabs = false`)
- Edition: 2024

**Formatting (TypeScript/Svelte):**
- Tool: Prettier with `web/.prettierrc`
- 2-space indentation (`tabWidth: 2`)
- 120-character line width (`printWidth: 120`)
- Single quotes (`singleQuote: true`)
- No trailing commas (`trailingComma: "none"`)
- Svelte parser with auto embedded language formatting
- TailwindCSS plugin for class sorting

**Linting:**
- ESLint with `eslint-config-prettier` and `eslint-plugin-svelte`
- TypeScript-ESLint for type-aware linting
- Prettier checked via `pnpm --dir web lint`

## Import Organization

**Rust Order:**
1. Standard library (`std::*`)
2. External crates (`axum`, `tokio`, `serde`, `sqlx`)
3. Workspace crates (`opsbox_core`, `opsbox_test_common`)
4. Local module imports (`crate::*`)

**TypeScript Order:**
1. SvelteKit imports (`$app`, `$env`)
2. External packages (`vitest`, `@vitest/browser`)
3. Module imports (`$lib/modules/*`)
4. Type imports with `type` keyword

**Path Aliases (TypeScript):**
- `$lib` - Maps to `web/src/lib/`
- `$env/dynamic/public` - Runtime environment variables
- Module-relative imports preferred within modules

## Error Handling

**Rust Patterns:**
- Unified error type via `opsbox_core::AppError` enum
- RFC 7807 Problem Details for HTTP responses
- Custom error types per layer with `From` conversions to `AppError`
- `Result<T>` type alias: `pub type Result<T> = std::result::Result<T, AppError>`
- Error constructors: `AppError::config()`, `AppError::internal()`, `AppError::bad_request()`, `AppError::not_found()`, `AppError::external_service()`
- `thiserror` derive macro for error enums
- Logging within `IntoResponse`: error-level for 5xx, warn for 4xx

**TypeScript Patterns:**
- `try/catch` with typed error extraction
- Error messages in Chinese for user-facing messages
- Problem Details parsing from backend responses
- Console logging for debugging: `console.warn`, `console.info`

## Logging

**Framework:** Rust `tracing` ecosystem with `tracing-subscriber`

**Patterns:**
- `tracing::info!` for request/response logging
- `tracing::debug!` for configuration details
- `tracing::warn!` for recoverable issues
- `tracing::error!` for server errors
- Span-based HTTP tracing via `tower_http::trace::TraceLayer`
- Environment-based log level configuration

## Comments

**When to Comment:**
- Module-level doc comments (`//!`) for crate/module descriptions
- Struct/trait/function doc comments (`///`) for public APIs
- Inline comments for non-obvious logic, especially conversions
- Section separators using `// ===` for logical grouping
- Chinese comments for business logic explanations

**JSDoc/TSDoc:**
- Used for exported functions with `@param`, `@returns` tags
- Interface/type documentation explaining field purposes
- Module-level `/** */` comments for API clients

## Function Design

**Size:** Small, focused functions. Complex logic extracted to helper functions.

**Parameters:**
- Accept `impl Into<String>` for flexible string parameters
- Use `&str` for read-only string references
- Options via builder pattern or `Option<T>` parameters

**Return Values:**
- `Result<T>` for fallible operations
- `Option<T>` for nullable returns
- `impl Trait` for iterator returns

## Module Design

**Exports:**
- Re-export common types at module root (`pub use error::{AppError, Result}`)
- Barrel `index.ts` files for TypeScript modules
- Public API surface minimized; implementation details in private modules

**Barrel Files:**
- TypeScript: `index.ts` in each module directory (`api/index.ts`, `composables/index.ts`)
- Svelte UI components: `index.ts` re-exporting component files
- Rust: `mod.rs` with `pub use` for key types

---

*Convention analysis: 2026-03-13*
