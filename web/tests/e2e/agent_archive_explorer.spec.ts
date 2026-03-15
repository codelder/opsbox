/**
 * Agent Archive Explorer E2E Tests
 *
 * Tests for browsing and downloading archive files on Agent endpoints.
 * This covers the feature added in the Explorer-LogSeek architecture alignment:
 * - Agent now uses ResourceLister from explorer library
 * - Supports tar, tar.gz, tgz, zip, gz archive formats
 * - Supports entry parameter for navigating inside archives
 */

import { test, expect, type Page } from '@playwright/test';
import { spawn, type ChildProcessWithoutNullStreams, execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import {
  getFreePort,
  findAgentCommand,
  stopProcess,
  waitForAgentReady,
  writeTarFile,
  writeTarGzFile,
  DEFAULT_AGENT_READY_TIMEOUT
} from './utils/agent';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Set debug logging for Rust components
process.env.RUST_LOG = 'debug';

test.describe('Agent Archive Explorer E2E', () => {
  test.describe.configure({ mode: 'serial' });

  const RUN_ID = Date.now();
  const AGENT_ID = `e2e-archive-agent-${RUN_ID}`;

  const repoRoot = path.resolve(__dirname, '../../..');
  const TEST_ROOT_DIR = path.join(__dirname, `temp_agent_archive_${RUN_ID}`);
  const TEST_LOGS_DIR = path.join(TEST_ROOT_DIR, 'logs');
  const TEST_AGENT_LOG_DIR = path.join(TEST_ROOT_DIR, 'agent_logs');

  const MARKER_TAR = `E2E_ARCHIVE_TAR_${RUN_ID}`;
  const MARKER_TARGZ = `E2E_ARCHIVE_TARGZ_${RUN_ID}`;
  const TAR_ROOT_LOG_CONTENT = `2025-01-01 10:00:00 [INFO] root log ${MARKER_TAR}`;
  const TARGZ_SERVICE_LOG_CONTENT = `2025-01-01 11:01:00 [INFO] service log ${MARKER_TARGZ}`;

  let agentProc: ChildProcessWithoutNullStreams | null = null;
  let agentPort: number | null = null;
  let zipArchiveReady = false;

  /**
   * 导航到Agent日志目录的辅助函数
   * 减少重复的页面导航代码
   */
  async function navigateToAgentLogs(page: Page): Promise<void> {
    await page.goto(
      `http://localhost:5173/explorer?orl=${encodeURIComponent(`orl://${AGENT_ID}@agent${TEST_LOGS_DIR}`)}`
    );
    await page.waitForLoadState('networkidle');
  }

  test.beforeAll(async ({ request }) => {
    test.setTimeout(120000);

    // Create test directories
    fs.mkdirSync(TEST_LOGS_DIR, { recursive: true });
    fs.mkdirSync(TEST_AGENT_LOG_DIR, { recursive: true });

    // Create tar archive with nested directory
    writeTarFile(path.join(TEST_LOGS_DIR, 'test.tar'), [
      { name: 'root.log', content: `${TAR_ROOT_LOG_CONTENT}\n` },
      { name: 'subdir/nested.log', content: `2025-01-01 10:01:00 [INFO] nested log ${MARKER_TAR}\n` }
    ]);

    // Create tar.gz archive with nested directory
    writeTarGzFile(path.join(TEST_LOGS_DIR, 'test.tar.gz'), [
      { name: 'app.log', content: `2025-01-01 11:00:00 [INFO] app log ${MARKER_TARGZ}\n` },
      { name: 'internal/service.log', content: `${TARGZ_SERVICE_LOG_CONTENT}\n` }
    ]);

    // Create tgz archive (same format as tar.gz)
    writeTarGzFile(path.join(TEST_LOGS_DIR, 'test.tgz'), [
      { name: 'data.log', content: `2025-01-01 12:00:00 [INFO] data log TGZ\n` }
    ]);

    // Create zip archive using system zip command (if available)
    const zipPath = path.join(TEST_LOGS_DIR, 'test.zip');
    try {
      const zipSourceDir = path.join(TEST_ROOT_DIR, 'zip_source');
      fs.mkdirSync(path.join(zipSourceDir, 'config'), { recursive: true });
      fs.writeFileSync(path.join(zipSourceDir, 'info.log'), `2025-01-01 13:00:00 [INFO] info log ZIP\n`);
      fs.writeFileSync(path.join(zipSourceDir, 'config/settings.log'), `2025-01-01 13:01:00 [INFO] settings log ZIP\n`);
      execSync(`cd "${zipSourceDir}" && zip -r "${zipPath}" .`);
      zipArchiveReady = fs.existsSync(zipPath) && fs.statSync(zipPath).size > 0;
      if (!zipArchiveReady) {
        console.log('Skipping zip archive test - zip file is empty or missing');
      }
    } catch (e) {
      zipArchiveReady = false;
      if (fs.existsSync(zipPath)) {
        fs.rmSync(zipPath, { force: true });
      }
      console.log('Skipping zip archive creation (zip command not available or failed):', e);
    }

    // Start agent
    agentPort = await getFreePort();
    console.log(`Starting archive test agent on port ${agentPort} with ID ${AGENT_ID}`);
    const { command, argsPrefix, cwd } = findAgentCommand(repoRoot);

    const args = [
      ...argsPrefix,
      '--agent-id',
      AGENT_ID,
      '--agent-name',
      'E2E Archive Agent',
      '--server-endpoint',
      'http://127.0.0.1:4001',
      '--search-roots',
      TEST_LOGS_DIR,
      '--listen-port',
      String(agentPort),
      '--no-heartbeat',
      '--log-dir',
      TEST_AGENT_LOG_DIR,
      '--log-retention',
      '1'
    ];

    agentProc = spawn(command, args, {
      cwd,
      env: { ...process.env, RUST_LOG: process.env.RUST_LOG ?? 'info' },
      stdio: 'pipe'
    });
    agentProc.stdout.on('data', (d) => process.stdout.write(d));
    agentProc.stderr.on('data', (d) => process.stderr.write(d));

    // Wait for agent to be ready (use increased timeout for parallel test runs)
    await waitForAgentReady(request, AGENT_ID, DEFAULT_AGENT_READY_TIMEOUT);
  });

  test.afterAll(async ({ request }) => {
    try {
      await request.delete(`http://127.0.0.1:4001/api/v1/agents/${AGENT_ID}`);
    } catch {
      // ignore
    }
    if (agentProc) {
      await stopProcess(agentProc);
    }
    fs.rmSync(TEST_ROOT_DIR, { recursive: true, force: true });
  });

  test('should list all archive formats, navigate subdirectories, and download files on agent', async ({ page }) => {
    // Navigate to agent logs directory
    await navigateToAgentLogs(page);

    // Verify all archive files are visible
    await expect(page.getByRole('button', { name: 'test.tar', exact: true })).toBeVisible({ timeout: 10000 });
    await expect(page.getByRole('button', { name: 'test.tar.gz', exact: true })).toBeVisible({ timeout: 10000 });
    await expect(page.getByRole('button', { name: 'test.tgz', exact: true })).toBeVisible({ timeout: 10000 });

    // Test tar archive and navigate into subdirectory
    await page.getByRole('button', { name: 'test.tar', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');
    await expect(page.locator('body')).not.toContainText(/500|Internal Server Error|错误/i);
    await expect(page.getByText('root.log')).toBeVisible({ timeout: 5000 });
    await expect(page.getByText('subdir')).toBeVisible();

    // Test download file from tar archive
    await page.getByRole('button', { name: 'root.log', exact: true }).click({ button: 'right' });
    const downloadOption = page.getByRole('menuitem', { name: /download|下载/i });

    if (await downloadOption.isVisible({ timeout: 2000 }).catch(() => false)) {
      const downloadPromise = page.waitForEvent('download', { timeout: 10000 });
      await downloadOption.click();
      const download = await downloadPromise;
      expect(download.suggestedFilename()).toContain('root.log');

      // Verify downloaded file content
      const downloadPath = await download.path();
      if (downloadPath) {
        const downloadedContent = fs.readFileSync(downloadPath, 'utf-8');
        expect(downloadedContent).toContain(TAR_ROOT_LOG_CONTENT);
      }
    } else {
      await page.keyboard.press('Escape');
      const [viewPage] = await Promise.all([
        page.waitForEvent('popup'),
        page.getByRole('button', { name: 'root.log', exact: true }).dblclick()
      ]);
      await viewPage.waitForURL(/\/view\?/);
      await expect(viewPage.getByRole('heading', { name: 'root.log' })).toBeVisible({ timeout: 10000 });
      await expect(viewPage.locator('.code-content')).toContainText(TAR_ROOT_LOG_CONTENT, { timeout: 10000 });
      await expect(viewPage.locator('body')).not.toContainText('暂无内容');
      await viewPage.close();
    }

    // Navigate into subdir
    await page.getByRole('button', { name: 'subdir', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');
    await expect(page.getByText('nested.log')).toBeVisible({ timeout: 5000 });

    // Navigate back up to archive root
    const upButton = page.locator('button:has(svg.lucide-arrow-left)');
    await upButton.click();
    await page.waitForLoadState('networkidle');
    await expect(page.getByText('root.log')).toBeVisible({ timeout: 5000 });
    await expect(page.getByText('subdir')).toBeVisible();

    // Go back to logs directory
    await upButton.click();
    await page.waitForLoadState('networkidle');

    // Test tar.gz archive
    await page.getByRole('button', { name: 'test.tar.gz', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');
    await expect(page.locator('body')).not.toContainText(/500|Internal Server Error|错误/i);
    await expect(page.getByText('app.log')).toBeVisible({ timeout: 5000 });
    await expect(page.getByText('internal')).toBeVisible();

    // Go back
    await upButton.click();
    await page.waitForLoadState('networkidle');

    // Test tgz archive
    await page.getByRole('button', { name: 'test.tgz', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');
    await expect(page.locator('body')).not.toContainText(/500|Internal Server Error|错误/i);
    await expect(page.getByText('data.log')).toBeVisible({ timeout: 5000 });

    // Test zip archive (if available)
    if (zipArchiveReady) {
      // Go back
      await upButton.click();
      await page.waitForLoadState('networkidle');

      await expect(page.getByRole('button', { name: 'test.zip', exact: true })).toBeVisible({ timeout: 10000 });
      await page.getByRole('button', { name: 'test.zip', exact: true }).dblclick();
      await page.waitForLoadState('networkidle');
      await expect(page.locator('body')).not.toContainText(/500|Internal Server Error|错误/i);
    }
  });

  test('should handle archive entry URL parameters and navigation', async ({ page }) => {
    // Test 1: Navigate directly to internal directory in tar.gz archive via URL
    const archiveOrl = `orl://${AGENT_ID}@agent${TEST_LOGS_DIR}/test.tar.gz?entry=/internal`;
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(archiveOrl)}`);
    await page.waitForLoadState('networkidle');

    // Should see service.log
    await expect(page.getByText('service.log')).toBeVisible({ timeout: 5000 });

    // Double-click to view (use exact match for file button)
    const [viewPage] = await Promise.all([
      page.waitForEvent('popup'),
      page.getByRole('button', { name: 'service.log', exact: true }).dblclick()
    ]);
    await viewPage.waitForURL(/\/view\?/);
    await expect(viewPage.getByRole('heading', { name: 'service.log' })).toBeVisible({ timeout: 10000 });
    await expect(viewPage.locator('.code-content')).toContainText(TARGZ_SERVICE_LOG_CONTENT, { timeout: 10000 });
    await expect(viewPage.locator('body')).not.toContainText('暂无内容');
    await viewPage.close();

    // Test 2: Navigate to nested file in archive - view page requires sid parameter
    const archiveOrl2 = `orl://${AGENT_ID}@agent${TEST_LOGS_DIR}/test.tar.gz?entry=/internal/service.log`;
    const testSid = 'test-archive-view-sid';
    await page.goto(`http://localhost:5173/view?sid=${testSid}&file=${encodeURIComponent(archiveOrl2)}`);
    await page.waitForLoadState('networkidle');

    // Verify page loads without the "缺少 sid 参数" error
    await expect(page.locator('body')).not.toContainText('缺少 sid 参数');
    await expect(page.getByRole('heading', { name: 'service.log' })).toBeVisible({ timeout: 10000 });
    await expect(page.locator('.code-content')).toContainText(TARGZ_SERVICE_LOG_CONTENT, { timeout: 10000 });
    await expect(page.locator('body')).not.toContainText('暂无内容');

    // Test 3: Navigate up from archive entry to archive root
    const archiveOrl3 = `orl://${AGENT_ID}@agent${TEST_LOGS_DIR}/test.tar?entry=/subdir`;
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(archiveOrl3)}`);
    await page.waitForLoadState('networkidle');

    // Should see nested.log
    await expect(page.getByText('nested.log')).toBeVisible({ timeout: 5000 });

    // Navigate up
    const upButton = page.locator('button:has(svg.lucide-arrow-left)');
    await upButton.click();
    await page.waitForLoadState('networkidle');

    // Should be at archive root (see root.log and subdir)
    await expect(page.getByText('root.log')).toBeVisible({ timeout: 5000 });

    // Test 4: Navigate up from archive root to parent directory
    const archiveOrl4 = `orl://${AGENT_ID}@agent${TEST_LOGS_DIR}/test.tar?entry=/`;
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(archiveOrl4)}`);
    await page.waitForLoadState('networkidle');

    // Should see archive contents
    await expect(page.getByRole('button', { name: 'root.log', exact: true })).toBeVisible({ timeout: 5000 });

    // Navigate up (should exit archive)
    await upButton.click();
    await page.waitForLoadState('networkidle');

    // After clicking back from archive root, the entry parameter is removed
    // but we're still viewing the archive (showing root.log, subdir)
    // Click back again to go to parent directory
    await upButton.click();
    await page.waitForLoadState('networkidle');

    // Now should be at the logs directory (parent of test.tar)
    // Check that test.tar button is visible in the directory listing
    await expect(page.getByRole('button', { name: 'test.tar', exact: true })).toBeVisible({ timeout: 5000 });

    // URL should not have entry parameter
    const url = new URL(page.url());
    const orlParam = url.searchParams.get('orl') || '';
    expect(orlParam).not.toContain('entry=');
  });
});
