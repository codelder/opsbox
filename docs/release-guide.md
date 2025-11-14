# 发布指南

## 方法 1: 使用自动化脚本（推荐）

### 快速发布
```bash
# 一键发布（设置版本 + 提交 + 打标签 + 推送）
./scripts/release/release.sh 0.1.0-rc1
```

### 仅设置版本号
```bash
# 只更新版本号，不提交
./scripts/release/set-version.sh 0.1.0-rc1

# 然后手动检查和提交
git diff
git add -A
git commit -m "chore: bump version to 0.1.0-rc1"
git tag v0.1.0-rc1
git push origin v0.1.0-rc1
```

## 方法 2: 手动发布

### 1. 更新版本号

**Backend (Rust):**
```bash
# 更新所有 Cargo.toml
for file in backend/*/Cargo.toml; do
  sed -i 's/^version = ".*"$/version = "0.1.0-rc1"/' "$file"
done

# 更新 Cargo.lock
cd backend && cargo update --workspace && cd ..
```

**Frontend:**
```bash
# 更新 package.json
sed -i 's/"version": ".*"/"version": "0.1.0-rc1"/' web/package.json
```

### 2. 提交和打标签
```bash
git add -A
git commit -m "chore: bump version to 0.1.0-rc1"
git tag -a v0.1.0-rc1 -m "Release v0.1.0-rc1"
git push origin v0.1.0-rc1
```

## 版本号规范

- **正式版本**: `0.1.0`, `1.0.0`
- **候选版本**: `0.1.0-rc1`, `1.0.0-rc2`
- **Beta 版本**: `0.1.0-beta1`
- **Alpha 版本**: `0.1.0-alpha1`

## 发布检查清单

- [ ] 所有测试通过
- [ ] 文档已更新
- [ ] CHANGELOG 已更新
- [ ] 版本号已统一
- [ ] Git 标签已创建
- [ ] 已推送到远程仓库
