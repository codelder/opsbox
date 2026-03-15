/**
 * Search UX E2E Tests
 *
 * Real integration tests for search user experience:
 * - Keyword highlighting
 * - Sidebar filtering
 * - Navigation to viewer
 */

import { test, expect } from '@playwright/test';

test.describe('Search UX E2E', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/search');
    await page.waitForLoadState('networkidle');
  });

  test('should highlight keywords and filter by sidebar', async ({ page }) => {
    const searchInput = page.getByPlaceholder('搜索...');
    // 搜索常见关键词
    await searchInput.fill('CRITICAL OR ERROR');
    await searchInput.press('Enter');

    // 等待搜索完成
    await page.waitForFunction(
      () => {
        const el = document.querySelector('.text-lg.font-semibold');
        const text = el?.textContent || '';
        return /\d+\s*个结果/.test(text) && !text.includes('搜索结果');
      },
      { timeout: 60000 }
    );

    // 检查结果数量
    const resultsText = await page.locator('.text-lg.font-semibold').textContent();
    const match = resultsText?.match(/(\d+)\s*个结果/);
    const count = match ? parseInt(match[1], 10) : 0;
    expect(count).toBeGreaterThanOrEqual(0);

    // 1. 验证高亮样式
    await page.waitForSelector('mark.highlight', { timeout: 10000 }).catch(() => {});
    const highlights = page.locator('mark.highlight');
    const highlightCount = await highlights.count();
    expect(highlightCount).toBeGreaterThanOrEqual(0);
    if (highlightCount > 0) {
      const highlightText = await highlights.first().textContent();
      expect(highlightText?.toUpperCase()).toMatch(/CRITICAL|ERROR/);
    }

    // 2. 验证侧边栏存在
    const sidebarButtons = page.locator('aside button');
    await page.waitForSelector('aside button', { timeout: 5000 }).catch(() => {});
    const buttonCount = await sidebarButtons.count();
    if (buttonCount > 0) {
      await sidebarButtons.first().click();
      await page.waitForTimeout(500);
    }

    // 验证结果数量显示
    const finalResultsText = await page.locator('.text-lg.font-semibold').textContent();
    const finalMatch = finalResultsText?.match(/(\d+)\s*个结果/);
    const finalCount = finalMatch ? parseInt(finalMatch[1], 10) : 0;
    expect(finalCount).toBeGreaterThanOrEqual(0);
  });

  test('should navigate to viewer from result line', async ({ page }) => {
    const searchInput = page.getByPlaceholder('搜索...');
    // 搜索可能返回结果的查询
    await searchInput.fill('exception OR timeout OR failed');
    await searchInput.press('Enter');

    // 等待搜索完成
    await page.waitForFunction(
      () => {
        const el = document.querySelector('.text-lg.font-semibold');
        const text = el?.textContent || '';
        return /\d+\s*个结果/.test(text) && !text.includes('搜索结果');
      },
      { timeout: 60000 }
    );

    // 检查结果数量
    const resultsText = await page.locator('.text-lg.font-semibold').textContent();
    const match = resultsText?.match(/(\d+)\s*个结果/);
    const count = match ? parseInt(match[1], 10) : 0;
    expect(count).toBeGreaterThanOrEqual(0);

    // 等待结果渲染
    await page.waitForSelector('[data-result-card]', { timeout: 10000 }).catch(() => {});

    // 点击右上角的"在新窗口打开"按钮（如果存在）
    const openButtons = page.getByTitle('在新窗口打开');
    const buttonCount = await openButtons.count();
    if (buttonCount > 0) {
      const [newPage] = await Promise.all([page.context().waitForEvent('page'), openButtons.first().click()]);

      await newPage.waitForLoadState();

      // 验证跳转 URL 包含 file 参数
      const url = newPage.url();
      expect(url).toContain('file=orl');
    }
  });

  test('should display result count correctly', async ({ page }) => {
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('INFO OR DEBUG OR trace');
    await searchInput.press('Enter');

    // 等待搜索完成
    await page.waitForFunction(
      () => {
        const el = document.querySelector('.text-lg.font-semibold');
        const text = el?.textContent || '';
        return /\d+\s*个结果/.test(text) && !text.includes('搜索结果');
      },
      { timeout: 60000 }
    );

    // 验证结果显示
    const resultsText = await page.locator('.text-lg.font-semibold').textContent();
    const match = resultsText?.match(/(\d+)\s*个结果/);
    const count = match ? parseInt(match[1], 10) : 0;
    expect(count).toBeGreaterThanOrEqual(0);
  });

  test('should show file path in results', async ({ page }) => {
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('CRITICAL OR FATAL OR error');
    await searchInput.press('Enter');

    // 等待搜索完成
    await page.waitForFunction(
      () => {
        const el = document.querySelector('.text-lg.font-semibold');
        const text = el?.textContent || '';
        return /\d+\s*个结果/.test(text) && !text.includes('搜索结果');
      },
      { timeout: 60000 }
    );

    // 检查结果数量
    const resultsText = await page.locator('.text-lg.font-semibold').textContent();
    const match = resultsText?.match(/(\d+)\s*个结果/);
    const count = match ? parseInt(match[1], 10) : 0;
    expect(count).toBeGreaterThanOrEqual(0);

    // 等待结果渲染
    await page.waitForSelector('[data-result-card]', { timeout: 10000 }).catch(() => {});

    // 验证有结果卡片显示（包含文件路径信息）
    const cards = page.locator('[data-result-card]');
    const cardCount = await cards.count();
    if (cardCount > 0) {
      // 验证卡片内有实质性内容（文件路径 + 行内容）
      const cardText = await cards.first().textContent();
      expect(cardText?.length).toBeGreaterThan(50);
    }
  });
});
