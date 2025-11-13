# OpsBox 代码重构需求文档

## 简介

本文档基于对 OpsBox 日志检索平台的全面代码审查,识别出需要改进的代码质量和结构问题。项目已具备良好的架构基础,本文档聚焦于代码重构,不涉及功能变更。

**重构目标**:
- 提升代码可维护性
- 改善代码结构和职责分离
- 统一代码风格和规范
- 增加测试覆盖率

**项目现状**:
- ✅ 分层架构完善 (Domain/Service/Repository/Routes)
- ✅ 错误处理分层清晰 (DomainError/ServiceError/RepositoryError/ApiError)
- ✅ 前端 FileUrl 解析完整 (支持 Local/S3/Agent/TarEntry/DirEntry)
- ✅ 模块化架构成熟 (基于 inventory 的插件系统)
- ✅ 多存储源支持 (Local/S3/Agent)
- ⚠️ 部分代码仍使用遗留的 AppError
- ⚠️ 路由层包含过多业务逻辑
- ⚠️ 测试覆盖率不足

## 术语表

- **System**: OpsBox 日志检索平台
- **SearchExecutor**: 搜索执行器服务类,封装多数据源搜索逻辑
- **Routes Layer**: HTTP 路由层,应仅负责请求响应处理
- **Service Layer**: 业务逻辑层,包含可复用的核心逻辑
- **AppError**: opsbox-core 中的通用错误类型(遗留,应逐步替换)
- **Tracing**: 结构化日志框架,替代传统 log 宏
- **RateLimiter**: API 请求速率限制器
- **Encryption**: 敏感数据加密存储机制

**重构范围**：
- ✅ 代码结构优化（分层、职责分离）
- ✅ 代码清理（移除重复、统一风格）
- ✅ 可维护性提升（测试、文档）
- ❌ 新功能开发（不在重构范围内）

## 需求

### 需求 1: 清理遗留的 AppError 使用

**用户故事**: 作为开发者,我希望代码统一使用分层错误类型,而不是混用 AppError,以保持架构一致性

**背景**: 项目已建立完善的错误处理分层(DomainError/ServiceError/RepositoryError/LogSeekApiError),但部分代码仍直接使用 `opsbox_core::AppError`

#### 验收标准

1. THE System SHALL 将 routes/view.rs 中的 AppError 替换为对应的 ServiceError 或 RepositoryError
2. THE System SHALL 将 routes/planners.rs 中的 AppError 替换为对应的错误类型
3. THE System SHALL 将 routes/nl2q.rs 中的 AppError 替换为 ServiceError
4. THE System SHALL 将 domain/source_planner/starlark_runtime.rs 中的 AppError 替换为 DomainError 或 ServiceError
5. WHEN 错误转换时, THE System SHALL 保留完整的上下文信息(文件路径、操作类型等)
6. THE System SHALL 确保所有错误最终通过 LogSeekApiError 转换为 Problem Details 格式

### 需求 2: 搜索逻辑服务层提取

**用户故事**: 作为开发者,我希望将复杂的搜索逻辑从路由层提取到服务层,以提高代码复用性和可测试性

**背景**: 当前 routes/search.rs 中的 stream_search 函数包含 280+ 行业务逻辑,包括并发控制、多数据源协调、结果转换等,违反了单一职责原则

#### 验收标准

1. THE System SHALL 创建 service/search_executor.rs 文件定义 SearchExecutor 服务类
2. THE System SHALL 在 SearchExecutor 中封装多数据源并行搜索逻辑
3. THE System SHALL 在 SearchExecutor 中封装并发控制逻辑(IO Semaphore 统一控制所有数据源的并发访问，防止端口耗尽和资源耗尽)
4. THE System SHALL 在 SearchExecutor 中封装单个数据源的搜索逻辑
5. THE System SHALL 将 routes/search.rs 简化为仅处理 HTTP 请求响应(目标 < 100 行)
6. THE System SHALL 使 SearchExecutor 可被非 HTTP 场景复用(CLI、定时任务等)
7. THE System SHALL 为 SearchExecutor 提供单元测试覆盖

### 需求 3: 代码可测试性提升

**用户故事**: 作为开发者,我希望关键业务逻辑有完整的测试覆盖,以确保重构不会引入回归问题

**背景**: 当前 SearchProcessor 和查询解析器有测试,但路由层和服务层缺少测试覆盖

#### 验收标准

1. THE System SHALL 为 SearchExecutor 服务类提供单元测试
2. THE System SHALL 为错误转换逻辑提供单元测试
3. THE System SHALL 为 EntryStreamFactory 提供单元测试
4. THE System SHALL 确保核心服务层模块的测试覆盖率达到 70% 以上
5. THE System SHALL 使用 mock 对象隔离外部依赖(数据库、S3、Agent)
6. THE System SHALL 为边界情况和错误路径提供测试用例
