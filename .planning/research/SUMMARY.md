# E2E 测试断言收紧 - 研究总结

**Analysis Date:** 2026-03-14

## 关键发现

### 断言质量问题（按严重程度排序）

1. **`body` 可见性检查** — `settings.spec.ts` 有 10 处，永远通过
2. **`\d+` 正则匹配** — `search.spec.ts` 和 `search_ux.spec.ts`，匹配 "0 个结果"
3. **条件断言跳过** — `if (count > 0)` 无 else 分支，空数据时静默通过
4. **非 web-first 断言** — `expect(await x).toBeTruthy()` 无自动重试
5. **琐碎断言** — `bodyText.length > 0`，任何页面都能通过

### 测试架构建议

- **选择性 POM**：仅对复杂页面（Search、Explorer）引入 Page Object
- **共享工具**：提取 `_helpers/` 目录（archive.ts、backend.ts、network.ts、assertions.ts）
- **3 层测试策略**：Tier 1（mocked UI）、Tier 2（真实后端）、Tier 3（关键流程）

### 常见陷阱

- **假阳性**：15+ 处永远通过的断言
- **守卫子句反模式**：7 个测试用 `if` 包裹断言
- **任意超时**：20+ 处 `waitForTimeout`，值不一致（200ms-1000ms）
- **复制粘贴重复**：搜索完成等待逻辑重复 15+ 次

## 推荐断言模式

```typescript
// 之前（宽松）
expect(resultsText).toMatch(/\d+\s*个结果/);
await expect(page.locator('body')).toBeVisible();
expect(response.ok()).toBeTruthy();

// 之后（紧密）
await expect(page.getByText('5 个结果')).toBeVisible();
await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
expect(response).toBeOK();
```

## 文件优先级

| 文件 | 严重度 | 修复工作量 |
|------|--------|-----------|
| `settings.spec.ts` | 高 | 中 |
| `search.spec.ts` | 高 | 低 |
| `search_ux.spec.ts` | 高 | 低 |
| `integration_explorer.spec.ts` | 中 | 中 |
| 其他集成测试 | 低 | 低 |

---

*Research synthesis: 2026-03-14*
