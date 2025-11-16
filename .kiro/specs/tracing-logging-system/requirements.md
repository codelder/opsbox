# 需求文档：Tracing 日志系统重构

## 简介

本文档定义了将 OpsBox 项目从 `log` + `env_logger` 迁移到 `tracing` 生态系统的需求。新的日志系统将支持滚动日志、动态日志级别调整、自定义日志路径等高级功能，适用于 Server 和 Agent 两个组件。

## 术语表

- **Server**: OpsBox 主服务器程序（opsbox-server）
- **Agent**: OpsBox 远程代理程序（agent）
- **Tracing**: Rust 的结构化日志和追踪框架
- **RollingFileAppender**: tracing-appender 提供的滚动文件日志写入器
- **LogLevel**: 日志级别（ERROR, WARN, INFO, DEBUG, TRACE）
- **LogRetention**: 日志保留策略，指定保留的日志文件数量
- **DynamicReload**: 运行时动态修改日志配置的能力

## 需求

### 需求 1：迁移到 Tracing 框架

**用户故事：** 作为开发者，我希望使用 tracing 框架替代 log，以便获得更强大的结构化日志和追踪能力。

#### 验收标准

1. THE Server SHALL 使用 tracing 和 tracing-subscriber 替代 log 和 env_logger
2. THE Agent SHALL 使用 tracing 和 tracing-subscriber 替代 log 和 env_logger
3. THE opsbox-core SHALL 使用 tracing 替代 log
4. THE logseek SHALL 使用 tracing 替代 log
5. WHEN 迁移完成后，THE System SHALL 保持所有现有日志输出功能

### 需求 2：滚动日志文件支持

**用户故事：** 作为运维人员，我希望日志文件能够自动滚动，避免单个日志文件过大，以便更好地管理磁盘空间。

#### 验收标准

1. THE Server SHALL 支持按日期滚动日志文件（每日一个新文件）
2. THE Agent SHALL 支持按日期滚动日志文件（每日一个新文件）
3. WHEN 单个日志文件达到 10MB 时，THE System SHALL 创建新的日志文件
4. THE System SHALL 在日志文件名中包含日期时间戳
5. THE System SHALL 同时输出日志到控制台和文件

### 需求 3：动态日志保留配置

**用户故事：** 作为运维人员，我希望能够通过前端界面动态设置日志保留数量，以便根据磁盘空间灵活调整日志策略。

#### 验收标准

1. THE Server SHALL 提供 API 端点接受日志保留数量配置
2. THE Agent SHALL 提供 API 端点接受日志保留数量配置
3. WHEN 日志保留数量更新时，THE System SHALL 在下次日志滚动时应用新配置
4. THE System SHALL 将日志保留配置持久化到数据库
5. THE System SHALL 在启动时从数据库加载日志保留配置

### 需求 4：合理的默认日志级别

**用户故事：** 作为用户，我希望在正常运行时看到适量的日志信息，不要过于冗余，以便快速定位关键信息。

#### 验收标准

1. THE Server SHALL 默认使用 INFO 日志级别
2. THE Agent SHALL 默认使用 INFO 日志级别
3. THE System SHALL 在 ERROR 级别记录所有错误信息
4. THE System SHALL 在 WARN 级别记录警告信息
5. THE System SHALL 在 INFO 级别记录关键操作（启动、关闭、请求处理）
6. THE System SHALL 在 DEBUG 级别记录详细调试信息
7. THE System SHALL 在 TRACE 级别记录最详细的追踪信息

### 需求 5：动态日志级别调整

**用户故事：** 作为运维人员，我希望能够在不重启服务的情况下动态调整日志级别，以便在排查问题时获取更详细的日志。

#### 验收标准

1. THE Server SHALL 提供 API 端点接受日志级别更新请求
2. THE Agent SHALL 提供 API 端点接受日志级别更新请求
3. WHEN 日志级别更新时，THE System SHALL 立即应用新的日志级别
4. THE System SHALL 验证日志级别参数的有效性
5. THE System SHALL 返回当前日志级别给调用者

### 需求 6：自定义日志路径

**用户故事：** 作为运维人员，我希望能够在启动时指定日志文件的存储路径，以便将日志存储到合适的磁盘分区。

#### 验收标准

1. THE Server SHALL 接受命令行参数 --log-dir 指定日志目录
2. THE Agent SHALL 接受命令行参数 --log-dir 指定日志目录
3. WHEN 未指定日志目录时，THE Server SHALL 使用默认路径 ~/.opsbox/logs
4. WHEN 未指定日志目录时，THE Agent SHALL 使用默认路径 ~/.opsbox-agent/logs
5. THE System SHALL 在启动时创建日志目录（如果不存在）
6. WHEN 日志目录无法创建或写入时，THE System SHALL 输出错误信息并退出

### 需求 7：前端日志管理界面

**用户故事：** 作为运维人员，我希望通过前端界面管理日志配置，以便更方便地调整日志设置。

#### 验收标准

1. THE System SHALL 提供前端界面显示当前日志级别
2. THE System SHALL 提供前端界面显示当前日志保留数量
3. THE System SHALL 提供前端界面允许修改 Server 日志级别
4. THE System SHALL 提供前端界面允许修改 Agent 日志级别
5. THE System SHALL 提供前端界面允许修改 Server 日志保留数量
6. THE System SHALL 提供前端界面允许修改 Agent 日志保留数量
7. WHEN 配置更新成功时，THE System SHALL 显示成功提示
8. WHEN 配置更新失败时，THE System SHALL 显示错误信息

### 需求 8：日志格式和内容

**用户故事：** 作为开发者和运维人员，我希望日志格式清晰易读，包含必要的上下文信息，以便快速理解日志内容。

#### 验收标准

1. THE System SHALL 在日志中包含时间戳（ISO 8601 格式）
2. THE System SHALL 在日志中包含日志级别
3. THE System SHALL 在日志中包含模块路径
4. THE System SHALL 在日志中包含日志消息
5. THE System SHALL 支持结构化字段（key-value pairs）
6. WHEN 输出到控制台时，THE System SHALL 使用彩色输出（如果终端支持）
7. WHEN 输出到文件时，THE System SHALL 使用纯文本格式

### 需求 9：性能和资源管理

**用户故事：** 作为系统管理员，我希望日志系统不会对应用性能产生显著影响，以便保持系统的高性能。

#### 验收标准

1. THE System SHALL 使用异步日志写入避免阻塞主线程
2. THE System SHALL 限制日志缓冲区大小避免内存溢出
3. WHEN 日志写入失败时，THE System SHALL 丢弃日志而不是阻塞
4. THE System SHALL 在关闭时刷新所有待写入的日志
5. THE System SHALL 自动清理超过保留数量的旧日志文件
