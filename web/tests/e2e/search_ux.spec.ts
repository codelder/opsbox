import { test, expect } from '@playwright/test';

test.describe('Search UX E2E', () => {
  test.beforeEach(async ({ page }) => {
    // 拦截搜索请求
    await page.route('**/search.ndjson', async (route) => {
      const jsonResults = [
        {
          type: 'result',
          data: {
            path: 'orl://local/var/log/app.log',
            keywords: [{ type: 'literal', text: 'CRITICAL' }],
            encoding: 'UTF-8',
            chunks: [
              {
                range: [10, 10],
                lines: [{ no: 10, text: '2023-12-11 10:00:00 [CRITICAL] Database connection lost' }]
              }
            ]
          }
        },
        {
          type: 'complete',
          data: { source: 'mock', elapsed_ms: 50 }
        }
      ];

      const ndjson = jsonResults.map((r) => JSON.stringify(r)).join('\n');
      await route.fulfill({
        status: 200,
        headers: { 'Content-Type': 'application/x-ndjson', 'X-Logseek-SID': 'ux-test' },
        body: ndjson
      });
    });

    await page.goto('/search');
  });

  test('should highlight keywords and filter by sidebar', async ({ page }) => {
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('CRITICAL');
    await searchInput.press('Enter');

    // 1. 验证高亮样式 (使用更通用的选择器，因为可能是 .highlight 或内联样式)
    await expect(page.locator('mark, .highlight, [style*="background-color"]').first()).toContainText('CRITICAL');

    // 2. 验证侧边栏
    const localBtn = page.getByRole('button', { name: '本地文件' });
    await expect(localBtn).toBeVisible();

    // 3. 点击筛选
    await localBtn.click();
    await expect(page.locator('h2')).toContainText('1 个结果');
  });

  test('should navigate to viewer from result line', async ({ page }) => {
    await page.getByPlaceholder('搜索...').fill('CRITICAL');
    await page.keyboard.press('Enter');

    // 等待结果渲染
    await expect(page.getByText('Database connection lost')).toBeVisible();

    // 点击右上角的“在新窗口打开”按钮
    const [newPage] = await Promise.all([
      page.context().waitForEvent('page'),
      page.getByTitle('在新窗口打开').first().click()
    ]);

    await newPage.waitForLoadState();

    // 验证跳转 URL 包含 sid, file
    // 注意：URL 里的 orl:// 会被编码
    const url = newPage.url();
    expect(url).toContain('file=orl');
    expect(url).toContain('sid=ux-test');
  });
});
