# 实现计划：Tracing 日志系统重构

## 任务列表

- [x] 1. 更新依赖和基础设施
  - 在 workspace Cargo.toml 中添加 tracing 相关依赖
  - 在各个 crate 中替换 log 依赖为 tracing
  - 验证依赖编译通过
  - _需求: 1.1, 1.2, 1.3, 1.4_

- [x] 2. 实现核心 logging 模块
- [x] 2.1 创建 opsbox-core 的 logging 模块
  - 创建 `backend/opsbox-core/src/logging.rs` 文件
  - 实现 `LogConfig` 结构体和 `LogLevel` 枚举
  - 实现 `init()` 函数，配置 tracing-subscriber
  - 实现 Console Layer（带彩色输出）
  - 实现 File Layer（使用 RollingFileAppender）
  - 实现 `ReloadHandle` 用于动态修改日志级别
  - 导出 logging 模块到 `lib.rs`
  - _需求: 1.1, 2.1, 2.2, 2.3, 2.4, 2.5, 4.1, 4.2, 8.1, 8.2, 8.3, 8.4, 8.5, 8.6, 8.7, 9.1, 9.2, 9.3, 9.4_

- [x] 2.2 实现日志配置数据库 schema
  - 创建 `backend/opsbox-core/src/logging/schema.sql` 迁移文件
  - 定义 `log_config` 表结构
  - 实现数据库迁移函数
  - _需求: 3.4, 3.5_

- [x] 2.3 实现日志配置 Repository
  - 创建 `backend/opsbox-core/src/logging/repository.rs` 文件
  - 实现 `LogConfigRepository` 结构体
  - 实现 `get()` 方法获取配置
  - 实现 `update_level()` 方法更新日志级别
  - 实现 `update_retention()` 方法更新保留数量
  - _需求: 3.1, 3.2, 3.3, 3.4, 3.5, 5.1, 5.2_

- [x] 2.4 编写 logging 模块单元测试
  - 测试 LogConfig 解析
  - 测试 LogLevel 转换
  - 测试 Repository CRUD 操作
  - _需求: 1.5_

- [x] 3. 更新 Server 日志系统
- [x] 3.1 更新 Server 配置和命令行参数
  - 在 `backend/opsbox-server/src/config.rs` 中添加 `log_dir` 和 `log_retention` 参数
  - 实现默认值逻辑（`~/.opsbox/logs`）
  - _需求: 6.1, 6.3, 6.5_

- [x] 3.2 替换 Server logging 模块
  - 删除 `backend/opsbox-server/src/logging.rs` 中的旧实现
  - 使用 opsbox-core 的 logging 模块重新实现
  - 在 `main.rs` 中调用新的 `init()` 函数
  - 保存 `ReloadHandle` 到全局状态
  - _需求: 1.1, 2.1, 2.2, 2.3, 2.4, 2.5, 4.1, 6.3, 6.5, 6.6_

- [x] 3.3 实现 Server 日志配置 API
  - 创建 `backend/opsbox-server/src/log_routes.rs` 文件
  - 实现 `GET /api/v1/log/config` 端点
  - 实现 `PUT /api/v1/log/level` 端点
  - 实现 `PUT /api/v1/log/retention` 端点
  - 在 `main.rs` 中注册路由
  - _需求: 5.1, 5.2, 5.3, 5.4, 5.5_

- [x] 3.4 更新 Server 所有日志调用
  - 将所有 `log::info!` 替换为 `tracing::info!`
  - 将所有 `log::debug!` 替换为 `tracing::debug!`
  - 将所有 `log::warn!` 替换为 `tracing::warn!`
  - 将所有 `log::error!` 替换为 `tracing::error!`
  - 将所有 `log::trace!` 替换为 `tracing::trace!`
  - _需求: 1.5_

- [x] 3.5 编写 Server 日志 API 集成测试
  - 测试获取日志配置
  - 测试更新日志级别
  - 测试更新日志保留数量
  - 测试参数验证
  - _需求: 5.4_

