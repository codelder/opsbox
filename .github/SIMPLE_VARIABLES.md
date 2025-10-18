# 简单的全局版本管理方案

## GitHub Repository Variables

最简单的方法是使用 GitHub 的 Repository Variables 功能：

### 🎯 设置方式

在 GitHub 仓库页面：
1. 进入 **Settings** → **Secrets and variables** → **Actions**
2. 点击 **Variables** 标签页
3. 添加以下变量：

| 变量名 | 值 | 说明 |
|--------|----|----- |
| `RUST_VERSION` | `1.90.0` | Rust 版本 |
| `NODE_VERSION` | `20` | Node.js 版本 |
| `MANYLINUX_IMAGE` | `quay.io/pypa/manylinux2014_x86_64:2025.10.10-1` | Docker 镜像 |

### 🔧 使用方式

在 workflow 中直接使用：

```yaml
jobs:
  build:
    uses: ./.github/workflows/build-linux.yml
    with:
      rust-version: ${{ vars.RUST_VERSION }}      # ✅ 可以在 with 中使用
      node-version: ${{ vars.NODE_VERSION }}
      manylinux-image: ${{ vars.MANYLINUX_IMAGE }}
  
  quality-check:
    runs-on: ubuntu-22.04
    steps:
      - name: Setup Rust
        uses: ./.github/actions/setup-rust
        with:
          toolchain: ${{ vars.RUST_VERSION }}      # ✅ 可以在 with 中使用
```

### 📋 完整变量列表

建议添加的 Repository Variables：

```
RUST_VERSION = 1.90.0
NODE_VERSION = 20
MANYLINUX_IMAGE = quay.io/pypa/manylinux2014_x86_64:2025.10.10-1
UBUNTU_VERSION = 22.04
MACOS_INTEL_VERSION = 13
MACOS_ARM_VERSION = 14
WINDOWS_VERSION = 2022
MACOS_INTEL_TARGET = 10.15
MACOS_ARM_TARGET = 11.0
```

## 优势

### ✅ **简单直接**
- 不需要额外的 action 或复杂逻辑
- GitHub 原生支持，无需自定义代码

### ✅ **全局生效**
- 所有 workflows 都可以使用 `${{ vars.VARIABLE_NAME }}`
- 包括可复用 workflows 的 `with` 参数

### ✅ **Web 界面管理**
- 通过 GitHub 界面直接修改
- 无需修改代码，立即生效

### ✅ **权限控制**
- 可以设置谁能修改这些变量
- 支持环境级别的变量覆盖

## 当前的临时方案

如果不想使用 Repository Variables，最简单的方式是直接写在 workflow 文件的顶部：

```yaml
# .github/workflows/build-optimized.yml
name: build-optimized

# 版本配置 - 需要更新版本时修改这里
env:
  RUST_VERSION: '1.90.0'
  NODE_VERSION: '20'
  MANYLINUX_IMAGE: 'quay.io/pypa/manylinux2014_x86_64:2025.10.10-1'

jobs:
  # 在普通 steps 中可以使用 env
  quality-check:
    steps:
      - name: Setup Rust
        uses: ./.github/actions/setup-rust
        with:
          toolchain: ${{ env.RUST_VERSION }}  # ✅ 这里可以用

  # 但在 workflow_call 的 with 中不能用 env
  build-linux:
    uses: ./.github/workflows/build-linux.yml
    with:
      rust-version: '1.90.0'  # ❌ 这里只能写死或用 vars
```

## 推荐方案

**最简单**: 使用 Repository Variables（推荐）
- 设置一次，到处使用
- Web 界面管理，方便更新
- 支持所有使用场景

**次简单**: 在每个 workflow 顶部定义 env
- 文件内可以复用
- 但跨文件仍需要重复定义

选择哪种方案取决于你的偏好和团队的管理方式。