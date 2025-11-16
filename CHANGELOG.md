# Changelog

All notable changes to OpsBox will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added - Tracing 日志系统重构

#### 核心功能
- **日志框架升级**: 从 `log` + `env_logger` 迁移到 `tracing` 生态系统
  - 支持结构化日志（key-value pairs）
  - 支持 Span 追踪（跨函数调用的上下文）
  - 更好的性能和零成本抽象
  
- **滚动日志文件**: 自动按日期滚动日志文件
  - 每天午夜自动创建新的日志文件
  - 文件命名格式：`opsbox-server.YYYY-MM-DD.log`
  - 自动清理超过保留天数的旧日志文件
  
- **动态日志配置**: 无需重启即可调整日志设置
  - 动态修改日志级别（立即生效）
  - 动态修改日志保留天数（下次滚动时生效）
  - 配置持久化到 SQLite 数据库
  
- **自定义日志路径**: 支持通过命令行参数指定日志目录
  - Server 默认路径：`~/.opsbox/logs`
  - Agent 默认路径：`~/.opsbox-agent/logs`
  - 支持 `--log-dir` 参数自定义路径
  
- **日志保留策略**: 灵活的日志保留配置
  - 支持 `--log-retention` 参数指定保留天数（默认 7 天）
  - 范围：1-365 天
  - 自动清理超过保留期的旧日志

#### REST API
- **Server 日志配置 API**:
  - `GET /api/v1/log/config` - 获取当前日志配置
  - `PUT /api/v1/log/level` - 更新日志级别
  - `PUT /api/v1/log/retention` - 更新日志保留天数
  
- **Agent 日志配置 API** (通过 Server 代理):
  - `GET /api/v1/agents/{agent_id}/log/config` - 获取 Agent 日志配置
  - `PUT /api/v1/agents/{agent_id}/log/level` - 更新 Agent 日志级别
  - `PUT /api/v1/agents/{agent_id}/log/retention` - 更新 Agent 日志保留天数
  
- **Agent 本地 API**:
  - `GET /api/v1/log/config` - 获取本地日志配置
  - `PUT /api/v1/log/level` - 更新本地日志级别
  - `PUT /api/v1/log/retention` - 更新本地日志保留天数

#### Web UI
- **Server 日志管理界面**: 在设置页面添加 "Server 日志" 标签
  - 日志级别选择器（ERROR/WARN/INFO/DEBUG/TRACE）
  - 日志保留天数输入框
  - 日志路径显示（只读）
  - 实时保存和重置功能
  
- **Agent 日志管理界面**: 在 Agent 管理页面添加日志设置区域
  - 每个 Agent 独立的日志配置
  - 支持展开/折叠日志设置
  - Agent 离线时自动禁用配置修改
  - 通过 Server 代理管理 Agent 日志

#### 技术改进
- **异步日志写入**: 使用后台线程处理日志写入，避免阻塞主线程
- **多输出目标**: 同时输出到控制台和文件
  - 控制台：彩色输出（如果终端支持）
  - 文件：纯文本格式
- **高效过滤**: 使用 `EnvFilter` 进行高效的日志过滤
- **性能优化**: 批量写入、缓冲写入、零成本抽象

#### 文档
- **用户文档**: [docs/guides/logging-configuration.md](docs/guides/logging-configuration.md)
  - 日志级别说明
  - 启动参数配置
  - 日志文件管理
  - Web UI 使用指南
  - 故障排查指南
  
- **开发者文档**: 
  - [docs/architecture/logging-architecture.md](docs/architecture/logging-architecture.md) - 日志系统架构
  - [docs/guides/tracing-usage.md](docs/guides/tracing-usage.md) - Tracing 使用指南
  - [docs/api/logging-api.md](docs/api/logging-api.md) - API 文档

### Changed

#### 依赖更新
- **新增依赖**:
  - `tracing = "0.1"` - 结构化日志和追踪框架
  - `tracing-subscriber = "0.3"` - Tracing 订阅器（支持 env-filter, json, fmt）
  - `tracing-appender = "0.2"` - 滚动文件追加器
  
- **移除依赖**:
  - `log = "0.4"` - 已被 tracing 替代
  - `env_logger = "0.11"` - 已被 tracing-subscriber 替代

#### 代码迁移
- **所有 crate 迁移到 tracing**:
  - `opsbox-server`: 完全迁移到 tracing
  - `opsbox-core`: 完全迁移到 tracing
  - `agent`: 完全迁移到 tracing
  - `agent-manager`: 完全迁移到 tracing
  - `logseek`: 完全迁移到 tracing
  
