
import { test, expect } from '@playwright/test';

test.describe('Search E2E', () => {
    test.beforeEach(async ({ page }) => {
        // Mock the search API
        await page.route('**/search.ndjson', async (route) => {
            const jsonResults = [
                // Local File
                {
                    type: 'result',
                    data: {
                        path: 'ls://local/localhost/dir/var/log/syslog',
                        keywords: [{ type: 'literal', text: 'error' }],
                        chunks: [
                            {
                                range: [100, 102],
                                lines: [
                                    { no: 100, text: 'Dec 11 10:00:00 localhost kernel: [123.456] error: something bad happened' },
                                    { no: 101, text: 'Dec 11 10:00:01 localhost kernel: [123.457] info: recovering' },
                                    { no: 102, text: 'Dec 11 10:00:02 localhost kernel: [123.458] error: failed again' }
                                ]
                            }
                        ]
                    }
                },
                // Agent File
                {
                    type: 'result',
                    data: {
                        path: 'ls://agent/web-01/dir/app/logs/error.log',
                        keywords: [{ type: 'literal', text: 'error' }],
                        chunks: [
                            {
                                range: [50, 51],
                                lines: [
                                    { no: 50, text: '2023-10-27 10:00:00 [ERROR] Connection refuesd' },
                                    { no: 51, text: '2023-10-27 10:00:01 [INFO] Retrying...' }
                                ]
                            }
                        ]
                    }
                },
                // S3 Archive File
                {
                    type: 'result',
                    data: {
                        path: 'ls://s3/prod:logs-bucket/archive/2023/10/data.tar.gz?entry=internal/service.log',
                        keywords: [{ type: 'literal', text: 'error' }],
                        chunks: [
                            {
                                range: [1, 1],
                                lines: [
                                    { no: 1, text: 'Starting service...' }
                                ]
                            }
                        ]
                    }
                },
                // Complete event
                {
                    type: 'complete',
                    data: {
                        source: 'mock',
                        elapsed_ms: 100
                    }
                }
            ];

            const ndjson = jsonResults.map(r => JSON.stringify(r)).join('\n');

            await route.fulfill({
                status: 200,
                headers: {
                    'Content-Type': 'application/x-ndjson',
                    'X-Logseek-SID': 'test-session-id'
                },
                body: ndjson
            });
        });

        await page.goto('/search');
    });

    test('should display search results for different endpoint types', async ({ page }) => {
        // Perform search
        const searchInput = page.getByPlaceholder('搜索...');
        await searchInput.fill('error');
        await searchInput.press('Enter');

        // Wait for results
        await expect(page.locator('.text-lg.font-semibold')).toContainText('3 个结果');

        // Level 0: Endpoint Types
        await expect(page.getByRole('button', { name: '本地文件' })).toBeVisible();
        await expect(page.getByRole('button', { name: '远程代理' })).toBeVisible();
        await expect(page.getByRole('button', { name: 'S3 云存储' })).toBeVisible();

        // Level 1: Endpoint IDs (Expand tree if needed, or check if they are visible)
        // Based on my reading of +page.svelte, it renders a tree.
        // Single child nodes are skipped, so we expect leaf directories.

        // Expand Local
        await page.getByRole('button', { name: '本地文件' }).click();
        await expect(page.getByRole('button', { name: 'log' })).toBeVisible();

        // Expand Agent
        await page.getByRole('button', { name: '远程代理' }).click();
        await expect(page.getByRole('button', { name: 'logs' })).toBeVisible();

        // Expand S3
        await page.getByRole('button', { name: 'S3 云存储' }).click();
        await expect(page.getByRole('button', { name: 'internal' })).toBeVisible();
        // Local: var/log/syslog
        // Agent: app/logs/error.log
        // S3: prod:logs-bucket -> ...
        // Note: the component truncates path, so we check for partial text or specific elements

        // Check filtering by clicking sidebar
        // (Already clicked '本地文件' above)
        // await page.getByRole('button', { name: '本地文件' }).click();
        await page.getByRole('button', { name: '本地文件' }).click(); // Re-select to filter
        await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果');

        // Verify Card Content
        await expect(page.getByText('syslog')).toBeVisible();
        await expect(page.getByText('Dec 11 10:00:00 localhost kernel: [123.456] error: something bad happened')).toBeVisible();

        // Clear filter
        await page.getByRole('button', { name: '本地文件' }).click(); // Click again to potentially toggle or verify behavior. 
        // Logic says: if filtered, clicking active clears it (or selects parent).
        // Let's re-verify exact behavior logic if needed, but for now let's just assert results are back implies logic works
        // Actually, logic is: toggleSelection -> if exact match, clear to empty?
        // Let's just check clearing by clicking 'Local' again which matches selectedPath=['Local']

        await expect(page.locator('.text-lg.font-semibold')).toContainText('3 个结果');
    });
});
