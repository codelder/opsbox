# GitHub Actions 路径修复说明

## 发现的问题

在检查项目结构后，发现 GitHub Actions 脚本中使用了错误的路径：

### 实际项目结构
```
opsbox/
├── backend/          # Rust 后端代码
│   ├── Cargo.toml
│   ├── Cargo.lock
│   ├── api-gateway/
│   ├── opsbox-core/
│   └── logseek/
└── web/             # 前端代码
    ├── package.json
    └── pnpm-lock.yaml
```

### 脚本中的错误路径
- ❌ 前端目录: `ui/` → ✅ 应为: `web/`
- ❌ 后端目录: `server/` → ✅ 应为: `backend/`

## 修复的文件

### 1. `.github/actions/setup-frontend/action.yml`
```diff
- default: 'ui'
+ default: 'web'
```

### 2. `.github/workflows/build-optimized.yml`

#### 缓存路径
```diff
- path: server/target-${{ matrix.target.name }}
+ path: backend/target-${{ matrix.target.name }}

- hashFiles('server/Cargo.lock')
+ hashFiles('backend/Cargo.lock')
```

#### 构建路径
```diff
- export CARGO_TARGET_DIR="${PWD}/server/target-musl"
+ export CARGO_TARGET_DIR="${PWD}/backend/target-musl"

- --manifest-path server/Cargo.toml
+ --manifest-path backend/Cargo.toml
```

#### 二进制文件路径
```diff
- server/target-musl/x86_64-unknown-linux-musl/release/api-gateway
+ backend/target-musl/x86_64-unknown-linux-musl/release/api-gateway

- server/target/release/api-gateway
+ backend/target/release/api-gateway
```

### 3. `.github/ACTIONS_MIGRATION.md`
```diff
- 检查 `ui/pnpm-lock.yaml` 文件是否存在
+ 检查 `web/pnpm-lock.yaml` 文件是否存在

- 检查 `server/Cargo.lock` 文件是否存在  
+ 检查 `backend/Cargo.lock` 文件是否存在
```

## 修复验证

修复后的路径现在与项目实际结构一致：

✅ **前端构建**: 
- 工作目录: `web/`
- 依赖文件: `web/pnpm-lock.yaml`
- 缓存路径: `web/pnpm-lock.yaml`

✅ **后端构建**:
- 工作目录: `backend/`
- Cargo 清单: `backend/Cargo.toml`
- 依赖锁文件: `backend/Cargo.lock`
- 构建目录: `backend/target/`

✅ **构建产物**:
- musl 构建: `backend/target-musl/x86_64-unknown-linux-musl/release/api-gateway`
- gnu 构建: `backend/target-gnu/x86_64-unknown-linux-gnu/release/api-gateway`
- 其他平台: `backend/target/release/api-gateway[.exe]`

## 影响范围

这些路径修复不会影响：
- ✅ CI 触发条件
- ✅ 构建目标和平台
- ✅ Artifact 名称和结构  
- ✅ 发布流程

只是修正了文件系统路径，使脚本能够正确找到项目文件。