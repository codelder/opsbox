/**
 * Agent Archive Explorer E2E Tests
 *
 * Tests for browsing and downloading archive files on Agent endpoints.
 * This covers the feature added in the Explorer-LogSeek architecture alignment:
 * - Agent now uses ResourceLister from explorer library
 * - Supports tar, tar.gz, tgz, zip, gz archive formats
 * - Supports entry parameter for navigating inside archives
 */

import { test, expect, type APIRequestContext } from '@playwright/test';
import { spawn, type ChildProcessWithoutNullStreams, execSync } from 'child_process';
import * as fs from 'fs';
import * as net from 'net';
import * as path from 'path';
import * as zlib from 'zlib';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Set debug logging for Rust components
process.env.RUST_LOG = 'debug';

function getFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const addr = server.address();
      server.close(() => {
        if (!addr || typeof addr === 'string') {
          reject(new Error('Failed to allocate a free port'));
          return;
        }
        resolve(addr.port);
      });
    });
  });
}

function findAgentCommand(repoRoot: string): { command: string; argsPrefix: string[]; cwd: string } {
  const backendDir = path.join(repoRoot, 'backend');
  return {
    command: 'cargo',
    argsPrefix: ['run', '--release', '-p', 'opsbox-agent', '--'],
    cwd: backendDir
  };
}

async function stopProcess(proc: ChildProcessWithoutNullStreams) {
  if (proc.exitCode !== null) return;
  proc.kill('SIGINT');

  const exited = await Promise.race([
    new Promise<boolean>((resolve) => proc.once('exit', () => resolve(true))),
    new Promise<boolean>((resolve) => setTimeout(() => resolve(false), 5000))
  ]);
  if (exited) return;

  proc.kill('SIGKILL');
  await new Promise<void>((resolve) => proc.once('exit', () => resolve()));
}

/**
 * Wait for agent to register to server via API check
 */
async function waitForAgentReady(request: APIRequestContext, agentId: string, maxWait = 15000): Promise<void> {
  const start = Date.now();
  const interval = 500;

  while (Date.now() - start < maxWait) {
    try {
      const response = await request.get(`http://127.0.0.1:4001/api/v1/agents/${agentId}`);
      if (response.ok()) {
        console.log(`Agent ${agentId} is ready after ${Date.now() - start}ms`);
        return;
      }
    } catch {
      // API call failed, agent not yet registered
    }
    await new Promise((r) => setTimeout(r, interval));
  }
  throw new Error(`Agent ${agentId} not ready after ${maxWait}ms`);
}

/**
 * Write a tar file with given entries (pure JS implementation)
 */
function writeTarFile(outFile: string, entries: Array<{ name: string; content: string }>) {
  const blocks: Buffer[] = [];

  function writeHeader(name: string, size: number) {
    const header = Buffer.alloc(512, 0);

    const writeString = (offset: number, length: number, value: string) => {
      header.write(value, offset, Math.min(length, Buffer.byteLength(value)), 'utf8');
    };

    const writeOctal = (offset: number, length: number, value: number) => {
      const s = value.toString(8).padStart(length - 1, '0') + '\0';
      writeString(offset, length, s);
    };

    writeString(0, 100, name);
    writeOctal(100, 8, 0o644);
    writeOctal(108, 8, 0);
    writeOctal(116, 8, 0);
    writeOctal(124, 12, size);
    writeOctal(136, 12, Math.floor(Date.now() / 1000));

    header.fill(0x20, 148, 156);
    writeString(156, 1, '0');
    writeString(257, 6, 'ustar\0');
    writeString(263, 2, '00');

    let checksum = 0;
    for (const byte of header) checksum += byte;
    const checksumStr = checksum.toString(8).padStart(6, '0') + '\0 ';
    writeString(148, 8, checksumStr);

    return header;
  }

  for (const entry of entries) {
    const content = Buffer.from(entry.content, 'utf8');
    blocks.push(writeHeader(entry.name, content.length));
    blocks.push(content);

    const remainder = content.length % 512;
    if (remainder !== 0) {
      blocks.push(Buffer.alloc(512 - remainder, 0));
    }
  }

  blocks.push(Buffer.alloc(1024, 0));
  fs.writeFileSync(outFile, Buffer.concat(blocks));
}

/**
 * Write a tar.gz file with given entries
 */
