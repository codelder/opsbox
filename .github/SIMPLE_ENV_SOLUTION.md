# 最简版本管理方案

## 🎯 最终方案：每个组件独立定义 env

采用最简单直接的方案：**每个 workflow/action 在自己的 `env` 中定义版本**。

## 📋 实现方式

### 1. 主 Workflow (build-optimized.yml)
```yaml
name: build-optimized

on:
  push:
    branches: [ main ]
    tags: [ 'v*' ]
  pull_request:

jobs:
  # 干净简洁，无需任何版本配置
```

### 2. 可复用 Workflow (build-linux.yml)
```yaml
name: build-linux-reusable

on:
  workflow_call:

env:
  RUST_VERSION: '1.90.0'
  NODE_VERSION: '20'
  MANYLINUX_IMAGE: 'quay.io/pypa/manylinux2014_x86_64:2025.10.10-1'

jobs:
  # 使用 ${{ env.RUST_VERSION }} 等
```

### 3. Actions
```yaml
# setup-frontend/action.yml
env:
  NODE_VERSION: '20'

# setup-rust/action.yml  
env:
  RUST_VERSION: '1.90.0'
```

## ✅ 优势

### 🎯 **极简调用**
```yaml
# 主 workflow 中的调用超级简洁
- uses: ./.github/actions/setup-frontend
- uses: ./.github/actions/setup-rust
- uses: ./.github/workflows/build-linux.yml
```

### 📍 **版本就近原则**
- 每个组件在自己文件顶部声明需要的版本
- 一目了然，无需查找其他文件
- 修改版本时直接在使用的地方修改

### 🔧 **独立管理**
- 每个组件可以独立升级版本
- 不需要全局协调，降低耦合度
- 测试和验证更容易

## 📊 文件结构

```
.github/
├── workflows/
│   ├── build-optimized.yml      # 主流程：无版本配置
│   └── build-linux.yml          # 子流程：有自己的 env
└── actions/
    ├── setup-frontend/
    │   └── action.yml            # 有自己的 env  
    └── setup-rust/
        └── action.yml            # 有自己的 env
```

## 🔄 版本更新

需要更新版本时，比如升级 Rust：
1. 修改 `build-linux.yml` 中的 `RUST_VERSION`
2. 修改 `setup-rust/action.yml` 中的 `RUST_VERSION`  
3. 完成！

## 💡 为什么这是最好的方案

### ✅ **最简单**
- 不需要 Repository Variables 设置
- 不需要复杂的参数传递
- 不需要外部依赖

### ✅ **最直观**
- 版本定义在使用的地方
- 代码即文档
- 容易理解和维护

### ✅ **最灵活**
- 每个组件可以独立演进
- 支持不同组件使用不同版本
- 便于局部测试和验证

## 🎯 核心理念

**"就近原则"** - 版本配置靠近使用的地方，减少查找成本和维护复杂度。

这是最符合软件工程实践的版本管理方案！🎉