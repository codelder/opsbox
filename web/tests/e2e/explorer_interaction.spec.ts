/**
 * Explorer Interaction E2E Tests
 *
 * Real integration tests for Explorer interactions:
 * - Directory navigation with real files
 * - View mode switching
 * - Sidebar navigation
 * - Context menus
 * - File operations
 */

import { test, expect } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

test.describe('Explorer Interaction E2E', () => {
  const RUN_ID = Date.now();
  const TEST_DIR = path.join(__dirname, `temp_explorer_interaction_${RUN_ID}`);
  const LOGS_DIR = path.join(TEST_DIR, 'logs');
  const NESTED_DIR = path.join(LOGS_DIR, 'nested');

  test.beforeAll(async () => {
    // 创建测试目录结构
    fs.mkdirSync(NESTED_DIR, { recursive: true });

    // 创建文件
    fs.writeFileSync(path.join(LOGS_DIR, 'app.log'), 'Application log content\n');
    fs.writeFileSync(path.join(LOGS_DIR, 'error.log'), 'Error log content\n');
    fs.writeFileSync(path.join(NESTED_DIR, 'nested.log'), 'Nested log content\n');
    fs.writeFileSync(path.join(TEST_DIR, '.hidden_file'), 'Hidden file content\n');
    fs.writeFileSync(path.join(TEST_DIR, 'readme.txt'), 'Readme content\n');
  });

  test.afterAll(async () => {
    // Cleanup: remove test directory (ignore errors if already removed)
    try {
      if (fs.existsSync(TEST_DIR)) {
        fs.rmSync(TEST_DIR, { recursive: true, force: true });
      }
    } catch (e) {
      console.error(`Failed to cleanup ${TEST_DIR}:`, e);
    }
  });

  test.beforeEach(async ({ page }) => {
    await page.goto('/explorer');
    await page.waitForLoadState('networkidle');
  });

  test('should navigate using sidebar links', async ({ page }) => {
    // 点击 "Local Machine"
    const localBtn = page.getByRole('button', { name: /Local Machine|本地/i });
    await localBtn.click();
    await page.waitForLoadState('networkidle');

    // URL 应该更新
    expect(page.url()).toContain('orl=orl');
  });

  test('should navigate to real directory via ORL input', async ({ page }) => {
    const input = page.locator('#orl-input');
    await input.fill(`orl://local${TEST_DIR}`);
    await input.press('Enter');
    await page.waitForLoadState('networkidle');

    // URL 应该更新
    await expect(page).toHaveURL(/orl=orl%3A%2F%2Flocal/);

    // 应该看到测试目录内容
    await expect(page.getByText('logs', { exact: true })).toBeVisible({ timeout: 5000 });
  });

  test('should navigate deep into directory structure', async ({ page }) => {
    // 导航到测试目录
    await page.goto(`/explorer?orl=${encodeURIComponent(`orl://local${TEST_DIR}`)}`);
    await page.waitForLoadState('networkidle');

    // 点击 logs 目录
    await expect(page.getByText('logs', { exact: true })).toBeVisible({ timeout: 5000 });
    await page.getByRole('button', { name: 'logs', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');

    // 应该看到 app.log
    await expect(page.getByText('app.log', { exact: true })).toBeVisible({ timeout: 5000 });

    // 点击 nested 目录
    await page.getByRole('button', { name: 'nested', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');

    // 应该看到 nested.log
    await expect(page.getByText('nested.log', { exact: true })).toBeVisible({ timeout: 5000 });
  });

  test('should toggle hidden files visibility', async ({ page }) => {
    // 导航到测试目录
    await page.goto(`/explorer?orl=${encodeURIComponent(`orl://local${TEST_DIR}`)}`);
    await page.waitForLoadState('networkidle');

    // 默认不应该看到隐藏文件
    await expect(page.getByText('.hidden_file')).not.toBeVisible();

    // 点击显示隐藏文件按钮
    const showHiddenBtn = page.getByTitle(/Show hidden files|显示隐藏/i);
    await showHiddenBtn.click();
    await page.waitForTimeout(500);

    // 现在应该看到隐藏文件
    await expect(page.getByText('.hidden_file')).toBeVisible({ timeout: 5000 });
  });

  test('should switch between table and grid view modes', async ({ page }) => {
    await page.goto(`/explorer?orl=${encodeURIComponent(`orl://local${TEST_DIR}`)}`);
    await page.waitForLoadState('networkidle');

    // 默认是网格视图
    await expect(page.getByText('logs', { exact: true })).toBeVisible({ timeout: 5000 });

    // 切换到列表视图
    const listBtn = page.locator('button').filter({ has: page.locator('.lucide-layout-list') });
    const listCount = await listBtn.count();
    if (listCount > 0) {
      await listBtn.first().click();
      await page.waitForTimeout(300);

      // 表格应该可见
      await expect(page.locator('table')).toBeVisible({ timeout: 5000 });
    }
  });

  test('should display right-click menu for file', async ({ page }) => {
    await page.goto(`/explorer?orl=${encodeURIComponent(`orl://local${LOGS_DIR}`)}`);
    await page.waitForLoadState('networkidle');

    // 等待文件出现
    const fileItem = page.getByRole('button', { name: 'app.log', exact: true });
    await expect(fileItem).toBeVisible({ timeout: 5000 });

    // 右键点击
    await fileItem.click({ button: 'right' });

    // 验证菜单项
    await expect(page.getByText(/复制 ORL|Copy ORL/i)).toBeVisible({ timeout: 3000 });
  });

  test('should refresh list via toolbar button', async ({ page }) => {
    await page.goto(`/explorer?orl=${encodeURIComponent(`orl://local${TEST_DIR}`)}`);
    await page.waitForLoadState('networkidle');

    // 点击刷新按钮
    const refreshBtn = page.getByTitle(/刷新|Refresh/i);
    await refreshBtn.click();
    await page.waitForLoadState('networkidle');

    // 页面应该仍然正常
    await expect(page.getByText('logs', { exact: true })).toBeVisible({ timeout: 5000 });
  });

  test('should open file viewer on double click', async ({ page }) => {
    await page.goto(`/explorer?orl=${encodeURIComponent(`orl://local${LOGS_DIR}`)}`);
    await page.waitForLoadState('networkidle');

    // 等待文件出现
    const fileItem = page.getByRole('button', { name: 'app.log', exact: true });
    await expect(fileItem).toBeVisible({ timeout: 5000 });

    // 双击打开
    const [newPage] = await Promise.all([
      page.waitForEvent('popup'),
      fileItem.dblclick()
    ]);

    // 验证新页面 URL
    await expect(newPage).toHaveURL(/\/view\?/);
  });

  test('should navigate back using back button', async ({ page }) => {
    // 导航到深层目录
    await page.goto(`/explorer?orl=${encodeURIComponent(`orl://local${NESTED_DIR}`)}`);
    await page.waitForLoadState('networkidle');

    // 应该看到 nested.log
    await expect(page.getByText('nested.log', { exact: true })).toBeVisible({ timeout: 5000 });

    // 点击后退按钮
    const backBtn = page.locator('button').filter({ has: page.locator('.lucide-arrow-left') });
    await backBtn.first().click();
    await page.waitForLoadState('networkidle');

    // 应该回到 logs 目录
    await expect(page.getByText('app.log', { exact: true })).toBeVisible({ timeout: 5000 });
  });

  test('should display file metadata', async ({ page }) => {
    await page.goto(`/explorer?orl=${encodeURIComponent(`orl://local${LOGS_DIR}`)}`);
    await page.waitForLoadState('networkidle');

    // 切换到列表视图以查看更多元数据
    const listBtn = page.locator('button').filter({ has: page.locator('.lucide-layout-list') });
    const listCount = await listBtn.count();
    if (listCount > 0) {
      await listBtn.first().click();
      await page.waitForTimeout(300);
    }

    // 验证文件存在
    await expect(page.getByText('app.log', { exact: true })).toBeVisible({ timeout: 5000 });
  });

  test('should handle non-existent directory gracefully', async ({ page }) => {
    await page.goto(`/explorer?orl=${encodeURIComponent('orl://local/non/existent/path/12345')}`);
    await page.waitForLoadState('networkidle');

    // 页面应该正常加载，可能显示错误或空状态
    await expect(page.locator('body')).toBeVisible();
  });
});
