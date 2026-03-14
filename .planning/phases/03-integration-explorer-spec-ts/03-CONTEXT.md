# Phase 3: 收紧 `integration_explorer.spec.ts` 断言 - Context

**Gathered:** 2026-03-14
**Status:** Ready for planning

<domain>
## Phase Boundary

完善 `integration_explorer.spec.ts` 中的下载测试，替换 `body` 正则检查为具体元素验证，移除条件跳过。仅修改此文件，不新增测试用例。

</domain>

<decisions>
## Implementation Decisions

### Body 替换策略 (error checks)
- 5 处 `body.toContainText(/error|错误/i)` 替换为具体错误元素验证
- 错误显示组件验证：检查具体错误消息元素（如 Alert 组件、error toast）
- 成功场景：验证具体文件列表元素存在，而不是 body 不包含错误

### 下载测试完善
- 当前测试只验证文件可见，应完善为验证下载事件
- 使用 `page.waitForEvent('download')` 等待下载
- 验证下载文件名匹配预期
- 验证下载文件大小 > 0

### 条件断言处理
- 移除所有条件包装，直接断言
- 使用 `waitForSelector` 给动态加载元素足够时间

### Claude's Discretion
- 具体错误元素选择器（取决于错误显示实现）
- 下载事件等待超时时间
- 是否需要验证响应体字段的具体值

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- Explorer page: `/explorer` route
- File list items: `page.getByText('filename')`
- Up button: `page.locator('button:has(svg.lucide-arrow-left)')`
- ORL input: `page.locator('#orl-input')`
- Context menu: right-click on file

### Integration Points
- Explorer API: `POST /api/v1/explorer/list`
- Download API: `GET /api/v1/explorer/download?orl=...`
- Agent registration via `request.get('/api/v1/agents/{id}')`

</code_context>

<specifics>
## Specific Ideas

- Error patterns currently matched: `/error|错误/i`, `/500|Internal Server Error/i`, `/404|Not Found|错误/i`, `/Access denied|Not Found|404|错误/i`
- Download test file: `test.txt` with content 'Hello Explorer!\n'
- Archive test: `test_archive.tar` with nested structure

</specifics>

<deferred>
## Deferred Ideas

- None — discussion stayed within phase scope

</deferred>

---

*Phase: 03-integration-explorer-spec-ts*
*Context gathered: 2026-03-14*
