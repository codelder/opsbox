# 版本固化策略说明

## 问题解决

原本使用环境变量引用版本号时遇到的问题：
- ❌ `workflow_call` 的 `with` 参数中不能使用 `env` 上下文
- ❌ GitHub Actions 报错：`Unrecognized named-value: 'env'`

## 解决方案

采用**版本固化策略**，直接在 workflow 中明确指定版本号：

### ✅ 修复前后对比

#### 修复前 (有问题)
```yaml
env:
  RUST_VERSION: '1.90.0'
  NODE_VERSION: '20'

jobs:
  build-linux:
    uses: ./.github/workflows/build-linux.yml
    with:
      rust-version: ${{ env.RUST_VERSION }}  # ❌ 错误：无法识别 env
```

#### 修复后 (正确)
```yaml
# 所有版本直接在 workflow 中明确指定，保证构建稳定性

jobs:
  build-linux:
    uses: ./.github/workflows/build-linux.yml
    with:
      rust-version: '1.90.0'  # ✅ 正确：直接使用明确版本
```

## 版本固化的优势

### 🎯 1. **构建稳定性**
- **可预测性**: 每次构建使用完全相同的工具版本
- **可重现性**: 任何时候都能重现相同的构建结果
- **避免意外**: 不会因为工具更新导致构建失败

### 🔒 2. **版本控制明确**
```yaml
# 当前使用的确定版本
rust-version: '1.90.0'           # Rust 1.90.0
node-version: '20'               # Node.js 20.x
manylinux-image: 'quay.io/pypa/manylinux2014_x86_64:2025.10.10-1'  # 固定版本
```

### 📋 3. **维护便利性**
- **集中管理**: 所有版本号在各个 workflow 中明确可见
- **升级控制**: 需要时可以精确控制哪些组件升级
- **回滚简单**: 出问题时可以快速回滚到之前的版本

### 🚀 4. **CI/CD 最佳实践**
- **生产级标准**: 生产环境部署通常要求版本固化
- **团队协作**: 所有团队成员使用相同的构建环境
- **问题排查**: 版本明确有助于问题定位

## 当前版本清单

### 🦀 Rust 生态
- **Rust**: `1.90.0` - 稳定版本，支持项目所需的所有特性
- **Rustup**: 通过 `dtolnay/rust-toolchain@stable` action 管理

### 🟢 Node.js 生态  
- **Node.js**: `20` - LTS 版本，稳定可靠
- **pnpm**: `latest` - 通过 corepack 管理，自动使用最新稳定版

### 🐧 Linux 构建环境
- **Ubuntu**: `22.04` - GitHub Actions 标准 runner
- **Manylinux**: `manylinux2014_x86_64:2025.10.10-1` - 确保 GLIBC 2.17 兼容，固定版本避免意外更新

### 🍎 macOS 构建环境
- **macOS 13** (Intel): 支持 10.15+ 部署目标
- **macOS 14** (Apple Silicon): 支持 11.0+ 部署目标

### 🪟 Windows 构建环境
- **Windows**: `2022` - 最新稳定的 Windows Server 版本

## 版本选择说明

### Manylinux 镜像版本
- **当前版本**: `2025.10.10-1` (2025年10月10日发布)
- **选择原因**: 最新的稳定版本，修复了安全问题和构建工具更新
- **兼容性**: 继续支持 GLIBC 2.17 基线，保证旧系统兼容

### 查看最新版本
```bash
# 查看 manylinux2014_x86_64 的最新标签
curl -s https://quay.io/api/v1/repository/pypa/manylinux2014_x86_64/tag/ | \
  jq -r '.tags[] | select(.name != "latest") | .name' | \
  sort -V | tail -5
```

## 升级策略

### 计划升级时机
1. **Rust 版本**: 每个 minor 版本发布后评估升级
2. **Node.js 版本**: LTS 版本更新时考虑升级  
3. **Manylinux 镜像**: 每月检查一次，安全更新及时升级
4. **系统镜像**: 定期评估 runner 镜像更新

### 升级流程
1. **测试分支验证**: 在独立分支测试新版本
2. **逐步升级**: 先升级 development 构建，后升级 release 构建
3. **回滚准备**: 保留上一个可工作的版本配置

## GitHub Actions 限制说明

### `workflow_call` 上下文限制
在可复用 workflow 的 `with` 参数中，可用的上下文有限：
- ✅ **可用**: `github`, `inputs`, `vars`, 字面值
- ❌ **不可用**: `env`, `secrets`, `jobs`, `steps`

### 解决方案选择
1. **版本固化** (当前采用) - 最稳定可靠
2. **Repository Variables** - 可选方案，但增加复杂性
3. **传递参数** - 适用于需要高度可配置的场景

## 总结

版本固化策略的核心价值：
- 🎯 **稳定性优先**: 确保构建环境的一致性
- 🔒 **安全可控**: 避免意外的版本更新带来的风险  
- 📋 **维护友好**: 版本信息清晰明确，便于管理
- 🚀 **生产就绪**: 符合生产级 CI/CD 的最佳实践

这种方式虽然需要手动管理版本升级，但换来了最大的构建稳定性和可预测性！