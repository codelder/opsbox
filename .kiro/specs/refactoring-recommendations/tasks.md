# 实施任务列表

## 阶段 1: 错误处理统一

- [x] 1. 识别和清理 AppError 使用
  - [x] 1.1 在 routes/view.rs 中替换 AppError
    - 将 `AppError::bad_request` 替换为 `ServiceError::ConfigError`
    - 将 `AppError::not_found` 替换为 `RepositoryError::NotFound`
    - 确保错误上下文信息完整保留
    - _Requirements: 需求 1.1, 1.5_
  
  - [x] 1.2 在 routes/planners.rs 中替换 AppError
    - 将验证错误替换为 `ServiceError::ConfigError`
    - 将 HTTP 响应构建错误替换为 `ServiceError::ProcessingError`
    - _Requirements: 需求 1.2, 1.5_
  
  - [x] 1.3 在 routes/nl2q.rs 中替换 AppError
    - 将 LLM 调用错误替换为 `ServiceError::ProcessingError`
    - 保留完整的错误堆栈信息
    - _Requirements: 需求 1.3, 1.5_
  
  - [x] 1.4 在 domain/source_planner/starlark_runtime.rs 中替换 AppError
    - 将脚本解析错误替换为 `DomainError` 或 `ServiceError`
    - 将脚本执行错误替换为 `ServiceError::ProcessingError`
    - 将配置缺失错误替换为 `ServiceError::ConfigError`
    - _Requirements: 需求 1.4, 1.5_
  
  - [x] 1.5 在 lib.rs 中替换 AppError
    - 将 `init_schema` 函数中的 `AppError::internal` 替换为 `ServiceError::ProcessingError`
    - 更新错误转换逻辑以使用分层错误类型
    - _Requirements: 需求 1.5_
  
  - [x] 1.6 验证错误转换正确性
    - 确认代码中不再有 `AppError::` 的直接使用（除了 lib.rs 和 api/error.rs 中的类型转换）
    - 所有路由层文件已使用分层错误类型
    - API 响应格式符合 Problem Details 规范
    - _Requirements: 需求 1.6_

## 阶段 2: SearchExecutor 服务层提取

- [ ] 2. 创建 SearchExecutor 基础结构
  - [ ] 2.1 创建 service/search_executor.rs 文件
    - 定义 `SearchExecutorConfig` 结构体（包含 io_max_concurrency 和 stream_channel_capacity）
    - 定义 `SearchExecutor` 结构体（包含 pool、config 和 io_semaphore）
    - 实现基本的构造函数 `new()`
    - 在 service/mod.rs 中导出新模块
    - _Requirements: 需求 2.1_
  
  - [ ] 2.2 实现数据源配置加载和查询解析
    - 从 routes/search.rs 提取 `get_storage_source_configs()` 逻辑到 SearchExecutor
    - 实现 `get_sources()` 方法调用 Planner 获取数据源配置
    - 实现查询解析逻辑 `parse_query()` 使用现有的 Query::parse_github_like
    - 处理 app 和 encoding 限定词提取
    - 实现 sid 生成和 keywords 缓存逻辑
    - _Requirements: 需求 2.2_
  
  - [ ] 2.3 实现 Agent 数据源搜索逻辑
    - 从 routes/search.rs 提取 Agent 数据源搜索逻辑（约 100 行）
    - 实现 `search_agent_source()` 函数处理 Agent 数据源
    - 使用 AgentClient 进行远程搜索调用
    - 处理 Target 路径调整（拼接 subpath）
    - 实现结果流消费和 SearchEvent 转换
    - 实现缓存逻辑和 FileUrl 构造
    - _Requirements: 需求 2.4_
  
  - [ ] 2.4 实现 Local/S3 数据源搜索逻辑
    - 从 routes/search.rs 提取 EntryStream 搜索逻辑（约 80 行）
    - 实现 `search_entry_stream_source()` 函数处理 Local/S3 数据源
    - 使用 EntryStreamFactory 创建条目流
    - 使用 SearchProcessor 和 EntryStreamProcessor 处理搜索
    - 实现 filter_glob 支持
    - 实现结果转换和缓存逻辑
    - _Requirements: 需求 2.4_
  
  - [ ] 2.5 实现并发控制和任务调度
    - 实现 `spawn_source_search()` 方法启动单个数据源搜索任务
    - 使用 io_semaphore 控制 S3/Local 数据源的并发访问
    - 确保 Agent 数据源不受 IO semaphore 限制
    - 实现结果通道管理和事件发送
    - 实现 Complete 事件发送
    - _Requirements: 需求 2.3_
  
  - [ ] 2.6 实现主搜索方法
    - 实现 `search()` 方法作为公共 API
    - 协调多数据源并行搜索
    - 创建 mpsc 通道用于结果聚合
    - 为每个数据源启动搜索任务
    - 返回 (Receiver<SearchEvent>, String) 供调用者消费（包含 sid）
    - _Requirements: 需求 2.2_

