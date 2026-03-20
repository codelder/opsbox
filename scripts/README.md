# 脚本工具目录

本文档列出仓库中当前存在的脚本工具。

## 目录结构

```text
scripts/
├── build/
│   ├── build-frontend.mjs
│   ├── build-frontend.sh
│   ├── cross-build-linux.sh
│   ├── package-release.sh
│   └── remote-build.sh
├── generate/
│   ├── generate-encoding-test-files.py
│   ├── generate-gbk-test-file.py
│   ├── generate-home-logs.py
│   ├── generate-million-logs.py
│   ├── generate-multiple-home-logs.py
│   └── generate-test-logs.py
├── monitor/
│   ├── analyze_coverage.sh
│   └── run_tests_with_monitoring.sh
├── run/
│   ├── run-agent.sh
│   ├── run-opsbox-server-jemalloc-aggressive.sh
│   ├── start-agent.sh
│   └── start-server.sh
└── test/
    ├── bench-logging-performance.sh
    └── bench-ndjson.sh
```

## 分类说明

### `run/`

- `start-server.sh`：快速启动 `opsbox-server`
- `start-agent.sh`：快速启动 `opsbox-agent`
- `run-agent.sh`：带完整参数的 Agent 启动脚本
- `run-opsbox-server-jemalloc-aggressive.sh`：特殊内存配置下运行服务

### `build/`

- `build-frontend.sh`：类 Unix 下构建前端
- `build-frontend.mjs`：跨平台构建前端
- `cross-build-linux.sh`：交叉构建 Linux 产物
- `package-release.sh`：打包发布产物
- `remote-build.sh`：远程构建辅助脚本

### `test/`

- `bench-ndjson.sh`：NDJSON 搜索压测
- `bench-logging-performance.sh`：日志接口性能压测

### `monitor/`

- `run_tests_with_monitoring.sh`：带监控执行测试
- `analyze_coverage.sh`：覆盖率分析

### `generate/`

用于生成测试日志、编码样例和性能测试数据。

## 快速链接

- [项目主 README](../README.md)
- [CLAUDE.md](../CLAUDE.md)
- [docs/README.md](../docs/README.md)

## 使用说明

- 在仓库根目录执行脚本
- 执行前确认依赖已安装：Rust、Node.js、Python、pnpm 等
- Shell 脚本如无执行权限可先运行：

```bash
chmod +x scripts/**/*.sh
```
