# OpsBox 文档目录

本文档目录提供了 OpsBox 项目的完整文档集合。

## 📁 目录结构

```
docs/
├── architecture/          # 架构设计文档
│   ├── architecture.md                    # 项目架构复盘分析
│   ├── error-handling-architecture.md     # 错误处理架构设计
│   ├── error-handling-quick-reference.md  # 错误处理快速参考
│   └── module-architecture.md              # 模块化架构设计
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
│   ├── query-rag.md                      # 查询语法 RAG 资料
│   └── query-syntax.md                    # 查询字符串规范
│
├── archive/               # 历史参考文档
│   └── cpu-tuning-analysis.md            # CPU 调优分析（历史参考）
│
└── examples/              # 示例代码
    └── coordinator_integration_example.rs # Coordinator 集成示例
```

## 📚 文档分类说明

### 架构文档 (`architecture/`)
系统架构设计、模块化设计、错误处理架构等核心设计文档。

### 模块文档 (`modules/`)
各个模块的详细文档，包括 API 规范和模块说明。

### 功能文档 (`features/`)
具体功能的详细说明，包括设计思路、使用方法等。

### 使用指南 (`guides/`)
面向开发者和用户的使用指南，包括开发指南、配置说明等。

### 历史参考 (`archive/`)
已过时但保留作为历史参考的文档。

### 示例代码 (`examples/`)
代码示例和集成示例。

## 🔗 快速链接

- [项目主 README](../README.md)
- [WARP 开发指南](../WARP.md)
- [架构复盘分析](architecture/architecture.md)
- [模块化架构](architecture/module-architecture.md)
- [Agent API 规范](modules/agent-api-spec.md)
- [前端开发指南](guides/frontend-development.md)

## 📝 文档维护

所有文档都包含版本信息：
- **文档版本**: v1.0
- **最后更新**: 2025-11-10

文档更新时请同步更新版本信息和最后更新日期。

