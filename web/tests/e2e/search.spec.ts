import { test, expect } from '@playwright/test';

test.describe('Search E2E', () => {
  test.beforeEach(async ({ page }) => {
    // 拦截搜索请求，返回固定的 NDJSON 结果
    await page.route('**/search.ndjson', async (route) => {
      const jsonResults = [
        // 本地文件结果
        {
          type: 'result',
          data: {
            path: 'odfi://local/dir/var/log/syslog',
            keywords: [{ type: 'literal', text: 'error' }],
            chunks: [
              {
                range: [100, 102],
                lines: [
                  { no: 100, text: 'Dec 11 10:00:00 localhost kernel: [123.456] error: something bad happened' },
                  { no: 101, text: 'Dec 11 10:00:01 localhost kernel: [123.457] info: recovering' },
                  { no: 102, text: 'Dec 11 10:00:02 localhost kernel: [123.458] error: failed again' }
                ]
              }
            ]
          }
        },
        // 远程代理结果
        {
          type: 'result',
          data: {
            path: 'odfi://web-01@agent/dir/app/logs/error.log',
            keywords: [{ type: 'literal', text: 'error' }],
            chunks: [
              {
                range: [50, 51],
                lines: [
                  { no: 50, text: '2023-10-27 10:00:00 [ERROR] Connection refuesd' },
                  { no: 51, text: '2023-10-27 10:00:01 [INFO] Retrying...' }
                ]
              }
            ]
          }
        },
        // S3 归档结果
        {
          type: 'result',
          data: {
            path: 'odfi://prod:logs-bucket@s3/archive/2023/10/data.tar.gz?entry=internal/service.log',
            keywords: [{ type: 'literal', text: 'error' }],
            chunks: [
              {
                range: [1, 1],
                lines: [{ no: 1, text: 'Starting service...' }]
              }
            ]
          }
        },
        // NDJSON 结束标记（让前端知道结果流结束了）
        {
          type: 'complete',
          data: {
            source: 'mock',
            elapsed_ms: 100
          }
        }
      ];

      const ndjson = jsonResults.map((r) => JSON.stringify(r)).join('\n');

      await route.fulfill({
        status: 200,
        headers: {
          'Content-Type': 'application/x-ndjson',
          'X-Logseek-SID': 'test-session-id'
        },
        body: ndjson
      });
    });

    await page.goto('/search');
  });

  test('should display search results for different endpoint types', async ({ page }) => {
    // 发起搜索
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('error');
    await searchInput.press('Enter');

    // 等待结果统计渲染出来
    await expect(page.locator('.text-lg.font-semibold')).toContainText('3 个结果');

    // 侧边栏第 1 层：数据源类型
    await expect(page.getByRole('button', { name: '本地文件' })).toBeVisible();
    await expect(page.getByRole('button', { name: '远程代理' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'S3 云存储' })).toBeVisible();

    // 展开后应看到叶子目录（页面会跳过只有一个子节点的中间目录）

    // 展开本地文件
    await page.getByRole('button', { name: '本地文件' }).click();
    await expect(page.getByRole('button', { name: 'log' })).toBeVisible();

    // 展开远程代理
    await page.getByRole('button', { name: '远程代理' }).click();
    await expect(page.getByRole('button', { name: 'logs' })).toBeVisible();

    // 展开 S3
    await page.getByRole('button', { name: 'S3 云存储' }).click();
    await expect(page.getByRole('button', { name: 'internal' })).toBeVisible();
    // 路径在卡片里可能会被截断，这里只断言关键节点出现

    // 点击“本地文件”过滤，只剩本地那条结果
    await page.getByRole('button', { name: '本地文件' }).click();
    await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果');

    // 卡片内容
    await expect(page.getByText('syslog')).toBeVisible();
    await expect(
      page.getByText('Dec 11 10:00:00 localhost kernel: [123.456] error: something bad happened')
    ).toBeVisible();

    // 再点一次取消过滤，回到全部结果
    await page.getByRole('button', { name: '本地文件' }).click();

    await expect(page.locator('.text-lg.font-semibold')).toContainText('3 个结果');
  });
});
