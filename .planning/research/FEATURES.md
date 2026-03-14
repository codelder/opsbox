# E2E 测试模式研究

**Analysis Date:** 2026-03-14

## 错误处理测试模式

### API 错误响应测试

```typescript
// 测试 500 错误处理
test('should show error message on 500', async ({ page }) => {
  await page.route('**/api/v1/logseek/search*', route =>
    route.fulfill({ status: 500, body: JSON.stringify({ error: 'Internal Server Error' }) })
  );
  await page.goto('/search');
  await page.fill('[data-testid="search-input"]', 'test');
  await page.click('[data-testid="search-button"]');

  // 验证错误消息显示
  await expect(page.getByText('搜索失败')).toBeVisible();
  await expect(page.getByText(/Internal Server Error/i)).toBeVisible();
});
```

### 网络超时测试

```typescript
test('should handle network timeout', async ({ page }) => {
  await page.route('**/api/**', route =>
    new Promise(() => {}) // 永不响应
  );
  await page.goto('/search');

  // 验证超时提示
  await expect(page.getByText(/请求超时/i)).toBeVisible({ timeout: 15000 });
});
```

### 错误 Toast 验证

```typescript
test('should dismiss error toast', async ({ page }) => {
  // 触发错误
  await expect(page.getByRole('alert')).toBeVisible();
  await page.getByRole('button', { name: /关闭/i }).click();
  await expect(page.getByRole('alert')).not.toBeVisible();
});
```

## 加载状态测试模式

### Spinner/Skeleton 验证

```typescript
test('should show loading state', async ({ page }) => {
  // 延迟响应以捕获加载状态
  await page.route('**/api/**', async route => {
    await new Promise(r => setTimeout(r, 1000));
    await route.continue();
  });

  const searchPromise = page.goto('/search');

  // 验证加载指示器出现
  await expect(page.getByTestId('loading-spinner')).toBeVisible();

  await searchPromise;

  // 验证加载完成
  await expect(page.getByTestId('loading-spinner')).not.toBeVisible();
});
```

### 内容加载过渡

```typescript
test('should transition from loading to content', async ({ page }) => {
  await page.goto('/search?q=test');

  // 初始状态：骨架屏
  await expect(page.getByTestId('skeleton-card')).toBeVisible();

  // 最终状态：实际内容
  await expect(page.getByTestId('result-card').first()).toBeVisible();
  await expect(page.getByTestId('skeleton-card')).not.toBeVisible();
});
```

## 边界情况测试模式

### 空状态验证

```typescript
test('should show empty state for no results', async ({ page }) => {
  await page.goto('/search?q=xyznonexistent123');

  // 明确验证 0 结果
  await expect(page.getByText('0 个结果')).toBeVisible();
  await expect(page.getByText(/暂无匹配/i)).toBeVisible();
});
```

### 超长输入处理

```typescript
test('should handle very long search query', async ({ page }) => {
  const longQuery = 'a'.repeat(10000);
  await page.goto('/search');
  await page.fill('[data-testid="search-input"]', longQuery);

  // 验证输入被截断或提示
  const inputValue = await page.inputValue('[data-testid="search-input"]');
  expect(inputValue.length).toBeLessThanOrEqual(1000);
});
```

### 特殊字符搜索

```typescript
test('should handle special characters in query', async ({ page }) => {
  const specialChars = ['<script>', '"; DROP TABLE', '%s%n', '中文', '🎉'];

  for (const chars of specialChars) {
    await page.goto(`/search?q=${encodeURIComponent(chars)}`);
    // 验证页面正常渲染，无 XSS
    await expect(page.getByTestId('search-page')).toBeVisible();
    // 验证无脚本执行
    const alerts = page.on('dialog', () => fail('Unexpected dialog'));
  }
});
```

## 无障碍测试模式

### 键盘导航

```typescript
test('should support keyboard navigation', async ({ page }) => {
  await page.goto('/search');

  // Tab 键导航
  await page.keyboard.press('Tab');
  await expect(page.getByTestId('search-input')).toBeFocused();

  await page.keyboard.press('Tab');
  await expect(page.getByTestId('search-button')).toBeFocused();

  // Enter 键提交
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('results-container')).toBeVisible();
});
```

### ARIA 属性验证

```typescript
test('should have proper ARIA attributes', async ({ page }) => {
  await page.goto('/search');

  // 搜索输入框
  const searchInput = page.getByTestId('search-input');
  await expect(searchInput).toHaveAttribute('aria-label', /搜索/i);

  // 结果区域
  const results = page.getByTestId('results-container');
  await expect(results).toHaveAttribute('role', 'region');
  await expect(results).toHaveAttribute('aria-live', 'polite');
});
```

### 焦点管理

```typescript
test('should manage focus on route change', async ({ page }) => {
  await page.goto('/');
  await page.click('a[href="/search"]');

  // 导航后焦点应移到主要内容
  await expect(page.getByTestId('search-input')).toBeFocused();
});
```

## 测试数据策略

### Mock API 响应

```typescript
// 使用可控的 mock 数据
const MOCK_SEARCH_RESULTS = {
  results: [
    { file: 'test.log', line: 1, content: 'ERROR: Test error', match: 'ERROR' },
    { file: 'test.log', line: 2, content: 'INFO: Test info', match: 'INFO' }
  ],
  total: 2
};

test.beforeEach(async ({ page }) => {
  await page.route('**/api/v1/logseek/search*', route =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(MOCK_SEARCH_RESULTS)
    })
  );
});
```

## 新测试用例清单

### 错误处理（8 个）
- [ ] API 500 错误提示
- [ ] API 404 资源不存在
- [ ] 网络超时处理
- [ ] 无效输入验证
- [ ] 错误 toast 显示和关闭
- [ ] 搜索取消后状态清理
- [ ] 并发请求冲突处理
- [ ] 认证失败（如适用）

### 加载状态（4 个）
- [ ] 搜索加载 spinner
- [ ] 设置页面数据加载
- [ ] Explorer 目录加载
- [ ] 骨架屏到内容过渡

### 边界情况（6 个）
- [ ] 空搜索结果
- [ ] 超长搜索查询
- [ ] 特殊字符搜索
- [ ] 空目录浏览
- [ ] 大文件查看
- [ ] 并发搜索

### 无障碍（4 个）
- [ ] 键盘导航流程
- [ ] ARIA 属性完整性
- [ ] 焦点管理
- [ ] 屏幕阅读器兼容（基础检查）

---

*Features research: 2026-03-14*
