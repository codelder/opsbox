import { test, expect } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

test.describe('Relative Glob Filtering E2E', () => {
  test.describe.configure({ mode: 'serial' });

  const RUN_ID = Date.now();
  const TEST_LOG_DIR = path.join(__dirname, `temp_relative_glob_${RUN_ID}`);
  const API_BASE = 'http://127.0.0.1:4001/api/v1/logseek';
  const TEST_APP_RELATIVE = `e2e_glob_rel_${RUN_ID}`;
  const TEST_APP_RECURSIVE = `e2e_glob_rec_${RUN_ID}`;
  const UNI_ID = `E2E_GLOB_${RUN_ID}`;

  // Test files
  const FILES = {
    'root.log': `2025-01-01 [INFO] Root log ${UNI_ID}`,
    'sub/target.log': `2025-01-01 [INFO] Target log ${UNI_ID}`,
    'deep/nested/deep.log': `2025-01-01 [INFO] Deep log ${UNI_ID}`
  };

  test.beforeAll(async ({ request }) => {
    // 1. Create directory structure
    if (!fs.existsSync(TEST_LOG_DIR)) {
      fs.mkdirSync(TEST_LOG_DIR, { recursive: true });
    }

    // Create subdirectories
    fs.mkdirSync(path.join(TEST_LOG_DIR, 'sub'), { recursive: true });
    fs.mkdirSync(path.join(TEST_LOG_DIR, 'deep/nested'), { recursive: true });

    // Write files
    for (const [relPath, content] of Object.entries(FILES)) {
      fs.writeFileSync(path.join(TEST_LOG_DIR, relPath), content);
    }

    // 2. Prepare Planner Scripts
    const absRoot = path.resolve(TEST_LOG_DIR);

    // Script A: filter_glob = '*/*.log'
    const scriptRelative = `
SOURCES = [{
    'endpoint': { 'kind': 'local', 'root': '${absRoot}' },
    'target':   { 'type': 'dir', 'path': '.', 'recursive': True },
    'filter_glob': '*/*.log',
    'display_name': 'Relative Glob Test'
}]
`;

    // Script B: filter_glob = '**/*.log'
    const scriptRecursive = `
SOURCES = [{
    'endpoint': { 'kind': 'local', 'root': '${absRoot}' },
    'target':   { 'type': 'dir', 'path': '.', 'recursive': True },
    'filter_glob': '**/*.log',
    'display_name': 'Recursive Glob Test'
}]
`;

    const res1 = await request.post(`${API_BASE}/settings/planners/scripts`, {
      data: { app: TEST_APP_RELATIVE, script: scriptRelative }
    });
    expect(res1.ok()).toBeTruthy();

    const res2 = await request.post(`${API_BASE}/settings/planners/scripts`, {
      data: { app: TEST_APP_RECURSIVE, script: scriptRecursive }
    });
    expect(res2.ok()).toBeTruthy();
  });

  test.afterAll(async ({ request }) => {
    try {
      await request.delete(`${API_BASE}/settings/planners/scripts/${TEST_APP_RELATIVE}`);
      await request.delete(`${API_BASE}/settings/planners/scripts/${TEST_APP_RECURSIVE}`);
    } catch {
      // Ignore
    }
    fs.rmSync(TEST_LOG_DIR, { recursive: true, force: true });
  });

  test('should support relative filter_glob (*/*.log) from config', async ({ page }) => {
    await page.goto('/search');

    // Query: app:<app> "UNI_ID"
    // No path: qualifier needed, the source config handles it.
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP_RELATIVE} "${UNI_ID}"`);
    await searchInput.press('Enter');

    // Verify
    // Should only match sub/target.log
    await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果', { timeout: 10000 });

    // Check matched file
    await expect(page.getByRole('link', { name: 'target.log' })).toBeVisible();

    // Ensure others are NOT visible
    await expect(page.getByRole('link', { name: 'root.log' })).not.toBeVisible();
    await expect(page.getByRole('link', { name: 'deep.log' })).not.toBeVisible();
  });

  test('should support recursive filter_glob (**/*.log) from config', async ({ page }) => {
    await page.goto('/search');

    // Query: app:<app> "UNI_ID"
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP_RECURSIVE} "${UNI_ID}"`);
    await searchInput.press('Enter');

    // Verify
    // Should match all 3 files
    await expect(page.locator('.text-lg.font-semibold')).toContainText('3 个结果', { timeout: 10000 });

    await expect(page.getByRole('link', { name: 'root.log' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'target.log' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'deep.log' })).toBeVisible();
  });
});
