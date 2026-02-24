# OpsBox 文档目录

本文档目录提供了 OpsBox 项目的完整文档集合。

## 📁 目录结构

```
docs/
├── architecture/          # 架构设计文档
│   ├── architecture.md                    # 项目架构分析
│   ├── error-handling-architecture.md     # 错误处理架构设计
│   ├── error-handling-quick-reference.md  # 错误处理快速参考
│   ├── logging-architecture.md            # 日志系统架构设计
│   └── module-architecture.md             # 模块化架构设计
│
├── api/                   # API 文档
│   └── logging-api.md                     # 日志配置 API 参考
│
├── modules/               # 模块文档
│   ├── agent-api-spec.md                  # Agent HTTP API 规范
│   └── agent-manager.md                   # Agent Manager 模块文档
│
├── features/              # 功能文档
│   ├── agent-tag-api.md                  # Agent 标签 API
│   ├── agent-tag-management.md           # Agent 标签管理策略
│   ├── agent-tags.md                     # Agent 标签功能
│   ├── file-url.md                       # 文件 URL 设计方案
│   └── s3-profiles.md                    # S3 Profile 管理功能
│
├── guides/                # 使用指南
│   ├── cpu-resource-control.md           # CPU 资源控制指南
│   ├── frontend-development.md           # 前端开发指南
│   ├── logging-configuration.md          # 日志配置和管理指南
│   ├── query-rag.md                      # 查询语法 RAG 资料
│   ├── query-syntax.md                   # 查询字符串规范
│   └── tracing-usage.md                  # Tracing 使用指南
│
├── performance/           # 性能文档
│   └── memory-management.md              # 内存管理优化（mimalloc）
│
├── testing/               # 测试文档
│   ├── logging-e2e-test-checklist.md     # 日志 E2E 测试清单
│   └── test-monitoring-guide.md          # 测试监控指南
│
└── archive/               # 历史参考文档
    └── cpu-tuning-analysis.md            # CPU 调优分析（历史参考）
```

## 📚 文档分类说明

### 架构文档 (`architecture/`)
系统架构设计、模块化设计、错误处理架构、日志系统架构等核心设计文档。

### API 文档 (`api/`)
REST API 接口文档，包括请求/响应格式、错误处理等。

### 模块文档 (`modules/`)
各个模块的详细文档，包括 API 规范和模块说明。

### 功能文档 (`features/`)
具体功能的详细说明，包括设计思路、使用方法等。

### 使用指南 (`guides/`)
面向开发者和用户的使用指南，包括开发指南、配置说明、日志管理等。

### 测试文档 (`testing/`)
测试相关文档，包括测试清单、测试监控指南等。

### 历史参考 (`archive/`)
已过时但保留作为历史参考的文档。

## 🔗 快速链接

### 项目文档
- [项目主 README](../README.md)
- [WARP 开发指南](../WARP.md)
- [CHANGELOG](../CHANGELOG.md)

### 架构文档
- [架构分析](architecture/architecture.md)
- [模块化架构](architecture/module-architecture.md)
- [日志系统架构](architecture/logging-architecture.md)

### 使用指南
- [日志配置指南](guides/logging-configuration.md)
- [Tracing 使用指南](guides/tracing-usage.md)
- [前端开发指南](guides/frontend-development.md)

### API 文档
- [日志配置 API](api/logging-api.md)
- [Agent API 规范](modules/agent-api-spec.md)

## 📝 文档维护

文档更新时请同步更新最后更新日期。

**最后审查**: 2026-02-24
