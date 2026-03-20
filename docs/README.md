# OpsBox 文档目录

本文档目录按“当前实现说明”和“历史设计记录”两类组织。

## 目录结构

```text
docs/
├── architecture/   # 当前架构与关键设计说明
├── api/            # API 参考
├── examples/       # 示例代码
├── features/       # 功能说明
├── guides/         # 开发与使用指南
├── modules/        # 模块说明
├── performance/    # 性能相关说明
├── plans/          # 历史方案与迭代记录
├── testing/        # 测试说明
└── unused-code-analysis.md
```

## 当前优先阅读

### 入口文档

- [README.md](../README.md)
- [CLAUDE.md](../CLAUDE.md)

### 架构

- [architecture/architecture.md](architecture/architecture.md)
- [architecture/module-architecture.md](architecture/module-architecture.md)
- [architecture/logging-architecture.md](architecture/logging-architecture.md)

### 开发指南

- [guides/query-syntax.md](guides/query-syntax.md)
- [guides/frontend-development.md](guides/frontend-development.md)
- [guides/logging-configuration.md](guides/logging-configuration.md)
- [guides/tracing-usage.md](guides/tracing-usage.md)

### 模块与功能

- [modules/agent-api-spec.md](modules/agent-api-spec.md)
- [modules/agent-manager.md](modules/agent-manager.md)
- [features/file-url.md](features/file-url.md)
- [features/s3-profiles.md](features/s3-profiles.md)

## 文档分类说明

### `architecture/`

描述当前系统结构、模块机制、错误与日志设计。

### `modules/`

对后端模块和 Agent 协议做实现级说明。

### `features/`

描述 ORL、S3 Profiles、Agent 标签等功能行为。

### `guides/`

开发者常用指南，尽量与当前代码目录和接口保持一致。

### `plans/`

历史设计、覆盖率改进方案、迭代记录。

这些文件保留决策上下文，但不应优先作为“当前实现真相来源”。

## 维护原则

- 优先以代码实现为准
- 涉及路由、字段、命令、目录结构变更时同步更新文档
- 设计草案与现状不一致时，优先在当前参考文档中澄清，再保留历史方案

最后审查: 2026-03-20