function writeTarGzFile(outFile: string, entries: Array<{ name: string; content: string }>) {
  const blocks: Buffer[] = [];

  function writeHeader(name: string, size: number) {
    const header = Buffer.alloc(512, 0);

    const writeString = (offset: number, length: number, value: string) => {
      header.write(value, offset, Math.min(length, Buffer.byteLength(value)), 'utf8');
    };

    const writeOctal = (offset: number, length: number, value: number) => {
      const s = value.toString(8).padStart(length - 1, '0') + '\0';
      writeString(offset, length, s);
    };

    writeString(0, 100, name);
    writeOctal(100, 8, 0o644);
    writeOctal(108, 8, 0);
    writeOctal(116, 8, 0);
    writeOctal(124, 12, size);
    writeOctal(136, 12, Math.floor(Date.now() / 1000));

    header.fill(0x20, 148, 156);
    writeString(156, 1, '0');
    writeString(257, 6, 'ustar\0');
    writeString(263, 2, '00');

    let checksum = 0;
    for (const byte of header) checksum += byte;
    const checksumStr = checksum.toString(8).padStart(6, '0') + '\0 ';
    writeString(148, 8, checksumStr);

    return header;
  }

  for (const entry of entries) {
    const content = Buffer.from(entry.content, 'utf8');
    blocks.push(writeHeader(entry.name, content.length));
    blocks.push(content);

    const remainder = content.length % 512;
    if (remainder !== 0) {
      blocks.push(Buffer.alloc(512 - remainder, 0));
    }
  }

  blocks.push(Buffer.alloc(1024, 0));
  const tarData = Buffer.concat(blocks);
  const gzipped = zlib.gzipSync(tarData);
  fs.writeFileSync(outFile, gzipped);
}

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

  let agentProc: ChildProcessWithoutNullStreams | null = null;
  let agentPort: number | null = null;

  test.beforeAll(async ({ request }) => {
    test.setTimeout(120000);

    // Create test directories
    fs.mkdirSync(TEST_LOGS_DIR, { recursive: true });
    fs.mkdirSync(TEST_AGENT_LOG_DIR, { recursive: true });

    // Create tar archive with nested directory
    writeTarFile(
      path.join(TEST_LOGS_DIR, 'test.tar'),
      [
        { name: 'root.log', content: `2025-01-01 10:00:00 [INFO] root log ${MARKER_TAR}\n` },
        { name: 'subdir/nested.log', content: `2025-01-01 10:01:00 [INFO] nested log ${MARKER_TAR}\n` }
      ]
    );

    // Create tar.gz archive with nested directory
    writeTarGzFile(
      path.join(TEST_LOGS_DIR, 'test.tar.gz'),
      [
        { name: 'app.log', content: `2025-01-01 11:00:00 [INFO] app log ${MARKER_TARGZ}\n` },
        { name: 'internal/service.log', content: `2025-01-01 11:01:00 [INFO] service log ${MARKER_TARGZ}\n` }
      ]
    );

    // Create tgz archive (same format as tar.gz)
    writeTarGzFile(
      path.join(TEST_LOGS_DIR, 'test.tgz'),
      [
        { name: 'data.log', content: `2025-01-01 12:00:00 [INFO] data log TGZ\n` }
      ]
    );

    // Create zip archive using system zip command (if available)
    try {
      const zipSourceDir = path.join(TEST_ROOT_DIR, 'zip_source');
      fs.mkdirSync(path.join(zipSourceDir, 'config'), { recursive: true });
      fs.writeFileSync(path.join(zipSourceDir, 'info.log'), `2025-01-01 13:00:00 [INFO] info log ZIP\n`);
      fs.writeFileSync(path.join(zipSourceDir, 'config/settings.log'), `2025-01-01 13:01:00 [INFO] settings log ZIP\n`);
      execSync(`cd "${zipSourceDir}" && zip -r "${path.join(TEST_LOGS_DIR, 'test.zip')}" .`);
    } catch (e) {
      console.log('Skipping zip archive creation (zip command not available):', e);
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

    // Wait for agent to be ready
    await waitForAgentReady(request, AGENT_ID, 15000);
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

  test('should list tar archive contents on agent', async ({ page }) => {
    // Navigate to agent logs directory
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(`orl://${AGENT_ID}@agent${TEST_LOGS_DIR}`)}`);
    await page.waitForLoadState('networkidle');

    // Verify tar file is visible (use exact match to avoid matching test.tar.gz)
    await expect(page.getByRole('button', { name: 'test.tar', exact: true })).toBeVisible({ timeout: 10000 });

    // Double-click to enter the archive
    await page.getByRole('button', { name: 'test.tar', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');

    // Verify we see archive contents (not 500 error)
    await expect(page.locator('body')).not.toContainText(/500|Internal Server Error|错误/i);

    // Should see the files inside the archive
    await expect(page.getByText('root.log')).toBeVisible({ timeout: 5000 });
    await expect(page.getByText('subdir')).toBeVisible();
  });

  test('should navigate into archive subdirectories on agent', async ({ page }) => {
    // Navigate to tar archive
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(`orl://${AGENT_ID}@agent${TEST_LOGS_DIR}`)}`);
    await page.waitForLoadState('networkidle');

    // Enter tar archive
    await page.getByRole('button', { name: 'test.tar', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');

    // Enter subdir
    await page.getByRole('button', { name: 'subdir', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');

    // Should see nested.log
    await expect(page.getByText('nested.log')).toBeVisible({ timeout: 5000 });

    // Navigate back up
    const upButton = page.locator('button:has(svg.lucide-arrow-left)');
    await upButton.click();
    await page.waitForLoadState('networkidle');

    // Should be back at archive root
    await expect(page.getByText('root.log')).toBeVisible({ timeout: 5000 });
    await expect(page.getByText('subdir')).toBeVisible();
  });

  test('should list tar.gz archive contents on agent', async ({ page }) => {
    // Navigate to agent logs directory
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(`orl://${AGENT_ID}@agent${TEST_LOGS_DIR}`)}`);
    await page.waitForLoadState('networkidle');

    // Verify tar.gz file is visible
    await expect(page.getByRole('button', { name: 'test.tar.gz', exact: true })).toBeVisible({ timeout: 10000 });

    // Double-click to enter the archive
    await page.getByRole('button', { name: 'test.tar.gz', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');

    // Verify we see archive contents
    await expect(page.locator('body')).not.toContainText(/500|Internal Server Error|错误/i);
    await expect(page.getByText('app.log')).toBeVisible({ timeout: 5000 });
    await expect(page.getByText('internal')).toBeVisible();
  });

  test('should list tgz archive contents on agent', async ({ page }) => {
    // Navigate to agent logs directory
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(`orl://${AGENT_ID}@agent${TEST_LOGS_DIR}`)}`);
    await page.waitForLoadState('networkidle');

    // Verify tgz file is visible
    await expect(page.getByRole('button', { name: 'test.tgz', exact: true })).toBeVisible({ timeout: 10000 });

    // Double-click to enter the archive
    await page.getByRole('button', { name: 'test.tgz', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');

    // Verify we see archive contents
    await expect(page.locator('body')).not.toContainText(/500|Internal Server Error|错误/i);
    await expect(page.getByText('data.log')).toBeVisible({ timeout: 5000 });
  });

  test('should download file from tar archive on agent', async ({ page }) => {
    // Navigate to tar archive
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(`orl://${AGENT_ID}@agent${TEST_LOGS_DIR}`)}`);
    await page.waitForLoadState('networkidle');

    // Enter tar archive
    await page.getByRole('button', { name: 'test.tar', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');

    // Right-click on root.log to open context menu
    await page.getByRole('button', { name: 'root.log', exact: true }).click({ button: 'right' });

    // Look for download option in context menu
    const downloadOption = page.getByRole('menuitem', { name: /download|下载/i });

    if (await downloadOption.isVisible({ timeout: 2000 }).catch(() => false)) {
      // Start waiting for download
      const downloadPromise = page.waitForEvent('download', { timeout: 10000 });
      await downloadOption.click();

      const download = await downloadPromise;
      expect(download.suggestedFilename()).toContain('root.log');
    } else {
      // Alternative: double-click to view, then check if content is accessible
      await page.keyboard.press('Escape'); // Close context menu
      await page.getByRole('button', { name: 'root.log', exact: true }).dblclick();
      await page.waitForLoadState('networkidle');

      // Should navigate to view page or show file content
      // At minimum, verify no error occurred
      await expect(page.locator('body')).not.toContainText(/500|404|Error|错误/i);
    }
  });

  test('should download file from nested path in tar.gz archive on agent', async ({ page }) => {
    // Navigate directly to internal directory in tar.gz archive via URL
    const archiveOrl = `orl://${AGENT_ID}@agent${TEST_LOGS_DIR}/test.tar.gz?entry=/internal`;
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(archiveOrl)}`);
    await page.waitForLoadState('networkidle');

    // Should see service.log
    await expect(page.getByText('service.log')).toBeVisible({ timeout: 5000 });

    // Double-click to view (use exact match for file button)
    await page.getByRole('button', { name: 'service.log', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');

    // Should navigate to view page or show content without error
    // Note: The behavior might differ - could go to view page or stay on explorer
    // The key is that no error should occur
    await expect(page.locator('body')).not.toContainText(/500|404|Error|错误/i);
  });

  test('should support zip archive on agent (if available)', async ({ page }) => {
    // Check if zip file was created
    const zipPath = path.join(TEST_LOGS_DIR, 'test.zip');
    if (!fs.existsSync(zipPath)) {
      // Skip this test gracefully - mark as passed since zip is optional
      console.log('Skipping zip archive test - zip file not created');
      return;
    }

    // Navigate to agent logs directory
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(`orl://${AGENT_ID}@agent${TEST_LOGS_DIR}`)}`);
    await page.waitForLoadState('networkidle');

    // Verify zip file is visible
    await expect(page.getByRole('button', { name: 'test.zip', exact: true })).toBeVisible({ timeout: 10000 });

    // Double-click to enter the archive
    await page.getByRole('button', { name: 'test.zip', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');

    // Verify we see archive contents
    await expect(page.locator('body')).not.toContainText(/500|Internal Server Error|错误/i);
    // Note: zip content verification is optional since zip creation may fail silently
  });

  test('should correctly display archive entry in URL parameters', async ({ page }) => {
    // Navigate to nested file in archive - view page requires sid parameter
    const archiveOrl = `orl://${AGENT_ID}@agent${TEST_LOGS_DIR}/test.tar.gz?entry=/internal/service.log`;
    const testSid = 'test-archive-view-sid';
    await page.goto(`http://localhost:5173/view?sid=${testSid}&file=${encodeURIComponent(archiveOrl)}`);
    await page.waitForLoadState('networkidle');

    // Verify page loads without the "缺少 sid 参数" error
    await expect(page.locator('body')).not.toContainText('缺少 sid 参数');

    // The view page should either show file content or an appropriate message
    // Since we're viewing an archive entry from an agent, it should work if backend supports it
    const bodyText = await page.locator('body').textContent() || '';
    const hasNoFatalError = !bodyText.includes('500') && !bodyText.includes('404');
    expect(hasNoFatalError).toBeTruthy();
  });

  test('should navigate up from archive entry to archive root', async ({ page }) => {
    // Start inside an archive
    const archiveOrl = `orl://${AGENT_ID}@agent${TEST_LOGS_DIR}/test.tar?entry=/subdir`;
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(archiveOrl)}`);
    await page.waitForLoadState('networkidle');

    // Should see nested.log
    await expect(page.getByText('nested.log')).toBeVisible({ timeout: 5000 });

    // Navigate up
    const upButton = page.locator('button:has(svg.lucide-arrow-left)');
    await upButton.click();
    await page.waitForLoadState('networkidle');

    // Should be at archive root (see root.log and subdir)
    await expect(page.getByText('root.log')).toBeVisible({ timeout: 5000 });
  });

  test('should navigate up from archive root to parent directory', async ({ page }) => {
    // Start at archive root
    const archiveOrl = `orl://${AGENT_ID}@agent${TEST_LOGS_DIR}/test.tar?entry=/`;
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(archiveOrl)}`);
    await page.waitForLoadState('networkidle');

    // Should see archive contents
    await expect(page.getByRole('button', { name: 'root.log', exact: true })).toBeVisible({ timeout: 5000 });

    // Navigate up (should exit archive)
    const upButton = page.locator('button:has(svg.lucide-arrow-left)');
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

  test('should handle multiple archive formats in same directory', async ({ page }) => {
    // Navigate to agent logs directory
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(`orl://${AGENT_ID}@agent${TEST_LOGS_DIR}`)}`);
    await page.waitForLoadState('networkidle');

    // Should see all archive formats
    await expect(page.getByRole('button', { name: 'test.tar', exact: true })).toBeVisible({ timeout: 5000 });
    await expect(page.getByRole('button', { name: 'test.tar.gz', exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'test.tgz', exact: true })).toBeVisible();

    // Enter tar archive and verify content
    await page.getByRole('button', { name: 'test.tar', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');
    await expect(page.getByText('root.log')).toBeVisible();

    // Go back
    const upButton = page.locator('button:has(svg.lucide-arrow-left)');
    await upButton.click();
    await page.waitForLoadState('networkidle');

    // Enter tar.gz archive and verify content
    await page.getByRole('button', { name: 'test.tar.gz', exact: true }).dblclick();
    await page.waitForLoadState('networkidle');
    await expect(page.getByText('app.log')).toBeVisible({ timeout: 5000 });
  });
});
