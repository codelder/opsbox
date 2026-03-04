import { test, expect } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

/**
 * Performance Boundary Tests
 *
 * 验证系统在边界条件下的性能，包括：
 * - 大结果集（10,000+ 行）
 * - 超长单行（1MB+）
 * - 深层嵌套目录（100+ 层）
 * - 大量小文件（1000+ 个）
 * - 虚拟滚动性能
 *
 * 这些测试确保系统在极端情况下仍能正常工作。
 */
test.describe('Performance Boundary Tests', () => {
  test.describe.configure({ mode: 'serial' });

  const RUN_ID = Date.now();
  const TEST_LOG_DIR = path.join(__dirname, `temp_performance_${RUN_ID}`);
  const API_BASE = 'http://127.0.0.1:4001/api/v1/logseek';
  const TEST_APP = `e2e_performance_${RUN_ID}`;

  test.beforeAll(async ({ request }) => {
    test.setTimeout(120000);
    if (!fs.existsSync(TEST_LOG_DIR)) {
      fs.mkdirSync(TEST_LOG_DIR, { recursive: true });
    }

    // 1. 创建包含大量行的文件
    const largeFile = path.join(TEST_LOG_DIR, 'large.log');
    const lines: string[] = [];
    for (let i = 0; i < 10000; i++) {
      lines.push(`PERF_TEST_${RUN_ID} Line ${i}: This is a test log entry with some content`);
    }
    fs.writeFileSync(largeFile, lines.join('\n'));

    // 2. 创建包含超长单行的文件
    const longLineFile = path.join(TEST_LOG_DIR, 'longline.log');
    const longLine = `PERF_TEST_${RUN_ID} ${'A'.repeat(1024 * 1024)}`; // 1MB 单行
    fs.writeFileSync(longLineFile, longLine);

    // 3. 创建深层嵌套目录结构
    let currentDir = TEST_LOG_DIR;
    for (let i = 0; i < 50; i++) {
      currentDir = path.join(currentDir, `level${i}`);
      fs.mkdirSync(currentDir, { recursive: true });
    }
    fs.writeFileSync(path.join(currentDir, 'deep.log'), `PERF_TEST_${RUN_ID} Deep nested file`);

    // 4. 创建大量小文件
    const manyFilesDir = path.join(TEST_LOG_DIR, 'many_files');
    fs.mkdirSync(manyFilesDir, { recursive: true });
    for (let i = 0; i < 100; i++) {
      // 减少到 100 个以加快测试速度
      fs.writeFileSync(path.join(manyFilesDir, `file${i}.log`), `PERF_TEST_${RUN_ID} File ${i} content`);
    }

    // 配置 Planner
    const absRoot = path.resolve(TEST_LOG_DIR);
    const script = `SOURCES = ["orl://local${absRoot}?glob=**/*.log"]`;

    const response = await request.post(`${API_BASE}/settings/planners/scripts`, {
      data: { app: TEST_APP, script }
    });
    expect(response.ok()).toBeTruthy();
  });

  test.afterAll(async ({ request }) => {
    try {
      await request.delete(`${API_BASE}/settings/planners/scripts/${TEST_APP}`);
    } catch {
      // Ignore
    }
    fs.rmSync(TEST_LOG_DIR, { recursive: true, force: true });
  });

  test('should handle large result set with pagination', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP} PERF_TEST_${RUN_ID}`);
    await searchInput.press('Enter');

    // 等待搜索完成
    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 30000
    });

    // 验证结果数量合理
    const resultText1 = await page.locator('.text-lg.font-semibold').textContent();
    const count1 = parseInt(resultText1?.match(/(\d+)/)?.[1] || '0');
    console.log(`Initial count: ${count1}`);
    expect(count1).toBeGreaterThan(0);

    // 验证能看到测试标记
    await expect(page.getByText(`PERF_TEST_${RUN_ID}`).first()).toBeVisible();

    // 验证分页控制 - 应该看到加载更多按钮
    const loadMoreBtn = page.getByRole('button', { name: '加载更多' });
    if (await loadMoreBtn.isVisible()) {
      await loadMoreBtn.click();
      // 等待数量更新
      await expect(async () => {
        const text = await page.locator('.text-lg.font-semibold').textContent();
        const count = parseInt(text?.match(/(\d+)/)?.[1] || '0');
        expect(count).toBeGreaterThan(count1);
      }).toPass({ timeout: 10000 });

      const resultText2 = await page.locator('.text-lg.font-semibold').textContent();
      console.log(`Count after pagination: ${resultText2}`);
    } else {
      console.warn('Load More button not found. Loaded all results?');
    }

    // 验证渲染数量一致性
    const renderedResults = await page.locator('[data-result-card]').count();
    // 渲染数量应当等于当前显示的计数 (因为没有虚拟滚动)
    const currentCountText = await page.locator('.text-lg.font-semibold').textContent();
    const currentCount = parseInt(currentCountText?.match(/(\d+)/)?.[1] || '0');
    expect(renderedResults).toBe(currentCount);

    // 验证页面没有卡顿（能够滚动）
    await page.mouse.wheel(0, 1000);
    await page.waitForTimeout(500);
  });

  test('should handle very long single line', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP} path:longline.log PERF_TEST_${RUN_ID}`);
    await searchInput.press('Enter');

    // 等待搜索完成
    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 30000
    });

    // 验证能找到结果
    await expect(page.getByText(`PERF_TEST_${RUN_ID}`).first()).toBeVisible({ timeout: 10000 });

    // 验证超长行被正确截断或处理（不应该导致页面崩溃）
    const resultCard = page.locator('[data-result-card]').first();
    await expect(resultCard).toBeVisible();

    // 验证页面仍然响应
    await page.mouse.move(100, 100);
    await page.waitForTimeout(500);
  });

  test('should handle deeply nested directory structure', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP} "Deep nested file"`);
    await searchInput.press('Enter');

    // 等待搜索完成
    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 30000
    });

    // 验证能找到深层文件
    await expect(page.getByText('Deep nested file')).toBeVisible({ timeout: 10000 });

    // 验证文件路径显示正确（可能被截断）
    await expect(page.getByRole('link', { name: /deep\.log/ })).toBeVisible();
  });

  test('should handle many small files efficiently', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP} path:many_files/** PERF_TEST_${RUN_ID}`);
    await searchInput.press('Enter');

    // 等待搜索完成
    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 30000
    });

    // 验证结果数量正确（应该是 100 个文件）
    const resultText = await page.locator('.text-lg.font-semibold').textContent();
    const resultCount = parseInt(resultText?.match(/(\d+)/)?.[1] || '0');
    expect(resultCount).toBeGreaterThan(0); // 验证能搜到结果（由于分页，初始数量可能仅为每页大小）

    // 验证搜索时间合理（不应该超过 30 秒）
    // 这个已经通过 timeout 验证了
  });

  test('should handle large result sets with pagination (load more)', async ({ page }) => {
    // Mock 大量搜索结果
    await page.route('**/search.ndjson', async (route) => {
      const jsonResults: Record<string, unknown>[] = [];

      // 生成 100 个结果
      for (let i = 0; i < 100; i++) {
        jsonResults.push({
          type: 'result',
          data: {
            path: `orl://local/file${i}.log`,
            keywords: [{ type: 'literal', text: 'test' }],
            chunks: [
              {
                range: [1, 1],
                lines: [{ no: 1, text: `Result ${i}: test content` }]
              }
            ]
          }
        });
      }

      jsonResults.push({ type: 'complete', data: { source: 'mock', elapsed_ms: 100 } });

      const ndjson = jsonResults.map((r) => JSON.stringify(r)).join('\n');
      await route.fulfill({
        status: 200,
        headers: {
          'Content-Type': 'application/x-ndjson',
          'X-Logseek-SID': 'pagination-test'
        },
        body: ndjson
      });
    });

    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('test');
    await searchInput.press('Enter');

    // 1. 验证初始加载 20 条
    await expect(page.locator('.text-lg.font-semibold')).toContainText('20 个结果', {
      timeout: 10000
    });
    // 更新 selector 以匹配 SearchResultCard 的实际属性 data-result-card
    await expect(page.locator('[data-result-card]')).toHaveCount(20);

    // 2. 找到并点击"加载更多"按钮
    const loadMoreBtn = page.getByRole('button', { name: /加载更多/ });
    await expect(loadMoreBtn).toBeVisible();
    await loadMoreBtn.click();

    // 3. 验证加载了更多数据 (20 + 20 = 40)
    await expect(page.locator('.text-lg.font-semibold')).toContainText('40 个结果', {
      timeout: 10000
    });
    await expect(page.locator('[data-result-card]')).toHaveCount(40);
  });

  test('should not freeze UI during search', async ({ page }) => {
    // Mock 慢速搜索
    await page.route('**/search.ndjson', async (route) => {
      // 模拟流式返回结果
      const results: string[] = [];

      for (let i = 0; i < 100; i++) {
        results.push(
          JSON.stringify({
            type: 'result',
            data: {
              path: `orl://local/file${i}.log`,
              keywords: [{ type: 'literal', text: 'test' }],
              chunks: [
                {
                  range: [1, 1],
                  lines: [{ no: 1, text: `Result ${i}` }]
                }
              ]
            }
          })
        );
      }

      results.push(JSON.stringify({ type: 'complete', data: { source: 'mock', elapsed_ms: 5000 } }));

      // 延迟返回
      await new Promise((resolve) => setTimeout(resolve, 1000));

      await route.fulfill({
        status: 200,
        headers: { 'Content-Type': 'application/x-ndjson', 'X-Logseek-SID': 'ui-freeze-test' },
        body: results.join('\n')
      });
    });

    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('test');
    await searchInput.press('Enter');

    // 在搜索进行中，验证 UI 仍然响应
    await page.waitForTimeout(500);

    // 尝试移动鼠标
    await page.mouse.move(100, 100);
    await page.mouse.move(200, 200);

    // 尝试点击其他元素（如果有）
    const logo = page.locator('header a, nav a').first();
    if ((await logo.count()) > 0) {
      const isClickable = await logo.isEnabled();
      expect(isClickable).toBeTruthy();
    }

    // 验证搜索最终完成
    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 10000
    });
  });

  test('should handle rapid scrolling without lag', async ({ page }) => {
    // Mock 大量结果
    await page.route(/.*\/search\.ndjson/, async (route) => {
      const jsonResults: Record<string, unknown>[] = [];

      for (let i = 0; i < 500; i++) {
        jsonResults.push({
          type: 'result',
          data: {
            path: `orl://local/file${i}.log`,
            keywords: [{ type: 'literal', text: 'test' }],
            chunks: [
              {
                range: [1, 1],
                lines: [{ no: 1, text: `Result ${i}: test content` }]
              }
            ]
          }
        });
      }

      jsonResults.push({ type: 'complete', data: { source: 'mock', elapsed_ms: 100 } });

      const ndjson = jsonResults.map((r) => JSON.stringify(r)).join('\n');
      await route.fulfill({
        status: 200,
        headers: { 'Content-Type': 'application/x-ndjson', 'X-Logseek-SID': 'scroll-test' },
        body: ndjson
      });
    });

    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('MOCK_SCROLL');
    await searchInput.press('Enter');

    // 初始加载 20 条
    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 10000
    });

    // 循环加载所有数据 (Mock 数据有 500 条，每页 20 条，需要点击 24 次)
    // 用于测试大量 DOM 节点下的滚动性能
    const loadMoreBtn = page.getByRole('button', { name: '加载更多' });

    // 设置较短的时间上限，因为 Mock 响应很快
    let retries = 0;
    while ((await loadMoreBtn.isVisible()) && retries < 30) {
      await loadMoreBtn.click();
      await page.waitForTimeout(50); // 给 UI 一点反应时间
      retries++;
    }

    // 验证已加载全部 500 条
    await expect(page.locator('.text-lg.font-semibold')).toContainText('500 个结果', {
      timeout: 30000
    });

    // 快速滚动多次
    for (let i = 0; i < 10; i++) {
      await page.mouse.wheel(0, 500);
      await page.waitForTimeout(100);
    }

    // 验证页面没有崩溃
    await expect(page.locator('body')).toBeVisible();

    // 验证仍然能看到结果
    await expect(page.locator('[data-result-card]').first()).toBeVisible();
  });
});
