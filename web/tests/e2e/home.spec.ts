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

  test('should display home page with logo and search input', async ({ page }) => {
    // 等待页面主要内容加载
    await page.waitForSelector('search, [role="search"], textarea, input', { timeout: 10000 });

    // 验证搜索输入框存在
    const searchInput = page.getByRole('search').getByRole('textbox').or(page.locator('textarea, input[type="text"]').first());
    await expect(searchInput).toBeVisible();

    // 验证 AI 模式按钮存在
    const aiButton = page.getByRole('button', { name: /AI 模式|AI mode/i });
    await expect(aiButton).toBeVisible();
  });

  test('should display syntax hints', async ({ page }) => {
    // 等待语法提示按钮出现
    await page.waitForSelector('button:has-text("OR"), button:has-text("AND")', { timeout: 10000 });

    // 验证语法提示按钮存在
    const orButton = page.getByRole('button', { name: 'OR', exact: true });
    const andButton = page.getByRole('button', { name: 'AND', exact: true });

    await expect(orButton.first()).toBeVisible();
    await expect(andButton.first()).toBeVisible();
  });

  test('should navigate to search page and perform real search', async ({ page }) => {
    // 填入搜索词并提交
    const searchInput = page.getByRole('search').getByRole('textbox').or(page.locator('textarea, input[type="text"]').first());
    await searchInput.fill('ERROR');
    await searchInput.press('Enter');

    // 验证跳转到搜索页面
    await page.waitForURL('**/search**', { timeout: 10000 });
    expect(page.url()).toContain('/search');
  });

  test('should have navigation links to other pages', async ({ page }) => {
    // 等待页面加载
    await page.waitForSelector('button', { timeout: 10000 });

    // 验证设置按钮存在
    const settingsButton = page.getByRole('button', { name: /打开设置|settings/i });
    await expect(settingsButton).toBeVisible();

    // 验证主题切换按钮存在
    const themeButton = page.getByRole('button', { name: /toggle theme|主题/i });
    await expect(themeButton).toBeVisible();
  });

  test('should support Enter key to submit search', async ({ page }) => {
    const searchInput = page.getByRole('search').getByRole('textbox').or(page.locator('textarea, input[type="text"]').first());
    await searchInput.fill('INFO');
    await searchInput.press('Enter');

    // 验证页面跳转到搜索页面
    await page.waitForURL('**/search**', { timeout: 10000 });
    expect(page.url()).toContain('/search');
  });

  test('should display AI mode button', async ({ page }) => {
    await page.waitForSelector('button:has-text("AI")', { timeout: 10000 });

    const aiButton = page.getByRole('button', { name: /AI 模式|AI mode/i });
    await expect(aiButton).toBeVisible();
  });

  test('should click syntax hint buttons to insert text', async ({ page }) => {
    await page.waitForSelector('button:has-text("OR")', { timeout: 10000 });

    const searchInput = page.getByRole('search').getByRole('textbox').or(page.locator('textarea, input[type="text"]').first());

    // 点击 OR 按钮应该插入 OR 到搜索框
    const orButton = page.getByRole('button', { name: 'OR', exact: true });
    await orButton.click();

    // 验证输入框包含 OR
    const inputValue = await searchInput.inputValue();
    expect(inputValue).toContain('OR');
  });

  test('should have example button', async ({ page }) => {
    await page.waitForSelector('button', { timeout: 10000 });

    const exampleButton = page.getByRole('button', { name: /示例|example/i });
    await expect(exampleButton).toBeVisible();
  });

  test('should have system prompt button', async ({ page }) => {
    await page.waitForSelector('button', { timeout: 10000 });

    const promptButton = page.getByRole('button', { name: /系统提示词|prompt/i });
    await expect(promptButton).toBeVisible();
  });
});
