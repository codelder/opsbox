/**
 * Search Error Visibility E2E Tests
 *
 * Tests for search error visibility feature:
 * - Partial source failures display warning indicator
 * - Error details panel shows failed sources
 * - Statistics show correct success/failed counts
 * - All sources success shows green indicator
 */
import { test, expect } from '@playwright/test';

test.describe('Search Error Visibility', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/search');
    await page.waitForLoadState('networkidle');
  });

  test('SEV-01: should show warning indicator when some sources fail', async ({ page }) => {
    // Mock NDJSON stream with partial failures
    await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
      const ndjsonLines = [
        // Success result
        JSON.stringify({
          type: 'result',
          data: {
            path: 'orl://local/var/log/success.log',
            keywords: [{ type: 'literal', text: 'error' }],
            chunks: [{ range: [1, 3], lines: [{ no: 1, text: 'error line 1' }] }],
            encoding: 'UTF-8'
          }
        }),
        // Error event - timeout
        JSON.stringify({
          type: 'error',
          source: 'orl://local/var/log/large-file.log',
          message: '处理超时 (超过 60s)',
          recoverable: true
        }),
        // Error event - connection refused
        JSON.stringify({
          type: 'error',
          source: 'orl://agent-01@agent/var/log/remote.log',
          message: 'Agent 连接被拒绝',
          recoverable: true
        }),
        // Complete event
        JSON.stringify({
          type: 'complete',
          source: 'local:/var/log',
          elapsed_ms: 1500
        }),
        // Finished event with statistics
        JSON.stringify({
          type: 'finished',
          total_sources: 3,
          successful_sources: 1,
          failed_sources: 2,
          total_elapsed_ms: 2350
        })
      ];

      await route.fulfill({
        status: 200,
        headers: {
          'Content-Type': 'application/x-ndjson; charset=utf-8',
          'X-Logseek-SID': 'test-sid-123'
        },
        body: ndjsonLines.join('\n')
      });
    });

    // Perform search
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('error');
    await searchInput.press('Enter');

    // Wait for status indicator to appear
    await page.waitForTimeout(1500);

    // Look for status button by amber class (warning state)
    // lucide-svelte renders SVGs without specific class names, so we use the button's class
    const statusButton = page.locator('header button[class*="amber"]');
    await expect(statusButton).toBeVisible({ timeout: 5000 });

    // Click to expand error panel
    await statusButton.click();

    // Assert: Error panel should appear
    const statusPanel = page.locator('[role="dialog"]').filter({ hasText: '搜索状态' });
    await expect(statusPanel).toBeVisible();

    // Assert: Statistics should show correct counts
    await expect(statusPanel.locator('text=/3.*数据源/')).toBeVisible();
    await expect(statusPanel.locator('text=/1.*成功/')).toBeVisible();
    await expect(statusPanel.locator('text=/2.*失败/')).toBeVisible();

    // Assert: Error details should be listed
    await expect(statusPanel.locator('text=large-file.log')).toBeVisible();
    await expect(statusPanel.locator('text=处理超时')).toBeVisible();
  });

  test('SEV-02: should show success indicator when all sources succeed', async ({ page }) => {
    // Mock NDJSON stream with all successes
    await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
      const ndjsonLines = [
        // Success result
        JSON.stringify({
          type: 'result',
          data: {
            path: 'orl://local/var/log/app.log',
            keywords: [{ type: 'literal', text: 'info' }],
            chunks: [{ range: [1, 2], lines: [{ no: 1, text: 'info message' }] }],
            encoding: 'UTF-8'
          }
        }),
        // Complete event
        JSON.stringify({
          type: 'complete',
          source: 'local:/var/log',
          elapsed_ms: 500
        }),
        // Finished event - all success
        JSON.stringify({
          type: 'finished',
          total_sources: 2,
          successful_sources: 2,
          failed_sources: 0,
          total_elapsed_ms: 850
        })
      ];

      await route.fulfill({
        status: 200,
        headers: {
          'Content-Type': 'application/x-ndjson; charset=utf-8',
          'X-Logseek-SID': 'test-sid-456'
        },
        body: ndjsonLines.join('\n')
      });
    });

    // Perform search
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('info');
    await searchInput.press('Enter');

    // Wait for results
    await page.waitForTimeout(1500);

    // Look for green success button
    const successButton = page.locator('header button[class*="green"]');
    await expect(successButton).toBeVisible({ timeout: 5000 });

    // Click to expand
    await successButton.click();

    // Assert: Panel shows all success
    const statusPanel = page.locator('[role="dialog"]').filter({ hasText: '搜索状态' });
    await expect(statusPanel).toBeVisible();
    await expect(statusPanel.locator('text=/2.*数据源/')).toBeVisible();
    await expect(statusPanel.locator('text=/2.*成功/')).toBeVisible();
  });

  test('SEV-03: should display timeout error with specific file path', async ({ page }) => {
    // Mock NDJSON stream with timeout error containing ORL path
    await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
      const specificPath = 'orl://local/var/log/production/large-service.log';
      const ndjsonLines = [
        // Error event with specific path
        JSON.stringify({
          type: 'error',
          source: specificPath,
          message: '处理超时 (超过 60s)',
          recoverable: true
        }),
        // Finished event
        JSON.stringify({
          type: 'finished',
          total_sources: 1,
          successful_sources: 0,
          failed_sources: 1,
          total_elapsed_ms: 60000
        })
      ];

      await route.fulfill({
        status: 200,
        headers: {
          'Content-Type': 'application/x-ndjson; charset=utf-8',
          'X-Logseek-SID': 'test-sid-789'
        },
        body: ndjsonLines.join('\n')
      });
    });

    // Perform search
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('timeout');
    await searchInput.press('Enter');

    // Wait for status indicator
    await page.waitForTimeout(1500);

    // Find and click the warning indicator
    const statusButton = page.locator('header button[class*="amber"]');
    await expect(statusButton).toBeVisible({ timeout: 5000 });
    await statusButton.click();

    // Assert: Error panel shows the specific file path
    const statusPanel = page.locator('[role="dialog"]').filter({ hasText: '搜索状态' });
    await expect(statusPanel).toBeVisible();

    // The path should be displayed (possibly truncated)
    await expect(statusPanel.locator('text=large-service.log')).toBeVisible();
    await expect(statusPanel.locator('text=处理超时')).toBeVisible();
  });

  test('SEV-04: should show archive entry error details', async ({ page }) => {
    // Mock NDJSON stream with archive entry error
    await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
      const archivePath = 'orl://local/logs/archive.tar.gz?entry=error.log';
      const ndjsonLines = [
        // Error event from archive entry
        JSON.stringify({
          type: 'error',
          source: archivePath,
          message: 'S3 连接被拒绝',
          recoverable: true
        }),
        // Finished event
        JSON.stringify({
          type: 'finished',
          total_sources: 1,
          successful_sources: 0,
          failed_sources: 1,
          total_elapsed_ms: 1200
        })
      ];

      await route.fulfill({
        status: 200,
        headers: {
          'Content-Type': 'application/x-ndjson; charset=utf-8',
          'X-Logseek-SID': 'test-sid-archive'
        },
        body: ndjsonLines.join('\n')
      });
    });

    // Perform search
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('archive error');
    await searchInput.press('Enter');

    // Wait for status indicator
    await page.waitForTimeout(1500);

    // Find and click the warning indicator
    const statusButton = page.locator('header button[class*="amber"]');
    await expect(statusButton).toBeVisible({ timeout: 5000 });
    await statusButton.click();

    // Assert: Error panel shows error information
    const statusPanel = page.locator('[role="dialog"]').filter({ hasText: '搜索状态' });
    await expect(statusPanel).toBeVisible();

    // Should show error message
    await expect(statusPanel.locator('text=连接被拒绝')).toBeVisible();
  });

  test('SEV-05: should display elapsed time in human-readable format', async ({ page }) => {
    // Mock NDJSON stream with various elapsed times
    await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
      const ndjsonLines = [
        // Success result (need at least one for the search to be considered complete)
        JSON.stringify({
          type: 'result',
          data: {
            path: 'orl://local/var/log/app.log',
            keywords: [{ type: 'literal', text: 'test' }],
            chunks: [{ range: [1, 1], lines: [{ no: 1, text: 'test line' }] }],
            encoding: 'UTF-8'
          }
        }),
        // Finished event with 2.35 seconds
        JSON.stringify({
          type: 'finished',
          total_sources: 1,
          successful_sources: 1,
          failed_sources: 0,
          total_elapsed_ms: 2350
        })
      ];

      await route.fulfill({
        status: 200,
        headers: {
          'Content-Type': 'application/x-ndjson; charset=utf-8',
          'X-Logseek-SID': 'test-sid-time'
        },
        body: ndjsonLines.join('\n')
      });
    });

    // Perform search
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('time test');
    await searchInput.press('Enter');

    // Wait for status indicator
    await page.waitForTimeout(1500);

    // Find and click the success indicator
    const successButton = page.locator('header button[class*="green"]');
    await expect(successButton).toBeVisible({ timeout: 5000 });
    await successButton.click();

    // Assert: Elapsed time is displayed
    const statusPanel = page.locator('[role="dialog"]').filter({ hasText: '搜索状态' });
    await expect(statusPanel).toBeVisible();

    // formatElapsed(2350) returns "2.3s" (toFixed(1) rounds 2.35 to 2.3)
    // The text should contain "耗时" and a time value ending in "s"
    await expect(statusPanel.getByText(/耗时/)).toBeVisible();
    // Check for time format (e.g., "2.3s" or "2.35s")
    await expect(statusPanel.getByText(/\d+\.\d+s/)).toBeVisible();
  });
});
