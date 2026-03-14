/**
 * Loading State E2E Tests
 *
 * Tests for loading state visual feedback on search and explorer pages:
 * - Search spinner appearance and input disabled state during loading
 * - Spinner-to-content transition after search completes
 * - Explorer refresh button spin animation and back button disabled state
 */

import { test, expect } from '@playwright/test';

test.describe('Search Loading States', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/search');
    await page.waitForLoadState('networkidle');
  });

  test('LOAD-01: should show spinner and disable input during search', async ({ page }) => {
    // Mock search API with 500ms delay to create observable loading window
    await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
      await new Promise((resolve) => setTimeout(resolve, 500));
      await route.fulfill({
        status: 200,
        headers: { 'Content-Type': 'application/x-ndjson' },
        body: ''
      });
    });

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('loading test');
    await searchInput.press('Enter');

    // Wait for spinner to appear
    await page.waitForFunction(
      () => document.querySelector('.animate-spin') !== null,
      { timeout: 10000 }
    );

    // Assert: search input is disabled during loading
    await expect(searchInput).toBeDisabled();
  });

  test('LOAD-02: should hide spinner and show content after search completes', async ({ page }) => {
    // Mock search API with 500ms delay, then return empty NDJSON
    await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
      await new Promise((resolve) => setTimeout(resolve, 500));
      await route.fulfill({
        status: 200,
        headers: { 'Content-Type': 'application/x-ndjson' },
        body: ''
      });
    });

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('transition test');
    await searchInput.press('Enter');

    // Wait for spinner to appear first
    await page.waitForFunction(
      () => document.querySelector('.animate-spin') !== null,
      { timeout: 10000 }
    );

    // Wait for spinner to disappear after search completes
    await page.waitForFunction(
      () => document.querySelector('.animate-spin') === null,
      { timeout: 15000 }
    );

    // Assert: result count is no longer "搜索结果" (has transitioned from loading state)
    const resultCountText = await page.locator('h2.text-lg.font-semibold').textContent();
    expect(resultCountText).not.toBe('搜索结果');
  });
});

test.describe('Explorer Loading States', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/explorer');
    await page.waitForLoadState('networkidle');
  });

  test('LOAD-03: should show refresh spin animation and disable back button during load', async ({ page }) => {
    // Mock explorer API with 500ms delay to create observable loading window
    await page.route('**/api/v1/explorer/list', async (route) => {
      await new Promise((resolve) => setTimeout(resolve, 500));
      await route.fulfill({
        status: 200,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ items: [] })
      });
    });

    // Click the refresh button (title="刷新")
    const refreshButton = page.getByTitle('刷新');
    await refreshButton.click();

    // Wait for spinner to appear on RefreshCw icon
    await page.waitForFunction(
      () => document.querySelector('.animate-spin') !== null,
      { timeout: 10000 }
    );

    // Assert: back button (title="后退") is disabled during loading
    const backButton = page.getByTitle('后退');
    await expect(backButton).toBeDisabled();

    // Wait for spinner to disappear after load completes
    await page.waitForFunction(
      () => document.querySelector('.animate-spin') === null,
      { timeout: 15000 }
    );

    // Assert: back button is enabled after loading completes
    await expect(backButton).toBeEnabled();
  });
});
