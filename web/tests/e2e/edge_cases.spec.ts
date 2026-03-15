/**
 * Edge Cases E2E Tests
 *
 * Tests for boundary conditions and unexpected inputs:
 * - Empty search results display
 * - Extremely long query handling
 * - XSS payload escaping
 * - Empty directory display
 */

import { test, expect } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

test.describe('Edge Cases E2E', () => {
  test.describe.configure({ mode: 'serial' });

  test.beforeEach(async ({ page }) => {
    await page.goto('/search');
    await page.waitForLoadState('networkidle');
  });

  test('EDGE-01: should show empty state message for zero results', async ({ page }) => {
    // Mock search API to return empty NDJSON (no results)
    await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
      await route.fulfill({
        status: 200,
        headers: { 'Content-Type': 'application/x-ndjson' },
        body: ''
      });
    });

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('NONEXISTENT_KEYWORD_XYZ123');
    await searchInput.press('Enter');

    // Wait for search to complete with 0 results
    await page.waitForFunction(
      () => {
        const el = document.querySelector('.text-lg.font-semibold');
        const text = el?.textContent || '';
        return /0\s*个结果/.test(text);
      },
      { timeout: 10000 }
    );

    // Assert: empty state h3 is visible
    await expect(page.locator('h3', { hasText: '您的搜索没有匹配到任何日志' })).toBeVisible();
  });

  test('EDGE-02: should not crash on extremely long query (10000 chars)', async ({ page }) => {
    const longQuery = 'a'.repeat(10000);

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(longQuery);
    await searchInput.press('Enter');

    // Wait briefly for any response or crash
    await page.waitForTimeout(5000);

    // Assert: page is still responsive (search input visible, body rendered)
    await expect(searchInput).toBeVisible();
    await expect(page.locator('body')).toBeVisible();
  });

  test('EDGE-03: should escape XSS payloads in search results', async ({ page }) => {
    // Mock search API to return NDJSON with XSS payload in result content
    // NDJSON format requires { type: 'result', data: { path, keywords, chunks } }
    const xssLine = 'Found: <script>alert("XSS")</script>';
    const resultEvent = JSON.stringify({
      type: 'result',
      data: {
        path: 'orl://local/test.log',
        keywords: [{ type: 'literal', text: 'script' }],
        chunks: [
          {
            range: [1, 1],
            lines: [{ no: 1, text: xssLine }]
          }
        ],
        encoding: 'UTF-8'
      }
    });

    await page.route('**/api/v1/logseek/search.ndjson', async (route) => {
      await route.fulfill({
        status: 200,
        headers: { 'Content-Type': 'application/x-ndjson' },
        body: resultEvent + '\n'
      });
    });

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill('<script>alert("XSS")</script>');
    await searchInput.press('Enter');

    // Wait for result card to appear
    await page.waitForFunction(
      () => {
        const cards = document.querySelectorAll('[data-result-card]');
        return cards.length > 0;
      },
      { timeout: 10000 }
    );

    // The highlight function escapes HTML: < → &lt;, > → &gt;
    // The keyword "script" is wrapped in <mark> tags, so the innerHTML is:
    //   &lt;<mark class="highlight">script</mark>&gt;
    // Verify: escaped entities appear (&lt; for < and &gt; for >)
    const cardHtml = await page.locator('[data-result-card]').first().innerHTML();
    expect(cardHtml).toContain('&lt;');
    expect(cardHtml).toContain('&gt;');
    // Verify: no executable <script> tag exists in the DOM
    const scriptTags = await page.locator('[data-result-card] script').count();
    expect(scriptTags).toBe(0);
  });

  test('EDGE-04: should show empty directory message', async ({ page }) => {
    const RUN_ID = Date.now();
    const EMPTY_DIR = path.join(__dirname, `temp_empty_${RUN_ID}`);

    // Create empty directory
    fs.mkdirSync(EMPTY_DIR, { recursive: true });

    // Cleanup in case test fails
    const cleanup = () => {
      try {
        if (fs.existsSync(EMPTY_DIR)) {
          fs.rmSync(EMPTY_DIR, { recursive: true, force: true });
        }
      } catch {
        // ignore
      }
    };

    try {
      // Navigate to explorer with the empty directory
      await page.goto(`/explorer?orl=${encodeURIComponent(`orl://local${EMPTY_DIR}`)}`);
      await page.waitForLoadState('networkidle');

      // Assert: empty directory message is visible
      await expect(page.getByText('此目录为空。')).toBeVisible({ timeout: 5000 });
    } finally {
      cleanup();
    }
  });
});
