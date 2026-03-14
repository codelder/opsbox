# Phase 1: 收紧 `settings.spec.ts` 断言 - Context

**Gathered:** 2026-03-14
**Status:** Ready for planning

<domain>
## Phase Boundary

替换 `settings.spec.ts` 中所有 `body` 可见性检查为具体 UI 元素验证。收紧 mock 数据断言、主题切换断言和条件断言。仅修改此一个文件，不新增测试用例。

</domain>

<decisions>
## Implementation Decisions

### Body 替换策略
- 按 section 定位具体元素：每个 describe block 验证其对应的具体 UI 元素
- Page Layout: 验证 `heading('系统设置')` + tabs 存在
- Planner Management: 验证 planner section 的具体 heading/form elements
- LLM Management: 验证 LLM section 的 Card、heading、form elements
- S3 Profile: 验证 Profile section 的 Card、heading、form elements
- Agent Management: 验证 Agent section 的具体元素
- Server Log Settings: 验证 Log section 的具体元素（如 log level label）
- Error Handling: 验证页面结构仍然存在（heading + tabs）

### Mock 数据验证深度
- 验证 mock 数据名称渲染：LLM 显示 'ollama-local'，S3 显示 'minio-local'
- 验证 mock 数据条数：列表项数量匹配 mock 返回的数据条数
- 结构始终验证：即使在 error handling 场景，也验证基本 UI 结构（cards, headings, forms）

### 主题切换断言
- 检查 html class 精确值变化：如默认状态 → toggle → 'dark' → toggle → 回到原始状态
- 验证 CSS 变量值改变：检查 `--background` 或等效 CSS variable 的值在 toggle 后变化
- 双向切换验证：toggle 两次验证回到原始状态

### 条件断言处理
- 全部严格失败：移除所有 `if (count > 0)` 条件断言包装
- 移除 + 等待元素：移除条件包装的同时添加 `waitForSelector` 给 UI 足够加载时间
- 涉及测试：Settings navigation 测试（settings button）、Theme toggle 测试（theme button）

### Claude's Discretion
- 具体 CSS variable 名称选择（检查哪个 CSS variable 最合适）
- waitForSelector 的超时时间设定
- 错误处理测试中 "页面仍然显示" 的具体断言元素选择

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- Settings page structure: `+page.svelte` has 5 tabs (对象存储, Agent, 规划脚本, 大模型, Server 日志) with corresponding components
- Tab components: ProfileManagement, AgentManagement, PlannerManagement, LlmManagement, ServerLogSettings
- ThemeToggle component: accessible via `page.getByRole('button', { name: /theme|主题|toggle/i })`
- Heading: `page.getByRole('heading', { name: '系统设置' })`

### Established Patterns
- Playwright E2E tests use `@playwright/test` with `test.describe` blocks
- API mocking via `page.route()` with `route.fulfill()`
- Tabs use `TabsTrigger` from `$lib/components/ui/tabs`
- Settings components use Card, Button, Input, Label from `$lib/components/ui/`

### Integration Points
- beforeEach navigates to `/settings` and verifies heading
- LLM mock intercepts `**/settings/llm/backends**`
- S3 mock intercepts `**/profiles**`
- Error handling mock intercepts `**/log/config` returning 500

</code_context>

<specifics>
## Specific Ideas

- Tab triggers have text: '对象存储', 'Agent', '规划脚本', '大模型', 'Server 日志'
- LLM mock data: `{ name: 'ollama-local', provider: 'ollama', base_url: 'http://127.0.0.1:11434', model: 'qwen3:8b', timeout_secs: 60 }`
- S3 mock data: `{ profile_name: 'minio-local', endpoint: 'http://127.0.0.1:9000', access_key: 'minioadmin' }`
- Theme toggle button pattern: `/theme|主题|toggle/i`
- Settings navigation button pattern: `/打开设置|settings/i`

</specifics>

<deferred>
## Deferred Ideas

- None — discussion stayed within phase scope

</deferred>

---

*Phase: 01-settings-spec-ts*
*Context gathered: 2026-03-14*