- [x] 4. 更新 Agent 日志系统
- [x] 4.1 更新 Agent 配置和命令行参数
  - 在 `backend/agent/src/main.rs` 的 `Args` 中添加 `log_dir` 和 `log_retention` 参数
  - 在 `AgentConfig` 中添加对应字段
  - 实现默认值逻辑（`~/.opsbox-agent/logs`）
  - _需求: 6.2, 6.4, 6.5_

- [x] 4.2 替换 Agent logging 初始化
  - 在 `backend/agent/src/main.rs` 中使用 opsbox-core 的 logging 模块
  - 调用 `logging::init()` 函数
  - 保存 `ReloadHandle` 到全局状态或 `AgentConfig`
  - _需求: 1.2, 2.1, 2.2, 2.3, 2.4, 2.5, 4.2, 6.4, 6.5, 6.6_

- [x] 4.3 实现 Agent 日志配置 API
  - 在 `backend/agent/src/main.rs` 中添加日志配置路由
  - 实现 `GET /api/v1/log/config` 端点
  - 实现 `PUT /api/v1/log/level` 端点
  - 实现 `PUT /api/v1/log/retention` 端点
  - _需求: 5.1, 5.2, 5.3, 5.4, 5.5_

- [x] 4.4 更新 Agent 所有日志调用
  - 将所有 `log::info!` 替换为 `tracing::info!`
  - 将所有 `log::debug!` 替换为 `tracing::debug!`
  - 将所有 `log::warn!` 替换为 `tracing::warn!`
  - 将所有 `log::error!` 替换为 `tracing::error!`
  - 将所有 `log::trace!` 替换为 `tracing::trace!`
  - _需求: 1.5_

- [x] 4.5 编写 Agent 日志 API 集成测试
  - 测试获取日志配置
  - 测试更新日志级别
  - 测试更新日志保留数量
  - 测试参数验证
  - _需求: 5.4_

- [x] 5. 实现 Agent Manager 代理功能
- [x] 5.1 添加 Agent 日志配置代理路由
  - 在 `backend/agent-manager/src/routes.rs` 中添加代理路由
  - 实现 `GET /api/v1/agents/{agent_id}/log/config` 端点
  - 实现 `PUT /api/v1/agents/{agent_id}/log/level` 端点
  - 实现 `PUT /api/v1/agents/{agent_id}/log/retention` 端点
  - 从 Agent 标签中提取 host 和 listen_port
  - 使用 reqwest 转发请求到 Agent
  - 实现错误处理（404, 502, 504, 500）
  - _需求: 5.1, 5.2, 5.3_

- [x] 5.2 编写代理功能集成测试
  - 测试代理获取配置
  - 测试代理更新级别
  - 测试代理更新保留数量
  - 测试 Agent 离线场景
  - 测试 Agent 不存在场景
  - _需求: 5.4_

- [x] 6. 更新其他 crate 的日志调用
- [x] 6.1 更新 opsbox-core 日志调用
  - 将所有 `use log::*` 替换为 `use tracing::*`
  - 将所有日志宏调用替换为 tracing 版本
  - _需求: 1.3, 1.5_

- [x] 6.2 更新 logseek 日志调用
  - 将所有 `use log::*` 替换为 `use tracing::*`
  - 将所有日志宏调用替换为 tracing 版本
  - _需求: 1.4, 1.5_

- [x] 6.3 更新 agent-manager 日志调用
  - 将所有 `use log::*` 替换为 `use tracing::*`
  - 将所有日志宏调用替换为 tracing 版本
  - _需求: 1.5_

- [x] 7. 实现前端日志管理界面
- [x] 7.1 创建 ServerLogSettings 组件
  - 创建 `web/src/routes/settings/ServerLogSettings.svelte` 文件
  - 实现日志级别选择器
  - 实现日志保留数量输入
  - 实现日志路径显示（只读）
  - 实现保存和重置按钮
  - 实现加载状态和错误处理
  - 添加提示信息
  - _需求: 7.1, 7.2, 7.7, 7.8_

