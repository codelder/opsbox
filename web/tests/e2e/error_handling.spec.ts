/**
 * Error Handling E2E Tests
 *
 * Tests for error scenario user feedback on the search page:
 * - API 500 error display with specific error message
 * - Network timeout error display
 * - Error details expand/collapse and retry button
 * - Search cancellation clearing loading state
 */

import { test, expect } from '@playwright/test';

test.describe('Error Handling E2E', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/search');
    await page.waitForLoadState('networkidle');
  });

  test('ERROR-01: should display error message on API 500', async ({ page }) => {
    // Mock search API to return 500 error with RFC 7807 Problem Details format
    await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
      await route.fulfill({
        status: 500,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ detail: 'Internal Server Error' })
      });
    });

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('test query');
    await searchInput.press('Enter');

    // Wait for error state to appear (check all h3 elements, not just first)
    await page.waitForFunction(
      () => {
        const h3s = document.querySelectorAll('h3');
        return Array.from(h3s).some((h3) => h3.textContent?.includes('搜索出错'));
      },
      { timeout: 10000 }
    );

    // Assert: error title is visible
    await expect(page.locator('h3', { hasText: '搜索出错' })).toBeVisible();

    // Assert: error message contains "Internal Server Error"
    await expect(page.locator('p', { hasText: 'Internal Server Error' }).first()).toBeVisible();

    // Assert: retry button is visible
    await expect(page.getByRole('button', { name: '重新搜索' })).toBeVisible();
  });

  test('ERROR-02: should display error on network timeout', async ({ page }) => {
    // Mock search API to simulate network timeout
    await page.route('**/api/v1/logseek/search.ndjson', (route) => {
      route.abort('timedout');
    });

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('timeout test');
    await searchInput.press('Enter');

    // Wait for error state to appear (check all h3 elements)
    await page.waitForFunction(
      () => {
        const h3s = document.querySelectorAll('h3');
        return Array.from(h3s).some((h3) => h3.textContent?.includes('搜索出错'));
      },
      { timeout: 10000 }
    );

    // Assert: error title is visible
    await expect(page.locator('h3', { hasText: '搜索出错' })).toBeVisible();

    // Assert: error message paragraph is visible (any error text)
    await expect(
      page.locator('h3', { hasText: '搜索出错' }).locator('xpath=following-sibling::p').first()
    ).toBeVisible();

    // Assert: retry button is visible
    await expect(page.getByRole('button', { name: '重新搜索' })).toBeVisible();
  });

  test('ERROR-03: should expand/collapse error details and retry', async ({ page }) => {
    // Mock search API to return 500 error
    await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
      await route.fulfill({
        status: 500,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ detail: 'Test error for details panel' })
      });
    });

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('details test');
    await searchInput.press('Enter');

    // Wait for error state (check all h3 elements)
    await page.waitForFunction(
      () => {
        const h3s = document.querySelectorAll('h3');
        return Array.from(h3s).some((h3) => h3.textContent?.includes('搜索出错'));
      },
      { timeout: 10000 }
    );

    // Assert: error details summary is visible
    const errorDetailsSummary = page.locator('summary', { hasText: '错误详情' });
    await expect(errorDetailsSummary).toBeVisible();

    // Assert: details element is initially open (has 'open' attribute)
    const errorDetails = page.locator('details').filter({ has: errorDetailsSummary });
    await expect(errorDetails).toHaveAttribute('open', '');

    // Collapse the error details by clicking summary
    await errorDetailsSummary.click();
    await page.waitForTimeout(300);

    // Verify collapsed (details element should no longer have 'open')
    await expect(errorDetails).not.toHaveAttribute('open', '');

    // Expand again
    await errorDetailsSummary.click();
    await page.waitForTimeout(300);
    await expect(errorDetails).toHaveAttribute('open', '');

    // Assert: retry button is clickable and triggers new search
    const retryButton = page.getByRole('button', { name: '重新搜索' });
    await expect(retryButton).toBeVisible();
    await retryButton.click();

    // After retry, the error state should reset (loading spinner appears or retry button disappears)
    await page.waitForFunction(
      () => {
        const spinner = document.querySelector('.animate-spin');
        const hasRetry = Array.from(document.querySelectorAll('button')).some((b) =>
          b.textContent?.includes('重新搜索')
        );
        return spinner !== null || !hasRetry;
      },
      { timeout: 10000 }
    );
  });

  test('ERROR-04: should clear loading spinner on search cancellation', async ({ page }) => {
    // Mock search API to abort immediately
    await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
      await route.abort('connectionreset');
    });

    const searchInput = page.getByPlaceholder('搜索...');

    // Trigger search - it will fail due to abort
    await searchInput.fill('cancel test');
    await searchInput.press('Enter');

    // Wait for error to appear (search fails with error state)
    await page.waitForFunction(
      () => {
        const h3s = document.querySelectorAll('h3');
        return Array.from(h3s).some((h3) => h3.textContent?.includes('搜索出错'));
      },
      { timeout: 10000 }
    );

    // Assert: loading spinner is not visible (search finished with error)
    await expect(page.locator('.animate-spin')).not.toBeVisible();

    // Clear the input and fill with new text to verify new search can be initiated
    // The clear button (aria-label="清除搜索内容") appears when input has text
    await searchInput.clear();
    await searchInput.fill('another query');

    // The clear button should be visible when there's text in the input
    const clearButton = page.getByRole('button', { name: '清除搜索内容' });
    await expect(clearButton).toBeVisible();

    // Click clear button
    await clearButton.click();

    // Assert: input is cleared
    await expect(searchInput).toHaveValue('');

    // Assert: can initiate a new search without errors
    await searchInput.fill('new search');
    await expect(searchInput).toHaveValue('new search');

    // Verify no error state is shown (search hasn't been triggered yet)
    await expect(page.locator('h3', { hasText: '搜索出错' })).not.toBeVisible();
  });
});
