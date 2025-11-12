# 脚本工具目录

本文档目录提供了 OpsBox 项目的所有脚本工具。

## 📁 目录结构

```
scripts/
├── run/                        # 运行脚本
│   ├── start-server.sh                    # 启动 Server
│   ├── start-agent.sh                     # 启动 Agent
│   ├── run-agent.sh                       # 运行 Agent（完整配置）
│   └── run-opsbox-server-jemalloc-aggressive.sh  # 运行 Server（jemalloc 配置）
│
├── test/                        # 测试脚本
│   ├── test-agent-api.sh                 # Agent API 测试
│   ├── test-graceful-shutdown.sh         # 优雅关闭测试
│   └── bench-ndjson.sh                   # NDJSON 性能测试
│
├── build/                       # 构建脚本
│   ├── build-frontend.sh                 # 构建前端（Shell）
│   └── build-frontend.mjs                 # 构建前端（Node.js）
│
└── generate/                    # 数据生成脚本
    ├── generate-test-logs.py              # 生成测试日志
    ├── generate-encoding-test-files.py    # 生成编码测试文件
    ├── generate-gbk-test-file.py          # 生成 GBK 测试文件
    ├── generate-home-logs.py              # 生成 home 日志
    ├── generate-million-logs.py           # 生成百万日志
    └── generate-multiple-home-logs.py     # 生成多个 home 日志
```

## 📚 脚本分类说明

### 运行脚本 (`run/`)
用于启动和运行 OpsBox 服务的脚本。

- **start-server.sh**: 快速启动 Server（开发模式）
- **start-agent.sh**: 快速启动 Agent（开发模式）
- **run-agent.sh**: 运行 Agent，包含完整的环境变量配置
- **run-opsbox-server-jemalloc-aggressive.sh**: 使用 jemalloc 内存分配器的 Server 运行脚本

### 测试脚本 (`test/`)
用于测试和性能评估的脚本。

- **test-agent-api.sh**: 测试 Agent HTTP API 端点
- **test-graceful-shutdown.sh**: 测试服务的优雅关闭功能
- **bench-ndjson.sh**: NDJSON 流式检索性能压测脚本

### 构建脚本 (`build/`)
用于构建项目的脚本。

- **build-frontend.sh**: 构建前端静态资源（Shell 版本）
- **build-frontend.mjs**: 构建前端静态资源（Node.js 版本）

### 数据生成脚本 (`generate/`)
用于生成测试数据和测试文件的脚本。

- **generate-test-logs.py**: 生成各种类型的测试日志文件
- **generate-encoding-test-files.py**: 生成不同编码格式的测试文件
- **generate-gbk-test-file.py**: 生成 GBK 编码的测试文件
- **generate-home-logs.py**: 生成 home 目录下的日志文件
- **generate-million-logs.py**: 生成大量日志文件用于性能测试
- **generate-multiple-home-logs.py**: 生成多个 home 目录的日志文件

## 🔗 快速链接

- [项目主 README](../README.md)
- [WARP 开发指南](../WARP.md)

## 📝 使用说明

所有脚本都使用统一的命名规范：
- 全小写字母
- 使用连字符 (`-`) 分隔单词
- 示例：`start-server.sh`, `test-agent-api.sh`

运行脚本时，请确保：
1. 已安装所需的依赖（Rust、Node.js、Python 等）
2. 脚本具有执行权限：`chmod +x scripts/**/*.sh`
3. 在项目根目录下运行脚本

