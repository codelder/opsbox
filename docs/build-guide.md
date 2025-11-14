# 构建指南

## 前置要求

### macOS 交叉编译到 Linux

1. **Docker Desktop**
   ```bash
   # 下载安装: https://www.docker.com/products/docker-desktop
   # 确保 Docker 正在运行
   docker info
   ```

2. **cross 工具**（自动安装）
   ```bash
   # 脚本会自动安装，或手动安装：
   cargo install cross --git https://github.com/cross-rs/cross
   ```

## 快速开始

### 本地构建（macOS/Linux）
```bash
# 构建所有组件
make build

# 只构建后端
make build-backend

# 只构建前端
make build-frontend
```

### 交叉编译到 Linux
```bash
# Release 版本（推荐）
make build-linux

# Debug 版本
make build-linux-debug

# 打包发布版本（编译 + 打包）
make package
```

## 手动构建

### 本地构建
```bash
cd backend
cargo build --release
```

### 交叉编译
```bash
# 使用 cross 工具
cd backend
cross build --release --target x86_64-unknown-linux-musl
```

## 输出文件

### 本地构建
```
backend/target/release/opsbox-server
backend/target/release/opsbox-agent
```

### Linux 交叉编译
```
backend/target/x86_64-unknown-linux-musl/release/opsbox-server
backend/target/x86_64-unknown-linux-musl/release/opsbox-agent
```

### 打包后
```
dist/opsbox-{version}-linux-x86_64.tar.gz
```

## 常见问题

### Docker 未运行
```
❌ 错误: Docker 未运行
```
**解决**: 启动 Docker Desktop

### cross 安装失败
```bash
# 使用国内镜像
export CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
cargo install cross --git https://github.com/cross-rs/cross
```

### 编译慢
- 首次编译需要下载 Docker 镜像，会比较慢
- 后续编译会使用缓存，速度会快很多
