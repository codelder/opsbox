import { test, expect, toLocalOrl, toLocalOrlForScript } from './fixtures';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { execSync } from 'child_process';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Set debug logging for Rust components
process.env.RUST_LOG = 'debug';

test.describe('Local Gzip Archive E2E', () => {
  test.describe.configure({ mode: 'serial' });

  const RUN_ID = Date.now();
  const TEST_ROOT_DIR = path.join(__dirname, `temp_gz_${RUN_ID}`);
  const ORIGINAL_LOG_CONTENT =
    '2024-01-01 12:00:00 [INFO] Application started\n' +
    '2024-01-01 12:01:00 [DEBUG] Processing request\n' +
    '2024-01-01 12:02:00 [INFO] Request completed\n';

  test.beforeAll(async () => {
    test.setTimeout(60000);
    // Create test directory
    fs.mkdirSync(TEST_ROOT_DIR, { recursive: true });

    // Create a test log file
    const testLogFile = path.join(TEST_ROOT_DIR, 'app_tranTime.log');
    fs.writeFileSync(testLogFile, ORIGINAL_LOG_CONTENT);

    // Create a gzip compressed version using system gzip command
    const gzFile = path.join(TEST_ROOT_DIR, 'app_tranTime.log.gz');
    execSync(`gzip -c "${testLogFile}" > "${gzFile}"`);

    // Verify the gzip file was created
    expect(fs.existsSync(gzFile)).toBe(true);
    console.log(`Created test gzip file: ${gzFile}`);
  });

  test.afterAll(async () => {
    // Clean up
    fs.rmSync(TEST_ROOT_DIR, { recursive: true, force: true });
  });

  test('should list and navigate into local gz file', async ({ page }) => {
    // Navigate to the directory containing the gz file
    const orl = toLocalOrl(TEST_ROOT_DIR);
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(orl)}`);
    await page.waitForLoadState('networkidle');

    // Capture console for debugging
    const consoleLogs: string[] = [];
    page.on('console', (msg) => {
      consoleLogs.push(`[${msg.type()}] ${msg.text()}`);
    });

    // Capture API requests/responses
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const apiCalls: { url: string; status: number; body?: any }[] = [];
    page.on('response', async (response) => {
      if (response.url().includes('/api/v1/explorer/')) {
        const url = response.url();
        const status = response.status();
        let body;
        try {
          body = await response.json();
        } catch {
          const text = await response.text();
          body = { text };
        }
        apiCalls.push({ url, status, body });
      }
    });

    // Verify we can see the gz file
    await expect(page.getByText('app_tranTime.log.gz')).toBeVisible({ timeout: 5000 });
    console.log('✓ GZ file is visible in listing');

    // Double-click the gz file to "enter" it
    await page.getByText('app_tranTime.log.gz').dblclick();
    await page.waitForLoadState('networkidle');

    // Debug: print API calls
    console.log('API calls after double-click:');
    for (const call of apiCalls) {
      console.log(`  ${call.url} -> ${call.status}`);
      if (call.body) {
        if (call.body.text) {
          console.log(`    Body: ${call.body.text}`);
        } else if (call.body.data) {
          console.log(`    Data: ${JSON.stringify(call.body.data).slice(0, 200)}...`);
        }
      }
    }

    // Debug: print console logs
    console.log('Console logs:');
    for (const log of consoleLogs) {
      console.log(`  ${log}`);
    }

    // After double-clicking a .gz file, we should see a virtual file entry
    // The file name is file_stem of the gz file (app_tranTime.log.gz -> app_tranTime.log)
    // This is what ArchiveFileSystem returns for Gz type

    // Check if there's an error message
    const hasError = await page
      .locator('body')
      .textContent()
      .then((text) => {
        return /错误|Error|Failed/i.test(text || '');
      });

    if (hasError) {
      const errorText = await page.locator('body').textContent();
      console.error(`Page shows error: ${errorText}`);
    }

    // We should either see the virtual file entry OR an error (which helps debugging)
    const bodyText = (await page.locator('body').textContent()) || '';
    console.log(`Page content after double-click: ${bodyText.substring(0, 500)}...`);

    // If successful, we should see the virtual file
    // If failed, the test will show what went wrong in the logs
    // Use more specific selector to avoid matching app_tranTime.log.gz
    const internalFileButton = page.locator('button[title="app_tranTime.log"]');
    await expect(internalFileButton).toBeVisible({ timeout: 5000 });
  });

  test('should return proper API response for gz file list', async ({ request }) => {
    // Direct API test - bypass UI
    const orl = toLocalOrl(`${TEST_ROOT_DIR}/app_tranTime.log.gz`);
    const maxAttempts = 3;
    let lastStatus = 0;
    let lastBodyText = '';
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    let body: any = null;

    // Retry a few times to reduce transient backend flakiness under heavy parallel E2E load.
    for (let attempt = 1; attempt <= maxAttempts; attempt++) {
      const response = await request.post(`/api/v1/explorer/list`, {
        data: { orl }
      });

      lastStatus = response.status();
      lastBodyText = await response.text();
      console.log(`API attempt ${attempt}/${maxAttempts} status: ${lastStatus}`);
      console.log(`API attempt ${attempt} body: ${lastBodyText}`);

      if (lastStatus === 200) {
        try {
          body = JSON.parse(lastBodyText);
          break;
        } catch {
          // keep retrying for transient non-JSON responses
        }
      }

      if (attempt < maxAttempts) {
        await new Promise((resolve) => setTimeout(resolve, 300));
      }
    }

    // Should return 200 OK with valid JSON body
    expect(lastStatus, `Expected 200 from /api/v1/explorer/list, got ${lastStatus}. Last body: ${lastBodyText}`).toBe(
      200
    );
    expect(body).toBeTruthy();
    console.log(`API Response body:`, JSON.stringify(body, null, 2));

    // Should have data with items
    expect(body).toHaveProperty('success', true);
    expect(body).toHaveProperty('data');
    expect(body.data).toHaveProperty('items');

    // For a gz file, should return one virtual item (the decompressed file)
    const items = body.data.items;
    console.log(`Items returned:`, JSON.stringify(items, null, 2));

    expect(items.length).toBeGreaterThan(0);
    // For gz files, name is file_stem of original file (app_tranTime.log.gz -> app_tranTime.log)
    expect(items[0]).toHaveProperty('name', 'app_tranTime.log');
    expect(items[0]).toHaveProperty('path'); // Should be a valid ORL with entry param

    // The path should contain the entry query parameter (?entry=)
    const itemPath = items[0].path;
    console.log(`Item path: ${itemPath}`);
    expect(itemPath).toContain('entry=');
    expect(itemPath).toContain('app_tranTime');
  });

  test('should open internal file when double-clicked', async ({ page, context }) => {
    // Navigate to the directory containing the gz file
    const orl = toLocalOrl(TEST_ROOT_DIR);
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(orl)}`);
    await page.waitForLoadState('networkidle');

    // Double-click the gz file to enter it
    await page.getByText('app_tranTime.log.gz').dblclick();
    await page.waitForLoadState('networkidle');

    // Should see the internal file (use more specific selector to avoid matching outer container)
    const internalFileButton = page.locator('button[title="app_tranTime.log"]');
    await expect(internalFileButton).toBeVisible({ timeout: 5000 });

    // Set up to capture the new page that will be opened
    const newPagePromise = context.waitForEvent('page');

    // Double-click the internal file (the .log button, not the .gz file)
    await internalFileButton.dblclick();

    // Wait for the new page to open
    const newPage = await newPagePromise;
    await newPage.waitForLoadState('networkidle');

    // Check the new page URL
    const newPageUrl = newPage.url();

    // Should have navigated to /view page in the new tab
    expect(newPageUrl).toContain('/view');
    expect(newPageUrl).toContain('file=');
  });

  test('should correctly load file content from gz archive', async ({ page, context }) => {
    // Navigate to the directory containing the gz file
    const orl = toLocalOrl(TEST_ROOT_DIR);
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(orl)}`);
    await page.waitForLoadState('networkidle');

    // Double-click the gz file to enter it
    await page.getByText('app_tranTime.log.gz').dblclick();
    await page.waitForLoadState('networkidle');

    // Should see the internal file (use more specific selector to avoid matching outer container)
    const internalFileButton = page.locator('button[title="app_tranTime.log"]');
    await expect(internalFileButton).toBeVisible({ timeout: 5000 });

    // Set up to capture the new page that will be opened
    const newPagePromise = context.waitForEvent('page');

    // Double-click the internal file
    await internalFileButton.dblclick();

    // Wait for the new page to open and load
    const newPage = await newPagePromise;

    // Wait for the page content to load
    await newPage.waitForLoadState('domcontentloaded');
    await newPage.waitForTimeout(3000); // Give time for content to load

    // Get the view page URL
    const viewPageUrl = newPage.url();
    console.log(`View page URL: ${viewPageUrl}`);

    // Extract the file parameter from URL
    const fileParamMatch = viewPageUrl.match(/[?&]file=([^&]+)/);
    expect(fileParamMatch).toBeTruthy();

    const fileParam = fileParamMatch![1];
    console.log(`File parameter: ${fileParam}`);

    // Verify correct encoding (no double encoding)
    // Should have %3Fentry%3D%2F (single encoding)
    // Should NOT have %3Fentry%3D%252F (double encoding)
    expect(fileParam).toContain('%3Fentry%3D%2F');
    expect(fileParam).not.toContain('%3Fentry%3D%252F');

    // Verify the page shows the same content as the original uncompressed file.
    const pageText = (await newPage.locator('body').textContent()) || '';
    console.log(`Page content preview: ${pageText.substring(0, 500)}`);
    const originalLines = ORIGINAL_LOG_CONTENT.trim().split('\n');

    expect(pageText).toBeDefined();
    for (const line of originalLines) {
      await expect(newPage.locator('body')).toContainText(line);
    }

    // Check that we don't have the double-encoding error message
    expect(pageText).not.toContain('未找到条目或流为空');
    expect(pageText).not.toContain('Failed to load file');
    expect(pageText).not.toContain('404');
  });
});