- [ ] 3. 简化路由层
  - [ ] 3.1 重构 routes/search.rs 使用 SearchExecutor
    - 将 `stream_search()` 函数简化为调用 SearchExecutor
    - 移除所有业务逻辑（数据源配置、并发控制、搜索协调）
    - 保留 HTTP 请求解析（SearchBody）
    - 保留 HTTP 响应构建（NDJSON 流）
    - 目标代码行数从 644 行减少到 < 150 行
    - _Requirements: 需求 2.5_
  
  - [ ] 3.2 实现 NDJSON 流转换辅助函数
    - 实现 `convert_to_ndjson_stream()` 将 SearchEvent 转换为 Bytes 流
    - 实现 `build_ndjson_response()` 构建 HTTP 响应（包含 X-Logseek-SID 头）
    - 确保流式处理正确（使用 ReceiverStream）
    - 保持与现有 API 行为完全一致
    - _Requirements: 需求 2.5_
  
  - [ ] 3.3 验证重构正确性
    - 运行现有单元测试（service/search.rs 中的测试）
    - 手动测试搜索 API 端点
    - 验证 NDJSON 流格式正确
    - 验证多数据源并行搜索正常工作
    - 验证错误处理和错误事件正确返回
    - 验证缓存功能正常（sid 生成和存储）
    - 验证 X-Logseek-SID 响应头正确设置
    - _Requirements: 需求 2.5_

## 阶段 3: 测试覆盖率提升

### 现有测试状态总结

**SearchProcessor 单元测试（已完成 - service/search.rs）：**
- ✅ 路径过滤测试（2个测试）
- ✅ 内容处理测试（6个测试）
- ✅ 结果发送测试（2个测试）
- ✅ grep_context 核心逻辑测试（50+ 个测试，包括编码检测、布尔查询、正则表达式等）

**EntryStream 集成测试（已完成 - service/search.rs）：**
- ✅ tar.gz 归档文件处理测试（10个测试）
- ✅ 多文件并发处理测试
- ✅ 二进制文件跳过测试
- ✅ 复杂查询和路径过滤测试

### 待补充测试

- [ ]* 4. SearchExecutor 单元测试（待实现 - 需要先完成 Phase 2）
  - [ ]* 4.1 创建测试模块和辅助函数
    - 在 service/search_executor.rs 中创建 #[cfg(test)] mod tests
    - 实现 create_test_pool() 辅助函数创建内存数据库
    - 实现 setup_test_sources() 辅助函数准备测试数据
    - _Requirements: 需求 3.1_
  
  - [ ]* 4.2 测试基本搜索功能
    - 测试 SearchExecutor::new() 构造函数
    - 测试 parse_query() 查询解析
    - 测试 get_sources() 数据源配置加载
    - 测试单数据源搜索流程
    - _Requirements: 需求 3.1_
  
  - [ ]* 4.3 测试并发控制
    - 测试 IO Semaphore 正确限制并发数
    - 测试多数据源并行搜索
    - 测试配置的并发数正确应用
    - _Requirements: 需求 3.1_
  
  - [ ]* 4.4 测试错误处理
    - 测试查询解析失败场景
    - 测试数据源配置加载失败
    - 测试部分数据源失败时其他数据源继续工作
    - 测试错误事件正确发送到结果流
    - _Requirements: 需求 3.1_

