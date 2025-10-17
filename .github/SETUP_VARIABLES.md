# Repository Variables 设置指南

## 🎯 需要设置的变量

请在 GitHub 仓库中设置以下 Repository Variables：

### 📋 变量列表

| 变量名 | 值 | 说明 |
|--------|----|----- |
| `RUST_VERSION` | `1.90.0` | Rust 版本 |
| `NODE_VERSION` | `20` | Node.js 版本 |
| `MANYLINUX_IMAGE` | `quay.io/pypa/manylinux2014_x86_64:2025.10.10-1` | Linux 构建 Docker 镜像 |

## 🔧 设置步骤

### 1. 打开仓库设置
- 进入 GitHub 仓库页面
- 点击 **Settings** 选项卡

### 2. 找到变量设置
- 在左侧菜单中找到 **"Secrets and variables"**
- 点击展开，选择 **"Actions"**

### 3. 添加变量
- 点击 **"Variables"** 标签页
- 点击 **"New repository variable"** 按钮

### 4. 逐个添加变量
对每个变量重复以下步骤：
- **Name**: 输入变量名（如 `RUST_VERSION`）
- **Value**: 输入对应的值（如 `1.90.0`）
- 点击 **"Add variable"**

## 🎨 设置截图示例

```
Settings → Secrets and variables → Actions → Variables

┌─────────────────────────────────────────┐
│ Repository variables                     │
├─────────────────────────────────────────┤
│ RUST_VERSION        = 1.90.0           │
│ NODE_VERSION        = 20               │  
│ MANYLINUX_IMAGE     = quay.io/pypa/... │
└─────────────────────────────────────────┘
```

## ✅ 验证设置

设置完成后，可以在任何 workflow 中使用：

```yaml
# 在 workflow 中使用
steps:
  - name: 输出版本信息
    run: |
      echo "Rust版本: ${{ vars.RUST_VERSION }}"
      echo "Node版本: ${{ vars.NODE_VERSION }}"
      echo "Docker镜像: ${{ vars.MANYLINUX_IMAGE }}"
```

## 🔄 更新版本

需要更新版本时：
1. 进入 **Settings → Secrets and variables → Actions → Variables**
2. 点击要修改的变量右侧的 **"Update"** 按钮
3. 修改值，点击 **"Update variable"**
4. 下次运行 workflow 时自动使用新版本

## 💡 优势

### ✅ **简单易用**
- GitHub 原生功能，无需额外代码
- Web 界面操作，直观方便

### ✅ **全局生效**
- 所有 workflows 都可以使用
- 支持 `workflow_call` 的 `with` 参数

### ✅ **即时生效**
- 修改后立即生效，无需重新部署代码
- 不需要重新提交代码

### ✅ **权限控制**
- 可以设置谁能修改这些变量
- 支持组织级别的变量管理

## 🛡️ 注意事项

1. **变量名大小写敏感**: `RUST_VERSION` ≠ `rust_version`
2. **没有引号**: 在界面中输入值时不需要引号
3. **备用值**: workflow 中使用 `${{ vars.RUST_VERSION || '1.90.0' }}` 提供备用值
4. **权限要求**: 需要仓库的管理员权限才能设置变量

设置完成后，所有的版本管理就变得非常简单了！🎉