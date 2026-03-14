# Phase 5: 添加加载状态测试 - Context

**Gathered:** 2026-03-14
**Status:** Ready for planning

<domain>
## Phase Boundary

创建 E2E 测试验证加载状态 UI：搜索加载 spinner、Explorer 加载状态、View 页面加载指示器。由于代码库中不存在骨架屏（skeleton），测试聚焦于 spinner 可见性及加载完成后的状态转换。

</domain>

<decisions>
## Implementation Decisions

### 测试范围调整
- **原计划**: 包含骨架屏到内容过渡测试
- **实际**: 代码库中无骨架屏实现，所有页面使用 spinner（`.animate-spin`）
- **调整**: 将 LOAD-02 改为测试 "spinner 到内容过渡"
- 测试 3 个场景：搜索 spinner、Explorer 加载、View 页面加载

### Loading Spinner 选择器
- 通用 spinner: `.animate-spin` CSS 类
- 搜索页: `LoaderCircle` 组件，文本 "搜索中..."（首次）或 "加载更多..."（后续）
- Explorer: `RefreshCw` 图标带 `animate-spin` 类（条件渲染）
- View 页: `LoaderCircle` + 文本 "加载中..."

### 状态转换验证
- 验证 loading 开始时 spinner 可见
- 验证 loading 结束后 spinner 消失
- 验证内容在 loading 完成后出现
- 使用 `page.waitForFunction()` 等待状态变化

### 搜索加载状态细节
- 输入框在 loading 期间 `disabled`
- 结果计数：loading 中显示 "搜索结果"，完成后显示 "X 个结果"
- Load More 按钮：loading 时显示 spinner + "加载更多..."

### Explorer 加载状态细节
- 刷新按钮的 `RefreshCw` 图标在 loading 时带 `animate-spin`
- 返回按钮在 loading 期间 `disabled`
- 空目录消息仅在 `!loading` 时显示

### Claude's Discretion
- 具体的等待超时时间
- 是否需要 mock API 延迟来确保观察到 loading 状态
- 哪些边缘情况值得测试

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `.animate-spin`: 通用 spinner CSS 类，所有 loading 状态都使用
- `LoaderCircle` (lucide-svelte): 搜索页和 View 页使用
- `RefreshCw` (lucide-svelte): Explorer 使用，条件渲染 `animate-spin`
- `SearchEmptyState.svelte`: 有 initial/error/no-results 状态

### Established Patterns
- `page.waitForFunction()` 等待异步状态变化
- `page.getByPlaceholder('搜索...')` 搜索输入框
- `.text-lg.font-semibold` 结果计数元素
- `data-result-card` 搜索结果卡片

### Integration Points
- 搜索加载: `searchStore.loading` 控制 spinner 和 input disabled
- Explorer 加载: `loading` 变量控制 RefreshCw 图标动画
- View 加载: `loading` 变量控制 LoaderCircle 显示

</code_context>

<specifics>
## Specific Ideas

- "测试应该验证用户在等待时能看到反馈，不是空白页"
- spinner 应该在搜索开始时立即出现
- 加载完成后，结果应该替换 spinner

</specifics>

<deferred>
## Deferred Ideas

- 骨架屏实现 — 当前不存在，如需添加应为独立 phase
- 乐观更新/占位符 — 超出当前 scope

</deferred>

---

*Phase: 05-loading-state-tests*
*Context gathered: 2026-03-14*
