# Phase 4: 添加错误处理测试 - Context

**Gathered:** 2026-03-14
**Status:** Ready for planning

<domain>
## Phase Boundary

创建 `error_handling.spec.ts`，包含 4 个测试用例验证错误场景的用户反馈。测试 API 500 错误提示、网络超时处理、错误显示交互、搜索取消状态清理。

</domain>

<decisions>
## Implementation Decisions

### Error Mocking Strategy
- 使用 Playwright `page.route()` 拦截 API 调用并返回错误响应
- 500 错误：`route.fulfill({ status: 500, body: JSON.stringify({detail: 'Internal Server Error'}) })`
- 超时：`route.abort('timedout')` 模拟网络超时
- 不使用真实后端错误（不可靠，CI 环境不同）

### Error Verification Depth
- 验证错误 UI 元素出现（标题、错误消息）
- 验证重试按钮存在且可点击
- 验证点击重试后搜索状态重置（不验证重试结果成功，那是集成测试范围）
- 错误详情展开/收起交互测试

### Test Structure
- 4 个独立测试，对应 ERROR-01 到 ERROR-04
- 每个测试独立 setup/teardown，互不依赖
- 共享 mock 配置提取为辅助函数

### Error Display Selectors
- 搜索错误：`h3` 包含 "搜索出错" + `p` 包含具体错误消息
- Explorer 错误：`h3` 包含 "资源列举失败" + `details` 展开显示错误详情
- Settings 错误：`[data-testid="alert"]` 带 error variant
- 重试按钮：搜索用 `button` 文本 "重新搜索"，Explorer 用 `button` 文本 "重试"

### Search Cancellation
- 验证 AbortController 取消后 loading 状态变为 false
- 验证搜索结果区域不再显示 loading spinner
- 验证可以发起新搜索（状态清理完成）

### Claude's Discretion
- 具体的 mock 错误消息内容
- 超时等待时间设置
- 重试按钮点击后的状态验证深度

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `SearchEmptyState.svelte`: 错误状态显示组件，选择器 `h3` "搜索出错"
- `Alert.svelte`: 通用警告组件，选择器 `[data-testid="alert"]`
- Explorer error display: `h3` "资源列举失败" + `details` 错误详情
- Test utilities: `web/tests/e2e/utils/agent.ts` 中的 `getFreePort()`, `stopProcess()` 等

### Established Patterns
- `test.beforeEach` 导航到页面并等待 `networkidle`
- `page.route()` 拦截 API 调用
- `test.describe.configure({ mode: 'serial' })` 用于有状态测试
- 搜索输入：`page.getByPlaceholder('搜索...')`

### Integration Points
- 搜索 API: `POST /api/v1/logseek/search.ndjson`
- Explorer API: `POST /api/v1/explorer/list`
- 搜索取消：AbortController signal 传递给 API 调用

</code_context>

<specifics>
## Specific Ideas

- "测试应该验证用户能看到有意义的错误消息，而不是空白页"
- 错误消息应该包含 HTTP 状态码或具体错误原因
- 重试按钮应该真正重置搜索状态并允许重新搜索

</specifics>

<deferred>
## Deferred Ideas

- None — discussion stayed within phase scope

</deferred>

---

*Phase: 04-error-handling-tests*
*Context gathered: 2026-03-14*
