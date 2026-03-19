# Changelog

All notable changes to OpsBox will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-03-19

### Added - ORL Protocol and Resource Explorer

#### Core Features
- **ORL Protocol Evolution**: Migrated from `odfi://` to `orl://` (OpsBox Resource Locator) protocol
  - RFC 3986 compliant `fluent-uri` parser
  - Unified resource identification for Local, Agent, S3 endpoints
  - Archive entry access support (`?entry=` parameter)
  - Backward compatible with legacy `odfi://` format

- **Distributed Resource Explorer**: New explorer module
  - Unified browsing of Local, S3, Agent endpoint resources
  - Archive file navigation (tar, tar.gz, zip, etc.)
  - File download functionality (`POST /api/v1/explorer/download`)
  - Automatic content type detection (MIME types)
  - Hidden file counts and directory sub-item statistics

- **Agent Manager Enhancements**: agent-manager module improvements
  - Complete tag CRUD operations (add/remove/clear)
  - Tag-based agent filtering and organization
  - Agent log configuration proxy functionality

### Added - DFS (Distributed File System) Subsystem

#### Core Architecture
- **DFS Module**: New distributed file system abstraction in opsbox-core
  - Unified file system interface (OpbxFileSystem trait)
  - Support for Local, S3, and Agent backends
  - Searchable trait for search integration
  - Standard tokio::io::AsyncRead trait support

- **S3 Streaming**: Implemented streaming read for S3 objects
  - Memory-efficient streaming for large files
  - Proper async/await patterns

- **Archive Support**: Comprehensive archive handling
  - Unified archive detection with magic bytes support
  - Common archive module with typed open function
  - Support for tar, tar.gz, zip formats
  - Archive navigation and entry browsing
  - Optimized archive handling with hard links and stream copy

- **Explorer Migration**: Migrated explorer module to DFS
  - Auto-detect archive files for navigation
  - Improved archive performance
  - Consistent file system operations across backends

### Added - Windows Cross-Platform Support

- **Windows Compatibility**: Full support for Windows platform
  - Handle Windows drive paths (C:\path)
  - Path normalization in ORL protocol
  - Cross-platform path handling in DFS subsystem
  - Fix tokio Mutex for Windows daemon mode

- **CI/CD**: GitHub Actions automatic Windows build

### Added - Search Error Visibility

- **Search Status Indicator**: New frontend UI component
  - Real-time search progress and error display
  - Failed sources count and error details
  - Enhanced error propagation in SearchExecutor
  - New `search-status-indicator.svelte` component

### Added - Tracing Logging System

#### Core Features
- **Logging Framework Upgrade**: Migrated from `log` + `env_logger` to `tracing` ecosystem
  - Structured logging support (key-value pairs)
  - Span tracing (cross-function call context)
  - Better performance with zero-cost abstraction

- **Rolling Log Files**: Automatic daily log file rotation
  - New log file created at midnight each day
  - File naming format: `opsbox-server.YYYY-MM-DD.log`
  - Automatic cleanup of log files exceeding retention period

- **Dynamic Log Configuration**: Adjust log settings without restart
  - Dynamic log level modification (immediate effect)
  - Dynamic log retention days modification (effective on next rotation)
  - Configuration persisted to SQLite database

- **Custom Log Path**: Specify log directory via CLI argument
  - Server default path: `~/.opsbox/logs`
  - Agent default path: `~/.opsbox-agent/logs`
  - Support `--log-dir` parameter for custom path

- **Log Retention Policy**: Flexible log retention configuration
  - Support `--log-retention` parameter for retention days (default 7 days)
  - Range: 1-365 days
  - Automatic cleanup of expired logs

#### REST API
- **Server Log Configuration API**:
  - `GET /api/v1/log/config` - Get current log configuration
  - `PUT /api/v1/log/level` - Update log level
  - `PUT /api/v1/log/retention` - Update log retention days

- **Agent Log Configuration API** (via Server proxy):
  - `GET /api/v1/agents/{agent_id}/log/config` - Get Agent log configuration
  - `PUT /api/v1/agents/{agent_id}/log/level` - Update Agent log level
  - `PUT /api/v1/agents/{agent_id}/log/retention` - Update Agent log retention days

- **Agent Local API**:
  - `GET /api/v1/log/config` - Get local log configuration
  - `PUT /api/v1/log/level` - Update local log level
  - `PUT /api/v1/log/retention` - Update local log retention days

#### Web UI
- **Server Log Management Interface**: Added "Server Log" tab in settings page
  - Log level selector (ERROR/WARN/INFO/DEBUG/TRACE)
  - Log retention days input
  - Log path display (read-only)
  - Real-time save and reset functionality

- **Agent Log Management Interface**: Added log settings section in Agent management page
  - Independent log configuration per Agent
  - Expandable/collapsible log settings
  - Auto-disable configuration when Agent is offline
  - Manage Agent logs via Server proxy

#### Technical Improvements
- **Async Log Writing**: Background thread handles log writing, avoiding main thread blocking
- **Multiple Output Targets**: Output to both console and file
  - Console: Colorized output (if terminal supports)
  - File: Plain text format
- **Efficient Filtering**: Use `EnvFilter` for efficient log filtering
- **Performance Optimization**: Batch writing, buffered writing, zero-cost abstraction

#### Documentation
- **User Documentation**: [docs/guides/logging-configuration.md](docs/guides/logging-configuration.md)
  - Log level descriptions
  - CLI argument configuration
  - Log file management
  - Web UI usage guide
  - Troubleshooting guide

- **Developer Documentation**:
  - [docs/architecture/logging-architecture.md](docs/architecture/logging-architecture.md) - Logging system architecture
  - [docs/guides/tracing-usage.md](docs/guides/tracing-usage.md) - Tracing usage guide
  - [docs/api/logging-api.md](docs/api/logging-api.md) - API documentation

