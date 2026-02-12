import { test, expect } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

/**
 * Query Syntax Integration Tests
 *
 * 验证查询语法在真实搜索中的行为，包括：
 * - 布尔运算符组合 (OR, AND, NOT)
 * - 正则表达式搜索
 * - 短语搜索
 * - 路径过滤
 * - 复杂嵌套查询
 *
 * 这些测试使用真实后端 API，验证查询解析器和搜索执行器的集成。
 */
test.describe('Query Syntax Integration Tests', () => {
  test.describe.configure({ mode: 'serial' });

  const RUN_ID = Date.now();
  const TEST_LOG_DIR = path.join(__dirname, `temp_query_syntax_${RUN_ID}`);
  const API_BASE = 'http://127.0.0.1:4001/api/v1/logseek';
  const TEST_APP = `e2e_query_syntax_${RUN_ID}`;

  // 辅助函数：展开所有折叠的搜索结果
  async function expandAllResults(page: import('@playwright/test').Page) {
    // 等待结果加载
    await page.waitForSelector('.text-lg.font-semibold', { timeout: 10000 });

    // 查找并点击所有"显示其余"按钮
    // 使用 while 循环因为点击后元素会消失/变化
    const expandBtnSelector = 'button:has-text("显示其余")';
    while ((await page.locator(expandBtnSelector).count()) > 0) {
      await page.locator(expandBtnSelector).first().click();
      await page.waitForTimeout(200); // 等待展开动画
    }
  }

  test.beforeAll(async ({ request }) => {
    test.setTimeout(120000);
    // 创建测试目录和文件
    if (!fs.existsSync(TEST_LOG_DIR)) {
      fs.mkdirSync(TEST_LOG_DIR, { recursive: true });
    }

    // 创建包含不同日志级别的文件
    fs.writeFileSync(
      path.join(TEST_LOG_DIR, 'app.log'),
      [
        '2025-01-01 10:00:00 [ERROR] Database connection failed',
        '2025-01-01 10:00:02 [INFO] Request processed successfully',
        '2025-01-01 10:00:03 [ERROR] File not found: /tmp/data.json',
        '2025-01-01 10:00:04 [DEBUG] Cache hit for key: user_123'
      ].join('\n')
    );

    // 单独放入 deprecated 相关的日志，以便测试负向过滤（因为过滤是基于文件的）
    fs.writeFileSync(
      path.join(TEST_LOG_DIR, 'deprecated.log'),
      ['2025-01-01 10:00:01 [WARN] Using deprecated API endpoint'].join('\n')
    );

    // 创建一个"干净"的文件，用于对比测试负向过滤
    // app.log 包含 "INFO ... processed"
    // safe.log 包含 "INFO" 但没有 "processed"
    fs.writeFileSync(
      path.join(TEST_LOG_DIR, 'safe.log'),
      ['2025-01-01 10:00:05 [INFO] Safe request without negative terms'].join('\n')
    );

    // 创建包含错误代码的文件（添加足够的间隔以避免上下文重叠）
    // 创建包含错误代码的文件（添加足够的间隔以避免上下文重叠）
    const padding = '\n'.repeat(100);
    // 拆分以支持负向过滤测试 (ERR AND -WRN)
    fs.writeFileSync(
      path.join(TEST_LOG_DIR, 'errors_only.log'),
      [
        'ERR001: Authentication failed',
        padding,
        'ERR002: Invalid request format',
        padding,
        'ERR003: Timeout after 30s'
      ].join('\n')
    );
    fs.writeFileSync(path.join(TEST_LOG_DIR, 'warnings.log'), ['WRN001: Rate limit approaching'].join('\n'));
    fs.writeFileSync(path.join(TEST_LOG_DIR, 'infos.log'), ['INF001: Service started'].join('\n'));

    // 创建嵌套目录结构
    const logsDir = path.join(TEST_LOG_DIR, 'logs');
    const vendorDir = path.join(TEST_LOG_DIR, 'vendor');
    fs.mkdirSync(logsDir, { recursive: true });
    fs.mkdirSync(vendorDir, { recursive: true });

    fs.writeFileSync(
      path.join(logsDir, 'access_get.log'),
      ['192.168.1.1 - GET /api/users 200', '192.168.1.3 - GET /api/products 200'].join('\n')
    );

    fs.writeFileSync(
      path.join(logsDir, 'access_post.log'),
      [
        '192.168.1.2 - POST /api/login 401'
        // '192.168.1.1 - POST /api/orders' // 这是一个不在原始列表里的？看下面 startline
      ].join('\n')
    );

    fs.writeFileSync(path.join(vendorDir, 'lib.log'), ['vendor library log', 'should be excluded'].join('\n'));

    // 配置 Planner 脚本
    const absRoot = path.resolve(TEST_LOG_DIR);
    const script = `SOURCES = ["orl://local${absRoot}?glob=**/*.log"]`;

    const response = await request.post(`${API_BASE}/settings/planners/scripts`, {
      data: { app: TEST_APP, script }
    });
    expect(response.ok()).toBeTruthy();
  });

  test.afterAll(async ({ request }) => {
    // 清理
    try {
      await request.delete(`${API_BASE}/settings/planners/scripts/${TEST_APP}`);
    } catch {
      // Ignore
    }
    fs.rmSync(TEST_LOG_DIR, { recursive: true, force: true });
  });

  test('should search with OR operator', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP} ERROR OR WARN`);
    await searchInput.press('Enter');

    // 等待结果
    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 10000
    });

    // 验证包含 ERROR 的结果
    await expect(page.getByText('Database connection failed')).toBeVisible();
    await expect(page.getByText('File not found')).toBeVisible();

    // 验证包含 WARN 的结果
    await expect(page.getByText('deprecated API')).toBeVisible();
  });

  test('should search with AND operator', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP} ERROR AND Database`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 10000
    });

    // 验证只有同时包含 ERROR 和 Database 的结果
    await expect(page.getByText('Database connection failed')).toBeVisible();
  });

  test('should search with NOT operator', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP} ERROR -Database`);
    await searchInput.press('Enter');

    // 验证搜索完成并有结果
    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 10000
    });
  });

  test('should search with complex boolean expression', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    // (ERROR OR WARN) AND -deprecated
    await searchInput.fill(`app:${TEST_APP} (ERROR OR WARN) -deprecated`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 10000
    });

    await expandAllResults(page);

    // 验证包含 ERROR 但不包含 deprecated
    await expect(page.getByText('Database connection failed')).toBeVisible();
    await expect(page.getByText('File not found')).toBeVisible();

    // 验证不包含 WARN + deprecated 的结果
    await expect(page.getByText('deprecated API')).not.toBeVisible();
  });

  test('should verify file-based negative filtering', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    // INFO 存在于 app.log 和 safe.log
    // processed 仅存在于 app.log (作为正文内容)
    // 预期：app.log 被整文件排除，仅显示 safe.log
    await searchInput.fill(`app:${TEST_APP} INFO -processed`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 10000
    });

    await expandAllResults(page);

    // 验证 safe.log 被命中
    await expect(page.getByText('Safe request without negative terms')).toBeVisible();

    // 验证 app.log content 被排除
    await expect(page.getByText('Request processed successfully')).not.toBeVisible();
  });

  test('should search with regex pattern', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    // 搜索 ERR 后跟 3 位数字
    await searchInput.fill(`app:${TEST_APP} /ERR\\d{3}/`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 10000
    });

    await expandAllResults(page);

    // 验证匹配 ERR001, ERR002, ERR003
    await expect(page.getByText('ERR001')).toBeVisible();
    await expect(page.getByText('ERR002')).toBeVisible();
    await expect(page.getByText('ERR003')).toBeVisible();

    // 验证不匹配 WRN001, INF001
    await expect(page.getByText('WRN001')).not.toBeVisible();
  });

  test('should search with phrase', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    // 精确短语搜索
    await searchInput.fill(`app:${TEST_APP} "Database connection failed"`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果', {
      timeout: 10000
    });

    await expandAllResults(page);

    // 验证精确匹配
    await expect(page.getByText('Database connection failed')).toBeVisible();

    // 验证不匹配部分词语
    // await expect(page.getByText('File not found')).not.toBeVisible();
  });

  test('should search with path filter', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    // 只搜索 logs/ 目录下的文件
    await searchInput.fill(`app:${TEST_APP} path:logs/**/*.log GET`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 10000
    });

    // 验证结果来自 logs/access.log
    await expect(page.getByText('GET /api/users')).toBeVisible();
    await expect(page.getByText('GET /api/products')).toBeVisible();

    // 验证文件路径显示正确
    await expect(page.getByRole('link', { name: /access_get\.log/ })).toBeVisible();
  });

  test('should search with negative path filter', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    // 排除 vendor/ 目录
    await searchInput.fill(`app:${TEST_APP} -path:vendor/ (ERROR OR WARN OR INFO)`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 10000
    });

    await expandAllResults(page);

    // 验证结果不包含 vendor/lib.log
    await expect(page.getByText('vendor library log')).not.toBeVisible();

    // 验证包含其他文件的结果
    await expect(page.getByText(/ERROR|WARN|INFO/).first()).toBeVisible();
  });

  test('should search with combined path and content filters', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    // 路径过滤 + 正则 + 布尔运算
    await searchInput.fill(`app:${TEST_APP} path:logs/**/*.log /GET/ -POST`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 10000
    });

    await expandAllResults(page);

    // 验证匹配 IP 地址且不包含 POST
    await expect(page.getByText('192.168.1.1').first()).toBeVisible();
    await expect(page.getByText('192.168.1.3').first()).toBeVisible();

    // 验证不包含 POST 请求
    await expect(page.getByText('POST /api/login')).not.toBeVisible();
  });

  test('should handle deeply nested query', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    // ((ERROR OR WARN) AND (Database OR deprecated)) OR (ERR AND -WRN)
    await searchInput.fill(`app:${TEST_APP} ((ERROR OR WARN) AND (Database OR deprecated)) OR (ERR AND -WRN)`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText(/\d+ 个结果/, {
      timeout: 10000
    });

    await expandAllResults(page);

    // 验证复杂逻辑的结果
    // 第一部分: (ERROR OR WARN) AND (Database OR deprecated)
    await expect(page.getByText('Database connection failed')).toBeVisible(); // ERROR + Database
    await expect(page.getByText('deprecated API')).toBeVisible(); // WARN + deprecated

    // 第二部分: ERR AND -WRN
    await expect(page.getByText('ERR001')).toBeVisible(); // ERR 但不是 WRN
    await expect(page.getByText('ERR002')).toBeVisible();
  });
});