- **日志调用更新**:
  - `log::info!` → `tracing::info!`
  - `log::debug!` → `tracing::debug!`
  - `log::warn!` → `tracing::warn!`
  - `log::error!` → `tracing::error!`
  - `log::trace!` → `tracing::trace!`

#### 日志级别优化
- 审查并优化所有日志调用的级别
- 减少 INFO 级别的日志输出
- 将详细信息移到 DEBUG 级别
- 确保 ERROR 和 WARN 级别的日志有意义

### Breaking Changes

#### 命令行参数
- **新增参数**:
  - `--log-dir <DIR>` - 指定日志文件目录
  - `--log-retention <N>` - 指定日志保留天数（默认 7）
  
- **保持兼容**:
  - `--log-level` - 继续支持（设置初始日志级别）
  - `-v/-vv/-vvv` - 继续支持（快捷方式）
  - `RUST_LOG` 环境变量 - 继续支持

#### 日志格式
- **控制台输出**: 格式略有变化，但保持可读性
  - 旧格式: `[2024-01-15 10:30:45] INFO Server started`
  - 新格式: `2024-01-15T10:30:45.123Z  INFO opsbox_server::server: Server started`
  
- **文件输出**: 新增滚动日志文件
  - 旧行为: 单个日志文件（可能无限增长）
  - 新行为: 按日期滚动，自动清理旧文件

#### 数据库 Schema
- **新增表**: `log_config` - 存储日志配置
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

#### 对于用户

1. **更新启动脚本**（可选）:
   ```bash
   # 旧启动方式（仍然有效）
   ./opsbox-server
   
   # 新启动方式（推荐）
   ./opsbox-server --log-dir /var/log/opsbox --log-retention 30
   ```

2. **日志文件位置变化**:
   - 旧位置: 日志输出到 stdout/stderr（或通过重定向）
   - 新位置: 
     - Server: `~/.opsbox/logs/opsbox-server.YYYY-MM-DD.log`
     - Agent: `~/.opsbox-agent/logs/opsbox-agent.YYYY-MM-DD.log`

3. **日志管理**:
   - 旧方式: 手动管理日志文件
   - 新方式: 自动滚动和清理，或通过 Web UI 管理

#### 对于开发者

1. **更新导入语句**:
   ```rust
   // 旧代码
   use log::{debug, error, info, warn};
   
   // 新代码
   use tracing::{debug, error, info, warn};
   ```

2. **使用结构化字段**（推荐）:
   ```rust
   // 旧代码
   log::info!("User {} logged in", user_id);
   
   // 新代码（推荐）
   tracing::info!(user_id = user_id, "User logged in");
   ```

3. **使用 instrument 宏**（推荐）:
   ```rust
   // 新功能：自动追踪函数调用
   #[tracing::instrument]
   async fn process_request(user_id: i64) -> Result<Response> {
       tracing::info!("Processing request");
       // ...
   }
   ```

4. **测试代码更新**:
   ```rust
   // 旧代码
   env_logger::init();
   
   // 新代码
   let _ = tracing_subscriber::fmt()
       .with_test_writer()
       .try_init();
   ```

### Fixed

- 修复了日志文件可能无限增长的问题（通过滚动和保留策略）
- 修复了日志输出可能阻塞主线程的问题（通过异步写入）
- 修复了无法动态调整日志级别的问题（通过 reload handle）

### Performance

- **日志写入性能**: 异步写入可以达到 100,000+ 条/秒
- **主线程延迟**: < 1μs（仅发送到通道）
- **内存使用**: 默认缓冲区 8KB，可配置
- **编译时优化**: 未启用的日志级别会被编译器优化掉

### Security

- **路径验证**: 验证日志目录路径，防止路径遍历攻击
- **权限检查**: 确保日志目录有正确的写入权限
- **敏感信息过滤**: 避免在日志中记录密码、密钥等敏感信息

---

## [Previous Versions]

_Previous changelog entries will be added here as the project evolves._

---

## Legend

- **Added**: 新功能
- **Changed**: 现有功能的变更
- **Deprecated**: 即将移除的功能
- **Removed**: 已移除的功能
- **Fixed**: Bug 修复
- **Security**: 安全相关的修复
- **Performance**: 性能改进
- **Breaking Changes**: 不兼容的变更
- **Migration Guide**: 迁移指南