### Changed

- **SearchExecutor Refactoring**: Simplified EntryStream creation, improved search performance
- **Relative Path Glob Filtering**: Support for relative path glob pattern filtering
- **Frontend Module Reorganization**: Restructured frontend module organization
- **Test Infrastructure**: Enhanced test coverage, configurable E2E test timeouts
- **Memory Optimization**: mimalloc allocator optimization and explicit memory reclamation

#### Dependencies
- **Added Dependencies**:
  - `tracing = "0.1"` - Structured logging and tracing framework
  - `tracing-subscriber = "0.3"` - Tracing subscriber (env-filter, json, fmt support)
  - `tracing-appender = "0.2"` - Rolling file appender
  - `fluent-uri = "0.3"` - RFC 3986 compliant URI parser

- **Removed Dependencies**:
  - `log = "0.4"` - Replaced by tracing
  - `env_logger = "0.11"` - Replaced by tracing-subscriber

#### Code Migration
- **All crates migrated to tracing**:
  - `opsbox-server`: Fully migrated to tracing
  - `opsbox-core`: Fully migrated to tracing
  - `agent`: Fully migrated to tracing
  - `agent-manager`: Fully migrated to tracing
  - `logseek`: Fully migrated to tracing
  - `explorer`: New module using tracing from start

- **Log call updates**:
  - `log::info!` → `tracing::info!`
  - `log::debug!` → `tracing::debug!`
  - `log::warn!` → `tracing::warn!`
  - `log::error!` → `tracing::error!`
  - `log::trace!` → `tracing::trace!`

### REST API

- **Explorer Module API**:
  - `POST /api/v1/explorer/list` - List resources
  - `POST /api/v1/explorer/download` - Download files

### Breaking Changes

#### CLI Arguments
- **New Arguments**:
  - `--log-dir <DIR>` - Specify log file directory
  - `--log-retention <N>` - Specify log retention days (default 7)

- **Backward Compatible**:
  - `--log-level` - Still supported (sets initial log level)
  - `-v/-vv/-vvv` - Still supported (shortcuts)
  - `RUST_LOG` environment variable - Still supported

#### Log Format
- **Console Output**: Slightly changed format, but maintains readability
  - Old format: `[2024-01-15 10:30:45] INFO Server started`
  - New format: `2024-01-15T10:30:45.123Z  INFO opsbox_server::server: Server started`

- **File Output**: New rolling log files
  - Old behavior: Single log file (potentially unbounded growth)
  - New behavior: Daily rotation, automatic cleanup of old files

#### Database Schema
- **New Table**: `log_config` - Stores log configuration
  ```sql
  CREATE TABLE log_config (
      id INTEGER PRIMARY KEY CHECK (id = 1),
      component TEXT NOT NULL,
      level TEXT NOT NULL,
      retention_count INTEGER NOT NULL,
      updated_at INTEGER NOT NULL
  );
  ```

### Migration Guide

#### For Users

1. **Update Startup Script** (optional):
   ```bash
   # Old startup (still works)
   ./opsbox-server

   # New startup (recommended)
   ./opsbox-server --log-dir /var/log/opsbox --log-retention 30
   ```

2. **Log File Location Change**:
   - Old location: Logs to stdout/stderr (or via redirection)
   - New location:
     - Server: `~/.opsbox/logs/opsbox-server.YYYY-MM-DD.log`
     - Agent: `~/.opsbox-agent/logs/opsbox-agent.YYYY-MM-DD.log`

3. **Log Management**:
   - Old way: Manual log file management
   - New way: Automatic rotation and cleanup, or manage via Web UI

#### For Developers

1. **Update Import Statements**:
   ```rust
   // Old code
   use log::{debug, error, info, warn};

   // New code
   use tracing::{debug, error, info, warn};
   ```

2. **Use Structured Fields** (recommended):
   ```rust
   // Old code
   log::info!("User {} logged in", user_id);

   // New code (recommended)
   tracing::info!(user_id = user_id, "User logged in");
   ```

3. **Use instrument Macro** (recommended):
   ```rust
   // New feature: automatic function call tracing
   #[tracing::instrument]
   async fn process_request(user_id: i64) -> Result<Response> {
       tracing::info!("Processing request");
       // ...
   }
   ```

4. **Test Code Update**:
   ```rust
   // Old code
   env_logger::init();

   // New code
   let _ = tracing_subscriber::fmt()
       .with_test_writer()
       .try_init();
   ```

### Fixed

- Fixed potential unbounded log file growth (via rotation and retention policy)
- Fixed log output potentially blocking main thread (via async writing)
- Fixed inability to dynamically adjust log level (via reload handle)
- Fixed Windows drive path handling in ORL protocol
- Fixed tokio Mutex issue for Windows daemon mode

### Performance

- **Log Write Performance**: Async writing achieves 100,000+ entries/second
- **Main Thread Latency**: < 1μs (just send to channel)
- **Memory Usage**: Default buffer 8KB, configurable
- **Compile-time Optimization**: Disabled log levels are optimized away by compiler

### Security

- **Path Validation**: Validate log directory path to prevent path traversal attacks
- **Permission Check**: Ensure log directory has correct write permissions
- **Sensitive Information Filtering**: Avoid logging passwords, keys, and other sensitive information

---

## [Previous Versions]

_Previous changelog entries will be added here as the project evolves._

---

## Legend

- **Added**: New features
- **Changed**: Changes to existing features
- **Deprecated**: Features to be removed
- **Removed**: Removed features
- **Fixed**: Bug fixes
- **Security**: Security-related fixes
- **Performance**: Performance improvements
- **Breaking Changes**: Incompatible changes
- **Migration Guide**: Migration instructions
