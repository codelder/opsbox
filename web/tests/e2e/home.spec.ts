/**
 * Home Page E2E Tests
 *
 * Real integration tests for the home page (/) functionality:
 * - Page layout and navigation
 * - Quick search input with real search
 * - Navigation to other pages
 * - Syntax hints display
 */

import { test, expect } from '@playwright/test';

test.describe('Home Page E2E', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
  });

  test('should submit search via Enter key', async ({ page }) => {
    // 填入搜索词并提交
    const searchInput = page
      .getByRole('search')
      .getByRole('textbox')
      .or(page.locator('textarea, input[type="text"]').first());
    await searchInput.fill('ERROR');
    await searchInput.press('Enter');

    // 验证跳转到搜索页面
    await page.waitForURL('**/search**', { timeout: 10000 });
    expect(page.url()).toContain('/search');
  });

  test('should click syntax hint buttons to insert text', async ({ page }) => {
    await page.waitForSelector('button:has-text("OR")', { timeout: 10000 });

    const searchInput = page
      .getByRole('search')
      .getByRole('textbox')
      .or(page.locator('textarea, input[type="text"]').first());

    // 点击 OR 按钮应该插入 OR 到搜索框
    const orButton = page.getByRole('button', { name: 'OR', exact: true });
    await orButton.click();

    // 验证输入框包含 OR
    const inputValue = await searchInput.inputValue();
    expect(inputValue).toContain('OR');
  });
});
