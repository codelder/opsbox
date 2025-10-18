# 最终简化方案：Actions 内置版本管理

## 🎯 解决方案

最终采用了最简单的方案：**让 actions 和 workflows 直接读取 Repository Variables**，无需通过参数传递。

## 🔧 实现方式

### 1. Repository Variables 设置
在 GitHub 仓库中设置全局变量：
```
RUST_VERSION = 1.90.0
NODE_VERSION = 20
MANYLINUX_IMAGE = quay.io/pypa/manylinux2014_x86_64:2025.10.10-1
```

### 2. Actions 内置版本读取

#### setup-frontend action
```yaml
# 之前：需要传递参数
- name: Setup Frontend
  uses: ./.github/actions/setup-frontend
  with:
    node-version: ${{ vars.NODE_VERSION }}

# 现在：action 内部直接读取
- name: Setup Frontend
  uses: ./.github/actions/setup-frontend
  # 无需 with 参数！
```

#### setup-rust action  
```yaml
# 之前：需要传递参数
- name: Setup Rust
  uses: ./.github/actions/setup-rust
  with:
    toolchain: ${{ vars.RUST_VERSION }}

# 现在：action 内部直接读取
- name: Setup Rust
  uses: ./.github/actions/setup-rust
  # 无需 with 参数！
```

### 3. 可复用 Workflow 简化

#### build-linux.yml
```yaml
# 之前：需要定义输入参数
on:
  workflow_call:
    inputs:
      rust-version: { type: string }
      node-version: { type: string }
      manylinux-image: { type: string }

# 现在：无需任何输入参数
on:
  workflow_call:
  # 空的！workflow 内部直接读取 vars
```

#### 调用方式
```yaml
# 之前：需要传递所有参数
build-linux:
  uses: ./.github/workflows/build-linux.yml
  with:
    rust-version: ${{ vars.RUST_VERSION }}
    node-version: ${{ vars.NODE_VERSION }}
    manylinux-image: ${{ vars.MANYLINUX_IMAGE }}

# 现在：无需任何参数
build-linux:
  uses: ./.github/workflows/build-linux.yml
  # 无需 with！
```

## 📊 对比效果

### 代码量对比
| 组件 | 之前行数 | 现在行数 | 减少 |
|------|----------|----------|------|
| 主 workflow | ~80 行 | ~45 行 | **44%** |
| setup-frontend | ~25 行 | ~20 行 | **20%** |
| setup-rust | ~30 行 | ~25 行 | **17%** |
| build-linux | ~60 行 | ~50 行 | **17%** |

### 维护复杂度
| 方面 | 之前 | 现在 |
|------|------|------|
| 参数传递 | 需要在每个调用处传递 | 自动读取，无需传递 |
| 版本更新 | 修改多处调用代码 | 只需修改 Repository Variables |
| 新增 action | 需要定义输入参数和传递逻辑 | 直接使用 vars，无需参数 |

## 🚀 使用体验

### ✅ **极简调用**
```yaml
# 所有 actions 都变成零参数调用
- uses: ./.github/actions/setup-frontend
- uses: ./.github/actions/setup-rust  
- uses: ./.github/workflows/build-linux.yml
```

### ✅ **全局一致**
- 所有地方自动使用相同版本
- Repository Variables 修改后立即全局生效
- 无需担心参数传递遗漏

### ✅ **扩展友好**
- 新增 action 无需考虑参数设计
- 新增版本配置只需在 Repository Variables 添加
- actions 内部可以任意使用 `${{ vars.* }}`

## 💡 关键优势

### 1. **最少样板代码**
- 消除了所有版本相关的参数传递
- actions 和 workflows 都变得更简洁
- 调用方无需关心版本细节

### 2. **真正的全局配置**
- Repository Variables 是 GitHub 原生的全局配置
- 修改一处，立即全局生效
- 支持权限控制和审计

### 3. **向后兼容**
- 使用 `${{ vars.VERSION || 'default' }}` 提供备用值
- 即使没有设置 Repository Variables 也能正常工作
- 渐进式迁移，无破坏性变更

## 🎯 最终架构

```
Repository Variables (GitHub 设置)
    ↓ (自动读取)
Actions/Workflows (内置版本管理)
    ↓ (零参数调用)
使用方 (极简调用)
```

这是我们能找到的最简洁、最实用的版本管理方案！🎉

## 📋 迁移完成清单

- [x] 设置 Repository Variables
- [x] 修改 setup-frontend action 内置版本读取
- [x] 修改 setup-rust action 内置版本读取  
- [x] 修改 build-linux workflow 内置版本读取
- [x] 简化主 workflow 删除所有 with 参数
- [x] 清理过期的复杂配置文件

现在版本管理变得超级简单！