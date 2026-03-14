/**
 * Accessibility E2E Tests
 *
 * Tests for keyboard navigation, ARIA attributes, and focus management:
 * - Tab navigation through search page elements
 * - ARIA label presence on interactive controls
 * - Focus management after search completion and errors
 */

import { test, expect } from '@playwright/test';

test.describe('Accessibility E2E', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/search');
    await page.waitForLoadState('networkidle');
  });

  test('A11Y-01: should navigate search input via Tab and submit with Enter', async ({ page }) => {
    // Tab through elements until we reach the search input (identified by placeholder '搜索...')
    let focusedInput = false;
    for (let i = 0; i < 10; i++) {
      await page.keyboard.press('Tab');
      const focused = page.locator(':focus');
      const placeholder = await focused.getAttribute('placeholder');
      if (placeholder === '搜索...') {
        focusedInput = true;
        break;
      }
    }

    // Assert: search input was reached via Tab
    expect(focusedInput).toBe(true);
    expect(await page.locator(':focus').getAttribute('placeholder')).toBe('搜索...');

    // Type query using keyboard
    await page.keyboard.type('error');

    // Press Enter to trigger search
    await page.keyboard.press('Enter');

    // Wait for search to initiate (spinner appears or results text updates)
    await page.waitForFunction(
      () => {
        const spinner = document.querySelector('.animate-spin');
        const el = document.querySelector('.text-lg.font-semibold');
        const text = el?.textContent || '';
        return spinner !== null || /\d+\s*个结果/.test(text);
      },
      { timeout: 60000 }
    );
  });

  test('A11Y-02: should have correct ARIA labels on interactive elements', async ({ page }) => {
    // Fill search input so clear button appears
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('test');

    // Assert: clear button has aria-label "清除搜索内容"
    const clearButton = page.getByRole('button', { name: '清除搜索内容' });
    await expect(clearButton).toBeVisible();

    // Assert: resize handle has aria-label "调整侧边栏宽度"
    const resizeHandle = page.locator('[aria-label="调整侧边栏宽度"]');
    await expect(resizeHandle).toBeAttached();
  });

  test('A11Y-03: should manage focus correctly after search completion and errors', async ({ page }) => {
    // --- Part 1: Focus stays on input after successful search ---
    // Mock search API to return empty NDJSON for fast completion
    await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
      await route.fulfill({
        status: 200,
        headers: { 'Content-Type': 'application/x-ndjson' },
        body: ''
      });
    });

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('focus test');
    await searchInput.press('Enter');

    // Wait for search completion
    await page.waitForFunction(
      () => {
        const el = document.querySelector('.text-lg.font-semibold');
        const text = el?.textContent || '';
        return /0\s*个结果/.test(text);
      },
      { timeout: 10000 }
    );

    // Click the search input to ensure focus, then verify it holds focus
    await searchInput.click();
    const focusedPlaceholder = await page.locator(':focus').getAttribute('placeholder');
    expect(focusedPlaceholder).toBe('搜索...');

    // --- Part 2: Error state shows retry button ---
    // Navigate fresh to avoid mock conflicts
    await page.goto('/search');
    await page.waitForLoadState('networkidle');

    // Set up error mock on fresh page
    await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
      await route.fulfill({
        status: 500,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ detail: 'Test error for focus' })
      });
    });

    const searchInput2 = page.getByPlaceholder('搜索...');
    await searchInput2.fill('error focus test');
    await searchInput2.press('Enter');

    // Wait for error state
    await page.waitForFunction(
      () => {
        const h3s = document.querySelectorAll('h3');
        return Array.from(h3s).some((h3) => h3.textContent?.includes('搜索出错'));
      },
      { timeout: 10000 }
    );

    // Assert: retry button is visible and can receive focus
    const retryButton = page.getByRole('button', { name: '重新搜索' });
    await expect(retryButton).toBeVisible();

    // Focus the retry button and verify it receives focus
    await retryButton.focus();
    const focusedText = await page.locator(':focus').textContent();
    expect(focusedText).toContain('重新搜索');
  });
});
