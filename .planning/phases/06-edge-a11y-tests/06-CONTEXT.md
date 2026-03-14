# Phase 6: 添加边界情况和无障碍测试 - Context

**Gathered:** 2026-03-14
**Status:** Ready for planning

<domain>
## Phase Boundary

创建 `edge_cases.spec.ts` 和 `accessibility.spec.ts`，覆盖 7 个测试用例：空搜索结果、超长查询、XSS 防护、空目录浏览、键盘导航、ARIA 属性、焦点管理。

</domain>

<decisions>
## Implementation Decisions

### 边界情况测试 (EDGE-01 至 EDGE-04)
- **EDGE-01 空搜索结果**: mock API 返回空结果，验证 `h3` "您的搜索没有匹配到任何日志"
- **EDGE-02 超长查询**: 输入 10000+ 字符查询，验证不会崩溃，页面正常响应
- **EDGE-03 XSS 防护**: 搜索 `<script>alert('XSS')</script>`，验证被转义为 `&lt;script&gt;`
- **EDGE-04 空目录**: 创建空目录，导航进入，验证 "This directory is empty." 显示

### 无障碍测试 (A11Y-01 至 A11Y-03)
- **A11Y-01 键盘导航**: Tab 键遍历搜索页元素，Enter 触发搜索，验证焦点顺序合理
- **A11Y-02 ARIA 属性**: 验证关键元素的 aria-label 存在（清除按钮、侧边栏调整等）
- **A11Y-03 焦点管理**: 搜索后焦点应保持在输入框，错误时焦点应移到重试按钮

### 文件组织
- 创建两个文件：`edge_cases.spec.ts` 和 `accessibility.spec.ts`
- 边界情况 4 个测试，无障碍 3 个测试
- 使用 `test.describe` 分组

### XSS 测试策略
- `escapeHtml()` 已在 highlight.ts 中实现
- 测试搜索包含 `<script>`、`<img onerror>` 的查询
- 验证结果卡片中显示的是转义后的文本

### 键盘导航测试
- 使用 `page.keyboard.press('Tab')` 遍历元素
- 验证 `page.locator(':focus')` 获取当前焦点元素
- 测试搜索输入框 → 搜索按钮 → 结果卡片的 Tab 顺序

### Claude's Discretion
- 超长查询的具体长度（10000 vs 100000）
- ARIA 验证的深度（只检查存在性 vs 完整性）
- 焦点管理的边缘情况

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `SearchEmptyState.svelte`: no-results 状态，`h3` "您的搜索没有匹配到任何日志"
- `escapeHtml()` in `highlight.test.ts`: XSS 转义函数已有测试
- Explorer empty dir: `text-muted-foreground` + "This directory is empty."
- `aria-label="清除搜索内容"`: 搜索清除按钮
- `aria-label="调整侧边栏宽度"`: 侧边栏调整手柄

### Established Patterns
- `page.keyboard.press()` 键盘交互
- `page.locator(':focus')` 获取焦点元素
- `page.route()` mock API 返回空结果
- `data-testid="explorer-container"` Explorer 容器

### Integration Points
- 搜索结果渲染: `SearchResultCard.svelte` 使用 `highlight.ts` 转义
- Explorer 目录加载: `loadResources()` 设置 `loading` 和 `error`
- Tab 导航: 浏览器原生 + `tabindex="0"` 元素

</code_context>

<specifics>
## Specific Ideas

- "XSS 测试应该覆盖常见的攻击向量：script 标签、img onerror、javascript: 协议"
- 键盘导航测试应该验证用户可以完全不使用鼠标完成搜索流程
- 空结果页面应该有有用的提示信息帮助用户改进搜索

</specifics>

<deferred>
## Deferred Ideas

- 更全面的 WCAG 合规性测试 — 需要 axe-core 等工具
- 屏幕阅读器测试 — 需要额外的测试基础设施

</deferred>

---

*Phase: 06-edge-a11y-tests*
*Context gathered: 2026-03-14*
