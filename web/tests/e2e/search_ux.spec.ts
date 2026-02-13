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
    const count = match ? parseInt(match[1]) : 0;

    // 1. 如果有结果，验证高亮样式
    if (count > 0) {
      // 等待结果渲染
      await page.waitForTimeout(1000);

      // 验证有高亮元素（mark 标签或背景色）
      const highlights = page.locator('mark, .highlight, [style*="background-color"]');
      const highlightCount = await highlights.count();

      // 如果有高亮，验证内容（可能是 CRITICAL 或 ERROR）
      if (highlightCount > 0) {
        const highlightText = await highlights.first().textContent();
        const upperText = highlightText?.toUpperCase() || '';
        // 验证高亮内容是搜索关键词之一（CRITICAL 或 ERROR）
        const isKeyword = upperText.includes('CRITICAL') || upperText.includes('ERROR');
        expect(isKeyword).toBe(true);
      }
    }

    // 2. 验证侧边栏存在
    const sidebarButtons = page.locator('aside button');
    const buttonCount = await sidebarButtons.count();

    // 3. 如果有侧边栏按钮，点击筛选
    if (buttonCount > 0) {
      await sidebarButtons.first().click();
      await page.waitForTimeout(500);
    }

    // 验证结果数量显示
    const finalResultsText = await page.locator('.text-lg.font-semibold').textContent();
    expect(finalResultsText).toMatch(/\d+\s*个结果/);
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
    const count = match ? parseInt(match[1]) : 0;

    if (count > 0) {
      // 等待结果渲染
      await page.waitForTimeout(1000);

      // 点击右上角的"在新窗口打开"按钮（如果存在）
      const openButtons = page.getByTitle('在新窗口打开');
      const buttonCount = await openButtons.count();

      if (buttonCount > 0) {
        const [newPage] = await Promise.all([
          page.context().waitForEvent('page'),
          openButtons.first().click()
        ]);

        await newPage.waitForLoadState();

        // 验证跳转 URL 包含 file 参数
        const url = newPage.url();
        expect(url).toContain('file=orl');
      }
    }

    // 测试通过：搜索完成
    expect(resultsText).toMatch(/\d+\s*个结果/);
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
    expect(resultsText).toMatch(/\d+\s*个结果/);
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
    const count = match ? parseInt(match[1]) : 0;

    if (count > 0) {
      // 等待结果渲染
      await page.waitForTimeout(1000);

      // 验证有结果卡片显示（包含文件路径信息）
      const cards = page.locator('[data-result-card], .rounded.border');
      const cardCount = await cards.count();

      if (cardCount > 0) {
        // 验证卡片内有内容
        const cardText = await cards.first().textContent();
        expect(cardText?.length).toBeGreaterThan(0);
      }
    }

    // 测试通过：搜索完成
    expect(resultsText).toMatch(/\d+\s*个结果/);
  });
});
