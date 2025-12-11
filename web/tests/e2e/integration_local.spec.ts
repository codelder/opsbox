
import { test, expect } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

test.describe('Local Integration E2E', () => {
    const TEST_APP = 'e2e_test';
    const TEST_LOG_DIR = path.join(__dirname, 'temp_logs');
    const TEST_LOG_FILE = path.join(TEST_LOG_DIR, 'e2e.log');
    const API_BASE = 'http://127.0.0.1:4001/api/v1/logseek';
    const UNI_ID = `E2E-${Date.now()}`;

    test.beforeAll(async ({ request }) => {
        // 1. Create temp log directory and file
        if (!fs.existsSync(TEST_LOG_DIR)) {
            fs.mkdirSync(TEST_LOG_DIR, { recursive: true });
        }

        fs.writeFileSync(TEST_LOG_FILE, `2025-01-01 12:00:00 [INFO] Test log entry ${UNI_ID}\n`);

        // 2. Prepare Planner Script
        // Must use absolute path for endpoint.root
        const absRoot = path.resolve(TEST_LOG_DIR);

        // Python-like Starlark script
        const script = `
SOURCES = [{
    'endpoint': { 'kind': 'local', 'root': '${absRoot}' },
    'target':   { 'type': 'dir', 'path': '.', 'recursive': True },
    'filter_glob': '*.log',
    'display_name': 'E2E Test Logs'
}]
`;

        // 3. Upload Script via API
        const response = await request.post(`${API_BASE}/settings/planners/scripts`, {
            data: {
                app: TEST_APP,
                script: script
            }
        });

        expect(response.ok()).toBeTruthy();
    });

    test.afterAll(async ({ request }) => {
        // Cleanup: Delete Script
        await request.delete(`${API_BASE}/settings/planners/scripts/${TEST_APP}`);

        // Cleanup: Delete Files
        if (fs.existsSync(TEST_LOG_FILE)) {
            fs.unlinkSync(TEST_LOG_FILE);
        }
        if (fs.existsSync(TEST_LOG_DIR)) {
            fs.rmdirSync(TEST_LOG_DIR);
        }
    });

    test('should search real local files using app: directive', async ({ page }) => {
        await page.goto('/search');

        // Type search query with app directive
        const searchInput = page.getByPlaceholder('搜索...');
        await searchInput.fill(`app:${TEST_APP} ${UNI_ID}`);
        await searchInput.press('Enter');

        // Wait for results
        // Since we are using a real backend, it might take a moment
        await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果', { timeout: 10000 });

        // Verify Result Card Content (Primary Goal)
        await expect(page.getByText(UNI_ID)).toBeVisible();

        // Verify Sidebar
        // "Local" endpoint type should be visible
        await expect(page.getByRole('button', { name: '本地文件' })).toBeVisible();

        // Check for file in sidebar (it might be auto-expanded or not)
        // Try to find it by text first
        await expect(page.getByText('e2e.log')).toBeVisible();
    });
});
