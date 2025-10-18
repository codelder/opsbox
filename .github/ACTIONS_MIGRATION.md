# GitHub Actions 优化迁移说明

## 概览

从 `build-matrix.yml` 迁移到新的优化版本 `build-optimized.yml`，主要改进包括：

## 主要优化

### 1. ✅ 代码复用
- **Composite Actions**: 创建了可复用的 actions
  - `.github/actions/setup-frontend/`: 前端设置和构建
  - `.github/actions/setup-rust/`: Rust 环境设置和质量检查
- **减少重复代码**: 从原来的 700+ 行减少到 400+ 行

### 2. ✅ 质量检查优化
- **快速失败策略**: 添加独立的 `quality-check` job
- **前置验证**: 在构建前运行 lint、format、test
- **并行执行**: 质量检查和构建可以并行进行

### 3. ✅ 缓存策略改进
- **pnpm 缓存**: 自动缓存 Node.js 依赖
- **分离的 Rust 缓存**: 按平台和目标分别缓存
- **更好的缓存键**: 使用更精确的缓存键策略

### 4. ✅ 并行化优化
- **Linux 构建并行**: musl 和 gnu 构建并行执行
- **矩阵策略**: 更清晰的矩阵配置
- **按需构建**: 非 tag 推送只构建 Linux

### 5. ✅ 参数化配置
```yaml
env:
  RUST_VERSION: '1.90.0'
  NODE_VERSION: '20'
  MANYLINUX_IMAGE: 'quay.io/pypa/manylinux2014_x86_64:latest'
  SCCACHE_VERSION: 'v0.7.6'
```

### 6. ✅ 工作流结构优化
- **清晰的 job 分离**: quality-check → build-linux/build-matrix → release
- **更好的依赖关系**: 明确的 needs 依赖
- **改进的错误处理**: 更好的错误信息和调试输出

## 迁移步骤

### 1. 备份现有配置
```bash
# 备份当前工作流
cp .github/workflows/build-matrix.yml .github/workflows/build-matrix.yml.backup
```

### 2. 应用新配置
```bash
# 删除旧工作流（可选，也可以先保留进行测试）
# rm .github/workflows/build-matrix.yml

# 新的优化工作流已创建为 build-optimized.yml
```

### 3. 测试验证
推送一个测试分支验证新工作流：
```bash
git add .github/
git commit -m "优化 GitHub Actions 工作流配置"
git push origin feature/optimize-actions
```

## 预期性能改进

### 构建时间
- **质量检查**: ~2-3 分钟（前置，快速失败）
- **Linux 构建**: 并行执行，预计减少 30-40% 时间
- **缓存命中**: 后续构建速度提升 50-70%

### 资源使用
- **更少的重复工作**: 代码复用减少资源浪费
- **更好的缓存**: 减少网络和计算资源使用
- **并行优化**: 更高效的资源利用

## 兼容性说明

### 保持兼容
- ✅ 相同的 artifact 名称和结构
- ✅ 相同的触发条件（push/PR/tags）
- ✅ 相同的构建目标和平台支持
- ✅ 相同的环境变量和配置

### 新增功能
- ✅ 自动 release notes 生成
- ✅ 前置质量检查
- ✅ 更好的错误处理和调试信息

## 回滚计划

如果遇到问题，可以快速回滚：

```bash
# 恢复旧配置
cp .github/workflows/build-matrix.yml.backup .github/workflows/build-matrix.yml
rm .github/workflows/build-optimized.yml

# 删除 composite actions（如果需要）
rm -rf .github/actions/

# 提交回滚
git add .github/
git commit -m "回滚到旧的 GitHub Actions 配置"
git push
```

## 监控要点

迁移后需要关注：

1. **首次构建**: 缓存为空，时间会较长
2. **质量检查**: 确保 lint/format/test 通过
3. **并行构建**: 验证 musl/gnu 构建都成功
4. **Artifacts**: 验证所有平台的构建产物正确
5. **发布流程**: 验证 tag 推送时的完整构建和发布

## 故障排查

### 常见问题

1. **Composite action 找不到**
   - 确保 `.github/actions/` 目录已提交到仓库

2. **缓存键冲突**
   - 可以手动清理 Actions 缓存页面的缓存

3. **依赖安装失败**
   - 检查 `web/pnpm-lock.yaml` 文件是否存在

4. **Rust 构建失败**
   - 检查 `backend/Cargo.lock` 文件是否存在

## 下一步优化方向

1. **远程 sccache**: 考虑配置远程缓存服务
2. **自托管 runners**: 考虑使用自托管运行器提升速度
3. **增量构建**: 进一步优化构建缓存策略
4. **安全扫描**: 添加安全漏洞扫描步骤