# E2E 测试断言收紧 - 需求

## v1 Requirements

### Phase 1: 断言收紧

- [ ] **ASSERT-01**: 收紧 `settings.spec.ts` — 替换 10 处 `body` 可见性检查为具体 UI 元素
- [ ] **ASSERT-02**: 收紧 `search.spec.ts` — 修复 `\d+` 正则，验证具体结果数或空状态
- [ ] **ASSERT-03**: 收紧 `search_ux.spec.ts` — 移除嵌套条件，检查高亮文本和文件路径
- [ ] **ASSERT-04**: 收紧 `integration_explorer.spec.ts` — 完善下载测试，验证响应体字段

### Phase 2: 新测试 - 错误处理

- [ ] **ERROR-01**: API 500 错误提示显示
- [ ] **ERROR-02**: 网络超时处理和提示
- [ ] **ERROR-03**: 错误 toast 显示和关闭
- [ ] **ERROR-04**: 搜索取消后状态清理

### Phase 3: 新测试 - 加载状态

- [ ] **LOAD-01**: 搜索加载 spinner 验证
- [ ] **LOAD-02**: 骨架屏到内容过渡
- [ ] **LOAD-03**: Explorer 目录加载状态

### Phase 4: 新测试 - 边界情况

- [ ] **EDGE-01**: 空搜索结果状态
- [ ] **EDGE-02**: 超长搜索查询处理
- [ ] **EDGE-03**: 特殊字符搜索（XSS 防护）
- [ ] **EDGE-04**: 空目录浏览

### Phase 5: 新测试 - 无障碍

- [ ] **A11Y-01**: 键盘导航流程（Tab/Enter）
- [ ] **A11Y-02**: ARIA 属性完整性
- [ ] **A11Y-03**: 焦点管理验证

## Out of Scope

- 后端 Rust 单元测试
- 性能基准测试（已有 `integration_performance.spec.ts`）
- 视觉回归测试（需额外工具）

---
*Created: 2026-03-14*