- [x] 7.2 更新设置页面主文件
  - 在 `web/src/routes/settings/+page.svelte` 中添加 "Server 日志" 标签
  - 导入并渲染 ServerLogSettings 组件
  - _需求: 7.1, 7.2_

- [x] 7.3 更新 AgentManagement 组件
  - 在 `web/src/routes/settings/AgentManagement.svelte` 中添加日志设置展开区域
  - 实现日志级别选择器
  - 实现日志保留数量输入
  - 实现保存按钮
  - 实现 Agent 离线状态检测和禁用逻辑
  - 添加提示信息
  - _需求: 7.3, 7.4, 7.7, 7.8_

- [x] 7.4 实现前端 API 调用
  - 实现 `fetchServerLogConfig()` 函数
  - 实现 `updateServerLogLevel()` 函数
  - 实现 `updateServerLogRetention()` 函数
  - 实现 `fetchAgentLogConfig()` 函数
  - 实现 `updateAgentLogLevel()` 函数
  - 实现 `updateAgentLogRetention()` 函数
  - 实现错误处理和提示
  - _需求: 7.7, 7.8_

- [x] 7.5 编写前端组件测试
  - 测试 ServerLogSettings 组件渲染
  - 测试 AgentManagement 日志设置渲染
  - 测试表单交互
  - 测试 API 调用
  - 测试错误处理
  - _需求: 7.7, 7.8_

- [x] 8. 验证和优化
- [x] 8.1 端到端测试
  - 测试 Server 启动时日志初始化
  - 测试 Agent 启动时日志初始化
  - 测试日志文件滚动
  - 测试日志保留策略
  - 测试动态修改日志级别
  - 测试前端界面操作
  - _需求: 2.1, 2.2, 2.3, 2.4, 2.5, 3.1, 3.2, 3.3, 5.1, 5.2, 5.3, 6.1, 6.2, 6.3, 6.4, 6.5, 6.6_

- [x] 8.2 性能测试
  - 测试高并发日志写入性能
  - 测试内存使用
  - 测试 CPU 使用
  - 测试磁盘 I/O
  - 对比 log 和 tracing 的性能差异
  - _需求: 9.1, 9.2, 9.3_

- [x] 8.3 优化日志级别设置
  - 审查所有日志调用，确保使用合适的级别
  - 减少 INFO 级别的日志输出
  - 将详细信息移到 DEBUG 级别
  - 确保 ERROR 和 WARN 级别的日志有意义
  - _需求: 4.3, 4.4, 4.5, 4.6, 4.7_

- [x] 8.4 清理旧代码
  - 删除 log 和 env_logger 依赖
  - 删除旧的 logging.rs 文件（如果有备份）
  - 清理未使用的导入
  - _需求: 1.1, 1.2, 1.3, 1.4_

- [x] 9. 文档更新
- [x] 9.1 更新用户文档
  - 更新启动参数文档（--log-dir, --log-retention）
  - 更新日志配置说明
  - 添加日志管理界面使用说明
  - 添加日志级别说明
  - 添加故障排查指南
  - _需求: 6.1, 6.2, 6.3, 6.4_

- [x] 9.2 更新开发者文档
  - 更新日志系统架构文档
  - 添加 tracing 使用指南
  - 添加日志最佳实践
  - 更新 API 文档
  - _需求: 1.1, 1.2, 1.3, 1.4, 1.5_

- [x] 9.3 更新 CHANGELOG
  - 记录日志系统重构
  - 记录新增功能
  - 记录 Breaking Changes
  - 记录迁移指南
  - _需求: 1.1, 1.2, 1.3, 1.4, 1.5, 2.1, 2.2, 2.3, 2.4, 2.5, 3.1, 3.2, 3.3, 5.1, 5.2, 5.3, 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 7.1, 7.2, 7.3, 7.4, 7.5, 7.6, 7.7, 7.8_
