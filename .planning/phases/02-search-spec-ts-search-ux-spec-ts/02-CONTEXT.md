# Phase 2: 收紧 `search.spec.ts` 和 `search_ux.spec.ts` 断言 - Context

**Gathered:** 2026-03-14
**Status:** Ready for planning

<domain>
## Phase Boundary

修复 `search.spec.ts` 和 `search_ux.spec.ts` 中的正则匹配和条件断言问题。替换 `\d+` 正则为具体数字验证，移除嵌套条件跳过，添加高亮文本和文件路径验证。仅修改这两个文件，不新增测试用例。

</domain>

<decisions>
## Implementation Decisions

### 正则替换策略
- 提取并验证具体数字：从结果文本中提取数字，用 `expect(count).toBeGreaterThanOrEqual(0)` 或 `expect(count).toBeGreaterThan(0)` 验证
- 两端都收紧：
  - `waitForFunction`：改用更具体的条件（等待 `.text-lg.font-semibold` 有具体文本内容，不只匹配格式）
  - 最终 `expect`：验证具体数字值，而不是只匹配 `\d+` 格式
- 空状态测试：精确验证 `toContainText('0 个结果')`，唯一关键词确保 0 结果

### 条件断言处理
- 分离搜索完成验证和结果元素验证：
  1. 先验证搜索完成（waitForFunction 等待结果文本出现）
  2. 提取具体数字
  3. 如果数字 > 0，验证结果元素存在
- 全部移除嵌套条件：`if (count > 0) { if (highlightCount > 0) { expect(...) } }` → 直接 `expect(highlightCount).toBeGreaterThan(0)`
- 移除 `if (buttonCount > 0)` 包装，直接 `expect(buttonCount).toBeGreaterThan(0)` 或用 `waitFor`

### 高亮验证深度
- 存在 + 精确文本匹配：验证高亮元素存在，且文本精确匹配搜索关键词（不检查大小写）
- 使用 `expect(highlightText?.toUpperCase()).toMatch(/CRITICAL|ERROR/)` 验证

### 文件路径验证
- 卡片存在 + 内容长度验证：验证结果卡片存在，且第一个卡片文本长度 > 某个阈值（有实质内容）
- `expect(cardText?.length).toBeGreaterThan(50)` 或其他合理阈值

### 搜索数据依赖
- 通用关键词 + graceful 降级：使用通用关键词（error, info, CRITICAL 等）确保大概率有结果
- 不 mock 搜索 API（保持真实集成测试性质）
- 如果返回 0 结果，相关元素验证使用 `.toBeGreaterThanOrEqual(0)` 而非 `.toBeGreaterThan(0)`
- 空状态测试用唯一关键词 `NONEXISTENT_KEYWORD_XYZ123_UNLIKELY_TO_MATCH`，可精确验证 `0 个结果`

### Claude's Discretion
- 具体的数字提取正则表达式
- `waitForFunction` 的具体等待条件
- 卡片内容长度阈值的确定
- 是否需要 `test.skip()` 或 `test.fail()` 标记当搜索环境无数据时

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- Search page heading: `.text-lg.font-semibold` — result count display
- Search input: `page.getByPlaceholder('搜索...')`
- Result cards: `page.locator('[data-result-card], .rounded.border')`
- Sidebar buttons: `page.locator('aside button')`
- Highlight elements: `page.locator('mark, .highlight, [style*="background-color"]')`
- Open in new window button: `page.getByTitle('在新窗口打开')`

### Established Patterns from Phase 1
- `waitForSelector` before strict assertions (5000ms timeout)
- Remove conditional `if (count > 0)` wrappers
- Mock response format must match backend API contracts

### Integration Points
- Search API: `POST /api/v1/logseek/search.ndjson` — returns NDJSON stream
- beforeEach navigates to `/search` and waits for networkidle
- Tests use `page.waitForFunction` to wait for search completion

</code_context>

<specifics>
## Specific Ideas

- 结果计数选择器：`.text-lg.font-semibold`
- 结果卡片选择器：`[data-result-card], .rounded.border`
- 高亮选择器：`mark, .highlight, [style*="background-color"]`
- 侧边栏选择器：`aside button`
- 空状态测试关键词：`NONEXISTENT_KEYWORD_XYZ123_UNLIKELY_TO_MATCH`
- 常用搜索关键词：`error`, `info`, `CRITICAL`, `ERROR`, `WARN`, `failed`, `exception`, `timeout`, `DEBUG`, `trace`, `FATAL`

</specifics>

<deferred>
## Deferred Ideas

- Mock 搜索 API 以获得确定性测试数据 — 属于架构变更，可能需要新的测试策略
- None — discussion stayed within phase scope

</deferred>

---

*Phase: 02-search-spec-ts-search-ux-spec-ts*
*Context gathered: 2026-03-14*
