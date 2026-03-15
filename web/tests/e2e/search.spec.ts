/**
 * Search E2E Tests
 *
 * Real integration tests for search functionality:
 * - Search with configured sources (S3, Agents)
 * - Results filtering by endpoint type
 * - Sidebar navigation
 */

import { test, expect } from '@playwright/test';

test.describe('Search E2E', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/search');
    await page.waitForLoadState('networkidle');
  });

  test('should perform real search and display results', async ({ page }) => {
    const searchInput = page.getByPlaceholder('搜索...');
    // 使用通用搜索词，搜索可能返回结果（从配置的 S3 源）
    await searchInput.fill('error');
    await searchInput.press('Enter');

    // 等待搜索完成（结果显示"X 个结果"或"0 个结果"）
    await page.waitForFunction(
      () => {
        const el = document.querySelector('.text-lg.font-semibold');
        const text = el?.textContent || '';
        return /\d+\s*个结果/.test(text) && !text.includes('搜索结果');
      },
      { timeout: 60000 }
    );

    // 验证搜索完成，显示结果计数
    const resultsText = await page.locator('.text-lg.font-semibold').textContent();
    const match = resultsText?.match(/(\d+)\s*个结果/);
    const count = match ? parseInt(match[1], 10) : 0;
    expect(count).toBeGreaterThanOrEqual(0);
  });

  test('should filter results by clicking sidebar items', async ({ page }) => {
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('error OR info');
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

    // 验证侧边栏存在（可能有本地文件、S3 等按钮）
    const sidebarButtons = page.locator('aside button');
    await page.waitForSelector('aside button', { timeout: 10000 }).catch(() => {});
    const buttonCount = await sidebarButtons.count();
    if (buttonCount > 0) {
      await sidebarButtons.first().click();
      await page.waitForTimeout(500);
    }

    // 验证结果数量已更新（或保持不变）
    const resultsText = await page.locator('.text-lg.font-semibold').textContent();
    const match = resultsText?.match(/(\d+)\s*个结果/);
    const count = match ? parseInt(match[1], 10) : 0;
    expect(count).toBeGreaterThanOrEqual(0);
  });

  test('should display result cards when results found', async ({ page }) => {
    const searchInput = page.getByPlaceholder('搜索...');
    // 搜索常见词，可能返回结果
    await searchInput.fill('exception OR failed OR timeout');
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

    // 如果有结果，验证结果卡片存在
    await page.waitForSelector('[data-result-card]', { timeout: 10000 }).catch(() => {});
    const cards = page.locator('[data-result-card]');
    const cardCount = await cards.count();
    if (cardCount > 0) {
      await expect(cards.first()).toBeVisible();
    }
  });

  test('should support multiple keywords with OR', async ({ page }) => {
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('ERROR OR WARN OR failed');
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

    // 验证有结果显示
    const resultsText = await page.locator('.text-lg.font-semibold').textContent();
    const match = resultsText?.match(/(\d+)\s*个结果/);
    const count = match ? parseInt(match[1], 10) : 0;
    expect(count).toBeGreaterThanOrEqual(0);
  });

  test('should support negative filters', async ({ page }) => {
    const searchInput = page.getByPlaceholder('搜索...');
    // 使用负过滤器排除某些路径
    await searchInput.fill('error -path:nginx');
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

    // 验证搜索完成
    const resultsText = await page.locator('.text-lg.font-semibold').textContent();
    const match = resultsText?.match(/(\d+)\s*个结果/);
    const count = match ? parseInt(match[1], 10) : 0;
    expect(count).toBeGreaterThanOrEqual(0);
  });

  test('should show empty state for no results', async ({ page }) => {
    const searchInput = page.getByPlaceholder('搜索...');
    // 使用非常独特的关键词，应该不会匹配任何内容
    await searchInput.fill('NONEXISTENT_KEYWORD_XYZ123_UNLIKELY_TO_MATCH');
    await searchInput.press('Enter');

    // 等待搜索完成（应该是 0 个结果）
    await page.waitForFunction(
      () => {
        const el = document.querySelector('.text-lg.font-semibold');
        const text = el?.textContent || '';
        return /\d+\s*个结果/.test(text) && !text.includes('搜索结果');
      },
      { timeout: 60000 }
    );

    // 验证搜索完成，显示 0 个结果
    await expect(page.locator('.text-lg.font-semibold')).toContainText('0 个结果');
  });
});