- [ ]* 5. 为错误转换添加测试
  - [ ]* 5.1 测试 ServiceError 转换
    - 在 api/error.rs 中添加测试模块
    - 测试 ServiceError::ConfigError 到 LogSeekApiError 转换
    - 测试 ServiceError::ProcessingError 到 LogSeekApiError 转换
    - 测试错误上下文信息保留
    - _Requirements: 需求 3.2_
  
  - [ ]* 5.2 测试 HTTP 响应转换
    - 测试 LogSeekApiError 到 HTTP Response 转换
    - 测试 Problem Details JSON 格式正确
    - 测试 HTTP 状态码映射（400/404/500/502）
    - 测试 Content-Type 头正确设置
    - _Requirements: 需求 3.2_

- [ ]* 6. 为 EntryStreamFactory 添加测试
  - [ ]* 6.1 测试 Local 数据源
    - 在 service/entry_stream.rs 中添加测试
    - 测试目录遍历（recursive 和 non-recursive）
    - 测试文件列表处理
    - 测试 tar/tar.gz 归档文件读取
    - _Requirements: 需求 3.3_
  
  - [ ]* 6.2 测试 S3 数据源
    - 创建 mock S3 客户端（使用 mockall 或手动 mock）
    - 测试 S3 对象列举
    - 测试 S3 对象读取和流处理
    - 测试 S3 连接失败处理
    - _Requirements: 需求 3.3_
  
  - [ ]* 6.3 测试 Agent 数据源
    - 创建 mock AgentClient
    - 测试远程搜索调用和结果解析
    - 测试 Agent 连接失败处理
    - 测试 Agent 超时处理
    - _Requirements: 需求 3.3_

- [ ]* 7. 测试覆盖率验证
  - [ ]* 7.1 配置和运行覆盖率工具
    - 安装 cargo-tarpaulin: `cargo install cargo-tarpaulin`
    - 在 backend/logseek 目录运行: `cargo tarpaulin --out Html --output-dir coverage`
    - 生成覆盖率报告并识别未覆盖的代码路径
    - _Requirements: 需求 3.4_
  
  - [ ]* 7.2 补充缺失测试
    - 为边界情况添加测试（空查询、超大结果集等）
    - 为错误路径添加测试（网络错误、解析错误等）
    - 为关键业务逻辑添加测试
    - 目标：服务层测试覆盖率 > 70%
    - _Requirements: 需求 3.4, 3.6_
  
  - [ ]* 7.3 性能基准测试
    - 创建性能基准测试（使用 criterion 或手动计时）
    - 对比重构前后的搜索性能
    - 确保性能下降 < 5%
    - 记录基准测试结果
    - _Requirements: 需求 3.4_

## 验收检查清单

### 阶段 1 验收（已完成）

- [x] ✅ 代码中不再有 `AppError::` 的直接使用（除了 lib.rs 和 api/error.rs 中的类型转换）
- [x] ✅ 所有路由层文件已使用分层错误类型（view.rs, planners.rs, nl2q.rs, starlark_runtime.rs）
- [x] ✅ API 响应格式符合 Problem Details 规范
- [x] ✅ 错误转换逻辑正确（ServiceError -> LogSeekApiError -> HTTP Response）

### 阶段 2 验收（待完成）

- [ ] ✅ SearchExecutor 服务类成功创建并可复用
- [ ] ✅ routes/search.rs 代码行数从 644 行减少到 < 150 行
- [ ] ✅ 所有业务逻辑已从路由层移至服务层
- [ ] ✅ API 行为完全一致（手动测试验证）
- [ ] ✅ 搜索功能正常工作（多数据源、并发控制、缓存）
- [ ] ✅ 错误处理正确（错误事件正确返回）
- [ ] ✅ X-Logseek-SID 响应头正确设置
- [ ] ✅ 所有现有单元测试通过（service/search.rs 中的 60+ 个测试）

### 阶段 3 验收（可选）

- [ ] ✅ 新增单元测试覆盖 SearchExecutor（至少 10 个测试）
- [ ] ✅ 新增错误转换测试（至少 5 个测试）
- [ ] ✅ 新增 EntryStreamFactory 测试（至少 5 个测试）
- [ ] ✅ 服务层测试覆盖率 > 70%
- [ ] ✅ 性能基准测试显示性能下降 < 5%

### 最终验收

- [ ] ✅ 代码审查通过
- [ ] ✅ 文档更新完成（如有必要）
- [ ] ✅ 无回归问题（所有功能正常工作）
