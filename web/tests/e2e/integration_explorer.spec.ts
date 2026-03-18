import { test, expect, toLocalOrl } from './fixtures';
import { spawn, type ChildProcessWithoutNullStreams } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import {
  getFreePort,
  findAgentCommand,
  stopProcess,
  waitForAgentReady,
  DEFAULT_AGENT_READY_TIMEOUT
} from './utils/agent';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Set debug logging for Rust components
process.env.RUST_LOG = 'debug';

test.describe('Explorer E2E', () => {
  test.describe.configure({ mode: 'serial' });

  const RUN_ID = Date.now();
  const AGENT_ID = `e2e-explorer-agent-${RUN_ID}`;

  const repoRoot = path.resolve(__dirname, '../../..');
  const TEST_ROOT_DIR = path.join(__dirname, `temp_explorer_${RUN_ID}`);
  const TEST_FILES_DIR = path.join(TEST_ROOT_DIR, 'files');
  const TEST_AGENT_LOG_DIR = path.join(TEST_ROOT_DIR, 'agent_runtime_logs');

  let agentProc: ChildProcessWithoutNullStreams | null = null;
  let agentPort: number | null = null;

  test.beforeAll(async ({ request }) => {
    test.setTimeout(120000);
    // Create test files
    fs.mkdirSync(TEST_FILES_DIR, { recursive: true });
    fs.mkdirSync(TEST_AGENT_LOG_DIR, { recursive: true });
    fs.writeFileSync(path.join(TEST_FILES_DIR, 'test.txt'), 'Hello Explorer!\n');
    fs.writeFileSync(path.join(TEST_FILES_DIR, 'test.log'), 'Log content\n');

    // Start agent
    agentPort = await getFreePort();
    console.log(`Starting agent on port ${agentPort} with ID ${AGENT_ID}`);
    const { command, argsPrefix, cwd } = findAgentCommand(repoRoot);

    const args = [
      ...argsPrefix,
      '--agent-id',
      AGENT_ID,
      '--agent-name',
      'E2E Explorer Agent',
      '--server-endpoint',
      'http://127.0.0.1:4001',
      '--search-roots',
      TEST_ROOT_DIR,
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
      env: { ...process.env, RUST_LOG: process.env.RUST_LOG ?? 'debug' },
      stdio: 'pipe'
    });
    agentProc.stdout.on('data', (d) => process.stdout.write(d));
    agentProc.stderr.on('data', (d) => process.stderr.write(d));

    // Wait for agent to be ready - rely only on API registration (Scheme A)
    // Use increased timeout (30s) to handle compilation delays in parallel test runs
    try {
      await waitForAgentReady(request, AGENT_ID, DEFAULT_AGENT_READY_TIMEOUT);
      console.log(`Agent ${AGENT_ID} fully ready`);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`Failed to wait for agent: ${errorMessage}`);
      // Fall back to old behavior for compatibility
      console.log(`Falling back to fixed 10-second wait...`);
      await new Promise((resolve) => setTimeout(resolve, 10000));
    }
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

  test('should list local files with correct API field name', async ({ page }) => {
    // This test ensures frontend sends the correct "orl" field name to backend

    // Monitor network requests to verify correct field name
    const requests: Record<string, unknown>[] = [];
    page.on('request', (request) => {
      if (request.url().includes('/api/v1/explorer/list')) {
        const postData = request.postData();
        if (postData) {
          requests.push(JSON.parse(postData) as Record<string, unknown>);
        }
      }
    });

    // Navigate to local files in browser
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(toLocalOrl(TEST_FILES_DIR))}`);

    // Wait for the page to load
    await page.waitForLoadState('networkidle');

    // Verify we can see our test files
    await expect(page.getByText('test.txt')).toBeVisible({ timeout: 5000 });
    await expect(page.getByText('test.log')).toBeVisible();

    // Verify the request used the correct 'orl' field name
    expect(requests.length).toBeGreaterThan(0);
    expect(requests[0]).toHaveProperty('orl');
    expect(requests[0]).not.toHaveProperty('odfi');
  });

  test('should list agent root (discovery) with correct provider registration', async ({ page, request }) => {
    // This test case captures the bug we just fixed:
    // - OrlManager was using effective_id() which mapped empty ID to "localhost"
    // - This caused key to be "agent.localhost" instead of "agent.root"
    // - AgentDiscoveryFileSystem was registered as "agent.root" but couldn't be found

    // First, verify the agent is registered via API (more reliable than UI polling)
    const agentResponse = await request.get(`http://127.0.0.1:4001/api/v1/agents/${AGENT_ID}`);
    expect(agentResponse.ok()).toBeTruthy();

    await page.goto('http://localhost:5173/explorer?orl=orl%3A%2F%2Fagent%2F');

    // Wait for the page to load
    await page.waitForLoadState('networkidle');

    // Should list available agents
    // Look for our test agent with extended timeout and retry logic
    // The UI may need time to fetch and render the agent list
    await expect(page.getByText(AGENT_ID, { exact: false })).toBeVisible({ timeout: 20000 });
  });

  test('should list agent root directory (empty path)', async ({ page, request }) => {
    // This test verifies the scenario: orl://agent-id@agent/
    // When path is empty (root directory), agent should list search roots
    // Bug: Agent returns 404 when path is empty or "/"

    // Ensure agent is registered before navigating (reduces CI/local race conditions)
    const agentResponse = await request.get(`http://127.0.0.1:4001/api/v1/agents/${AGENT_ID}`);
    expect(agentResponse.ok()).toBeTruthy();

    const agentRootOrl = `orl://${AGENT_ID}@agent/`;
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(agentRootOrl)}`);

    // Wait for the page to load
    await page.waitForLoadState('networkidle');

    // Should list the search root directory itself
    const rootDirName = path.basename(TEST_ROOT_DIR);

    // We use a regex to match because the UI might truncate long folder names
    // e.g., "temp_explorer_1768064125538" -> "temp_explorer...64125538"
    const namePrefix = rootDirName.substring(0, 10);
    await expect(page.getByText(new RegExp(namePrefix)).first()).toBeVisible({ timeout: 10000 });
  });

  test('should list agent files', async ({ page }) => {
    await page.goto(
      `http://localhost:5173/explorer?orl=${encodeURIComponent(`orl://${AGENT_ID}@agent${TEST_FILES_DIR}`)}`
    );

    await page.waitForLoadState('networkidle');

    // Verify we can see the test files
    await expect(page.getByText('test.txt')).toBeVisible({ timeout: 5000 });
    await expect(page.getByText('test.log')).toBeVisible();
  });

  test('should download local file by clicking', async ({ page }) => {
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(toLocalOrl(TEST_FILES_DIR))}`);

    await page.waitForLoadState('networkidle');

    // Right-click on the file to open context menu
    await page.getByText('test.txt').click({ button: 'right' });

    // Wait for download event before clicking menu item
    const downloadPromise = page.waitForEvent('download', { timeout: 10000 });
    await page.getByText('下载').click();

    const download = await downloadPromise;

    // Verify filename
    expect(download.suggestedFilename()).toBe('test.txt');

    // Verify file size > 0
    const downloadPath = await download.path();
    expect(downloadPath).toBeTruthy();
    const stats = fs.statSync(downloadPath!);
    expect(stats.size).toBeGreaterThan(0);
  });

  test('should navigate through agent files by clicking', async ({ page }) => {
    // This test captures a real bug reported by user:
    // When clicking into a subdirectory after entering agent's search-roots,
    // the agent returns 404 Not Found

    // Start at agent root
    await page.goto('http://localhost:5173/explorer?orl=orl%3A%2F%2Fagent%2F');
    await page.waitForLoadState('networkidle');

    // Find and click on our test agent link
    const agentItem = page.locator(`text=${AGENT_ID}`).first();
    await expect(agentItem).toBeVisible({ timeout: 5000 });

    // Click to navigate to agent root
    await agentItem.click();

    // Wait for the search root folder to appear
    const rootDirName = path.basename(TEST_ROOT_DIR);
    const namePrefix = rootDirName.substring(0, 10);
    const rootDirLocator = page.getByText(new RegExp(namePrefix)).first();
    await expect(rootDirLocator).toBeVisible({ timeout: 10000 });

    // Double-click the root folder to enter it
    await rootDirLocator.dblclick();

    // Now we should see 'files'
    await expect(page.getByText('files')).toBeVisible({ timeout: 5000 });

    // Find and double-click on 'files' directory to navigate into it
    const filesItem = page.locator('text=files').first();
    await filesItem.dblclick();

    // Wait for the test files to appear
    await expect(page.getByText('test.txt')).toBeVisible({ timeout: 10000 });
    await expect(page.getByText('test.log')).toBeVisible();
  });

  test('should verify API requests use correct field names', async ({ page }) => {
    // Monitor all explorer API requests
    interface ExplorerRequest {
      url: string;
      method: string;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      body: Record<string, any> | null;
    }
    const requests: ExplorerRequest[] = [];
    page.on('request', (request) => {
      if (request.url().includes('/api/v1/explorer/')) {
        const postData = request.postData();
        requests.push({
          url: request.url(),
          method: request.method(),
          body: postData ? JSON.parse(postData) : null
        });
      }
    });

    // Navigate through several pages to generate multiple requests
    await page.goto('http://localhost:5173/explorer?orl=orl%3A%2F%2Fagent%2F');
    await page.waitForLoadState('networkidle');

    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(`orl://${AGENT_ID}@agent/`)}`);
    await page.waitForLoadState('networkidle');

    // Verify all POST requests to /list use 'orl' field
    const listRequests = requests.filter((r) => r.url.includes('/list') && r.method === 'POST');
    expect(listRequests.length).toBeGreaterThan(0);

    for (const req of listRequests) {
      if (req.body) {
        const body = req.body as Record<string, unknown>;
        expect(body).toHaveProperty('orl');
        expect(body).not.toHaveProperty('odfi');
      }
    }
  });

  test('should support multiple search roots and navigate each individually', async ({ page, request }) => {
    const multiRootAgentId = `${AGENT_ID}-multi`;
    const root1 = path.join(TEST_ROOT_DIR, 'multi_root_1');
    const root2 = path.join(TEST_ROOT_DIR, 'multi_root_2');

    fs.mkdirSync(root1, { recursive: true });
    fs.mkdirSync(root2, { recursive: true });
    fs.writeFileSync(path.join(root1, 'file1.txt'), 'content 1');
    fs.writeFileSync(path.join(root2, 'file2.txt'), 'content 2');

    const multiPort = await getFreePort();
    const { command, argsPrefix, cwd } = findAgentCommand(repoRoot);

    const args = [
      ...argsPrefix,
      '--agent-id',
      multiRootAgentId,
      '--agent-name',
      'Multi Root Agent',
      '--server-endpoint',
      'http://127.0.0.1:4001',
      '--search-roots',
      `${root1},${root2}`,
      '--listen-port',
      String(multiPort),
      '--no-heartbeat',
      '--log-dir',
      path.join(TEST_ROOT_DIR, 'multi_agent_logs'),
      '--log-retention',
      '1'
    ];

    const proc = spawn(command, args, {
      cwd,
      env: { ...process.env, RUST_LOG: process.env.RUST_LOG ?? 'info' },
      stdio: 'pipe'
    });

    // Capture agent process output for debugging
    proc.stdout.on('data', (d) => process.stdout.write(d));
    proc.stderr.on('data', (d) => process.stderr.write(d));

    try {
      // Wait for agent to register - use smart waiting instead of fixed delay
      await waitForAgentReady(request, multiRootAgentId, 10000);

      // Go to agent root: orl://agent-id@agent/
      await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(`orl://${multiRootAgentId}@agent/`)}`);
      await page.waitForLoadState('networkidle');

      // Both roots should be visible since we now list roots as virtual directories
      await expect(page.getByText('multi_root_1')).toBeVisible({ timeout: 10000 });
      await expect(page.getByText('multi_root_2')).toBeVisible({ timeout: 5000 });

      // Navigate into root 1
      await page.locator('text=multi_root_1').first().dblclick();
      await expect(page.getByText('file1.txt')).toBeVisible({ timeout: 5000 });

      // Go back to agent root using breadcrumb or URL
      await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(`orl://${multiRootAgentId}@agent/`)}`);
      await page.waitForLoadState('networkidle');

      // Navigate into root 2
      await page.locator('text=multi_root_2').first().dblclick();
      await expect(page.getByText('file2.txt')).toBeVisible({ timeout: 5000 });
    } finally {
      await stopProcess(proc);
      try {
        await request.delete(`http://127.0.0.1:4001/api/v1/agents/${multiRootAgentId}`);
      } catch {
        // ignore
      }
    }
  });

  test('should not escape search-roots when clicking up button', async ({ page, request }) => {
    // Create a special agent for this test
    const escapeAgentId = `${AGENT_ID}-escape`;
    const innerRoot = path.join(TEST_ROOT_DIR, 'escape_test_root');
    fs.mkdirSync(innerRoot, { recursive: true });
    // Create a file inside the root
    fs.writeFileSync(path.join(innerRoot, 'authorized.txt'), 'authorized');
    // Create a file OUTSIDE the root (in the parent directory)
    fs.writeFileSync(path.join(TEST_ROOT_DIR, 'secret.txt'), 'secret outside root');

    const escapePort = await getFreePort();
    const { command, argsPrefix, cwd } = findAgentCommand(repoRoot);

    const args = [
      ...argsPrefix,
      '--agent-id',
      escapeAgentId,
      '--agent-name',
      'Escape Test Agent',
      '--server-endpoint',
      'http://127.0.0.1:4001',
      '--search-roots',
      innerRoot,
      '--listen-port',
      String(escapePort),
      '--no-heartbeat',
      '--log-dir',
      path.join(TEST_ROOT_DIR, 'escape_agent_logs'),
      '--log-retention',
      '1'
    ];

    const proc = spawn(command, args, {
      cwd,
      env: { ...process.env, RUST_LOG: process.env.RUST_LOG ?? 'info' },
      stdio: 'pipe'
    });

    try {
      // Wait for agent to register - use smart waiting instead of fixed delay
      await waitForAgentReady(request, escapeAgentId, 10000);

      // Navigate directly into the root folder
      const rootOrl = `orl://${escapeAgentId}@agent${innerRoot}`;
      await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(rootOrl)}`);
      await page.waitForLoadState('networkidle');

      // Verify we see authorized file
      await expect(page.getByText('authorized.txt')).toBeVisible({ timeout: 5000 });

      // Click the "Up" button to try to escape to parent
      // The button has lucide-arrow-left icon
      const upButton = page.locator('button:has(svg.lucide-arrow-left)');
      await upButton.click();

      // Ensure we didn't escape to the actual parent directory (TEST_ROOT_DIR)
      // If we escaped, we would see 'authorized.txt' within a folder or 'secret.txt'
      await expect(page.getByText('secret.txt')).not.toBeVisible();

      // Instead, we should have fallen back to the Agent's root list
      const virtualRootOrl = `orl://${escapeAgentId}@agent/`;
      await expect(page).toHaveURL(new RegExp(encodeURIComponent(virtualRootOrl)));

      // And we should see the authorized root directory name again
      const rootDirName = path.basename(innerRoot);
      await expect(page.getByText(rootDirName)).toBeVisible({ timeout: 5000 });
    } finally {
      await stopProcess(proc);
      try {
        await request.delete(`http://127.0.0.1:4001/api/v1/agents/${escapeAgentId}`);
      } catch {
        // ignore
      }
    }
  });

  test('should fall back to agent root instead of listing a fuzzy-matched trap directory after back navigation', async ({
    page,
    request
  }) => {
    const mismatchAgentId = `${AGENT_ID}-mismatch`;
    const outerParent = path.join(TEST_ROOT_DIR, 'mismatch_parent');
    const allowedRoot = path.join(outerParent, 'allowed_root');
    const validChild = path.join(allowedRoot, 'valid_child');
    fs.mkdirSync(validChild, { recursive: true });
    fs.writeFileSync(path.join(validChild, 'inside.txt'), 'inside root');

    const normalizedParent = outerParent.split(path.sep).filter(Boolean);
    const trapDir = path.join(allowedRoot, ...normalizedParent, 'codelder');
    fs.mkdirSync(trapDir, { recursive: true });
    fs.writeFileSync(path.join(trapDir, 'leaked.txt'), 'this should never be listed after back navigation');

    const mismatchPort = await getFreePort();
    const { command, argsPrefix, cwd } = findAgentCommand(repoRoot);
    const args = [
      ...argsPrefix,
      '--agent-id',
      mismatchAgentId,
      '--agent-name',
      'Mismatch Agent',
      '--server-endpoint',
      'http://127.0.0.1:4001',
      '--search-roots',
      allowedRoot,
      '--listen-port',
      String(mismatchPort),
      '--no-heartbeat',
      '--log-dir',
      path.join(TEST_ROOT_DIR, 'mismatch_agent_logs'),
      '--log-retention',
      '1'
    ];

    const proc = spawn(command, args, {
      cwd,
      env: { ...process.env, RUST_LOG: process.env.RUST_LOG ?? 'info' },
      stdio: 'pipe'
    });
    proc.stdout.on('data', (d) => process.stdout.write(d));
    proc.stderr.on('data', (d) => process.stderr.write(d));

    try {
      await waitForAgentReady(request, mismatchAgentId, 10000);

      const deepOrl = `orl://${mismatchAgentId}@agent${validChild}`;
      await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(deepOrl)}`);
      await page.waitForLoadState('networkidle');
      await expect(page.getByText('inside.txt')).toBeVisible({ timeout: 10000 });

      const upButton = page.locator('button:has(svg.lucide-arrow-left)');

      await upButton.click();
      const allowedRootOrl = `orl://${mismatchAgentId}@agent${allowedRoot}`;
      await expect(page).toHaveURL(new RegExp(encodeURIComponent(allowedRootOrl)));
      await expect(page.getByText('valid_child')).toBeVisible({ timeout: 10000 });

      await upButton.click();
      const virtualRootOrl = `orl://${mismatchAgentId}@agent/`;
      await expect(page).toHaveURL(new RegExp(encodeURIComponent(virtualRootOrl)));
      await expect(page.getByText('allowed_root')).toBeVisible({ timeout: 10000 });
      await expect(page.getByText('codelder')).not.toBeVisible();
      await expect(page.getByText('leaked.txt')).not.toBeVisible();
    } finally {
      await stopProcess(proc);
      try {
        await request.delete(`http://127.0.0.1:4001/api/v1/agents/${mismatchAgentId}`);
      } catch {
        // ignore
      }
    }
  });

  test('should prohibit access to paths outside search-roots via manual ORL entry', async ({ page, request }) => {
    // 1. Setup a restricted agent
    const restrictedAgentId = `${AGENT_ID}-restricted`;
    const allowedDir = path.join(TEST_ROOT_DIR, 'allowed_zone');
    fs.mkdirSync(allowedDir, { recursive: true });

    // We'll try to access a sibling directory that is NOT allowed
    const forbiddenDir = path.join(TEST_ROOT_DIR, 'forbidden_zone');
    fs.mkdirSync(forbiddenDir, { recursive: true });
    fs.writeFileSync(path.join(forbiddenDir, 'secret_data.txt'), 'this should never be seen');

    const restrictedPort = await getFreePort();
    const { command, argsPrefix, cwd } = findAgentCommand(repoRoot);

    const args = [
      ...argsPrefix,
      '--agent-id',
      restrictedAgentId,
      '--agent-name',
      'Restricted Agent',
      '--server-endpoint',
      'http://127.0.0.1:4001',
      '--search-roots',
      allowedDir,
      '--listen-port',
      String(restrictedPort),
      '--no-heartbeat',
      '--log-dir',
      path.join(TEST_ROOT_DIR, 'restricted_agent_logs'),
      '--log-retention',
      '1'
    ];

    const proc = spawn(command, args, {
      cwd,
      env: { ...process.env, RUST_LOG: process.env.RUST_LOG ?? 'info' },
      stdio: 'pipe'
    });

    // Capture agent process output for debugging
    proc.stdout.on('data', (d) => process.stdout.write(d));
    proc.stderr.on('data', (d) => process.stderr.write(d));

    try {
      // Wait for agent to register - use smart waiting instead of fixed delay
      await waitForAgentReady(request, restrictedAgentId, 10000);

      // 2. Try to manually navigate to the forbidden directory by typing ORL
      // Note: We use the absolute path of forbiddenDir to test access control
      const forbiddenOrl = `orl://${restrictedAgentId}@agent${forbiddenDir}`;

      await page.goto('http://localhost:5173/explorer');
      await page.waitForLoadState('networkidle');

      const input = page.locator('#orl-input');
      await input.fill(forbiddenOrl);
      await input.press('Enter');

      // 3. Verify access is denied
      // The Agent backend should return 404/Access Denied which frontend displays
      await expect(page.getByText('资源列举失败')).toBeVisible({ timeout: 5000 });
      await expect(page.getByText('错误详情')).toBeVisible();

      // Verify files in the forbidden directory are NOT visible
      await expect(page.getByText('secret_data.txt')).not.toBeVisible();

      // Navigate to allowed dir and verify it works
      const allowedOrl = `orl://${restrictedAgentId}@agent${allowedDir}`;
      await input.fill(allowedOrl);
      await input.press('Enter');
      // Verify success: error display is NOT visible
      await expect(page.getByText('资源列举失败')).not.toBeVisible();
    } finally {
      await stopProcess(proc);
      try {
        await request.delete(`http://127.0.0.1:4001/api/v1/agents/${restrictedAgentId}`);
      } catch {
        // ignore
      }
    }
  });

  test('should access file with Chinese characters in name', async ({ page, request }) => {
    const chineseAgentId = `${AGENT_ID}-chinese`;
    const chineseRoot = path.join(TEST_ROOT_DIR, 'chinese_test');
    fs.mkdirSync(chineseRoot, { recursive: true });

    // File name with Chinese characters
    const fileName = '测试文件_截屏.txt';
    fs.writeFileSync(path.join(chineseRoot, fileName), 'Chinese content');

    const chinesePort = await getFreePort();
    const { command, argsPrefix, cwd } = findAgentCommand(repoRoot);

    const args = [
      ...argsPrefix,
      '--agent-id',
      chineseAgentId,
      '--agent-name',
      'Chinese Path Agent',
      '--server-endpoint',
      'http://127.0.0.1:4001',
      '--search-roots',
      chineseRoot,
      '--listen-port',
      String(chinesePort),
      '--no-heartbeat',
      '--log-dir',
      path.join(TEST_ROOT_DIR, 'chinese_agent_logs'),
      '--log-retention',
      '1'
    ];

    const proc = spawn(command, args, {
      cwd,
      env: { ...process.env, RUST_LOG: process.env.RUST_LOG ?? 'info' },
      stdio: 'pipe'
    });

    // Capture agent process output for debugging
    proc.stdout.on('data', (d) => process.stdout.write(d));
    proc.stderr.on('data', (d) => process.stderr.write(d));

    try {
      // Wait for agent to register - use smart waiting instead of fixed delay
      await waitForAgentReady(request, chineseAgentId, 10000);

      // 1. Visit the directory
      const rootOrl = `orl://${chineseAgentId}@agent${chineseRoot}`;
      await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(rootOrl)}`);
      await page.waitForLoadState('networkidle');

      // 2. Verify file is listed correctly
      await expect(page.getByText(fileName)).toBeVisible({ timeout: 10000 });

      // 3. Optional: double click to verify it can be "opened" (redirected to /view)
      // Actually, we just want to know if the listing and path resolution works.
    } finally {
      await stopProcess(proc);
      try {
        await request.delete(`http://127.0.0.1:4001/api/v1/agents/${chineseAgentId}`);
      } catch {
        // ignore
      }
    }
  });

  test('should navigate into and out of archive correctly', async ({ page }) => {
    // 1. Prepare archive file using system tar command
    const archiveContentDir = path.join(TEST_FILES_DIR, 'archive_content');
    const archivePath = path.join(TEST_FILES_DIR, 'test_archive.tar');

    fs.mkdirSync(archiveContentDir, { recursive: true });
    // Create a file in root of archive
    fs.writeFileSync(path.join(archiveContentDir, 'root_file.txt'), 'root content');
    // Create a subdir in archive
    const subDir = path.join(archiveContentDir, 'sub_dir');
    fs.mkdirSync(subDir, { recursive: true });
    fs.writeFileSync(path.join(subDir, 'inner_file.txt'), 'inner content');

    // Create tar
    const { execSync } = await import('child_process');
    // -C to change directory so we don't include full path
    execSync(`tar -cf "${archivePath}" -C "${TEST_FILES_DIR}" archive_content`);

    // 2. Navigate to the containing folder
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(toLocalOrl(TEST_FILES_DIR))}`);
    await page.waitForLoadState('networkidle');

    // 3. Double click the archive file to enter it
    await page.getByText('test_archive.tar').dblclick();

    // Wait for page to load after navigation
    await page.waitForLoadState('networkidle');

    // 4. Verify we are "inside" the archive (URL params handling check) and see 'archive_content' folder
    // Note: tar command usually preserves the top level folder if we tarred 'archive_content'
    await expect(page.getByText('archive_content')).toBeVisible();

    // Enter 'archive_content' folder
    await page.getByText('archive_content').dblclick();

    // Wait for page to load after entering subdirectory
    await page.waitForLoadState('networkidle');

    // Verify we see files
    await expect(page.getByText('root_file.txt')).toBeVisible();
    await expect(page.getByText('sub_dir')).toBeVisible();

    // 5. Test "Go Up"
    const upButton = page.locator('button:has(svg.lucide-arrow-left)');

    // Up from archive_content -> archive root (virtual root listing contents of tar)
    // Note: We are currently at archive.tar?target=archive&entry=archive_content
    await upButton.click();
    // Wait for navigation to complete
    await page.waitForLoadState('networkidle');
    // Now we are at root of tar. Should see 'archive_content' folder again.
    await expect(page.getByText('archive_content')).toBeVisible();

    // 6. Up from archive root -> parent directory (TEST_FILES_DIR)
    await upButton.click();
    // Wait for navigation to complete
    await page.waitForLoadState('networkidle');

    // Now we should be back at TEST_FILES_DIR
    // We should see 'test_archive.tar' as a FILE
    await expect(page.getByText('test_archive.tar')).toBeVisible();

    // CRITICAL: Check URL does NOT contain target=archive
    const url = new URL(page.url());
    expect(url.searchParams.has('target')).toBe(false);
    expect(url.searchParams.has('entry')).toBe(false);
  });

  test('should navigate up correctly for Local, Agent, and S3 paths', async ({ page, request }) => {
    // 1. Setup Local directory structure
    const localL1 = path.join(TEST_FILES_DIR, 'level1');
    const localL2 = path.join(localL1, 'level2');
    fs.mkdirSync(localL2, { recursive: true });

    // --- Local Navigation Test ---
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(toLocalOrl(localL2))}`);
    await page.waitForLoadState('networkidle');

    const upButton = page.locator('button:has(svg.lucide-arrow-left)');

    // Go Up: level2 -> level1
    // Note: Frontend normalizes ORL paths (orl://local//path -> orl://local/path)
    // We check for the path content rather than exact slash count for cross-platform robustness
    await upButton.click();
    await expect(page).toHaveURL(/level1/);

    // Go Up: level1 -> TEST_FILES_DIR (files)
    await upButton.click();
    await expect(page).toHaveURL(/files/);

    // --- Agent Navigation Test ---
    const navAgentId = `${AGENT_ID}-nav`;
    const agentRoot = path.join(TEST_ROOT_DIR, 'nav_agent_root');
    const agentSub = path.join(agentRoot, 'subdir');
    fs.mkdirSync(agentSub, { recursive: true });

    const navPort = await getFreePort();
    const { command, argsPrefix, cwd } = findAgentCommand(repoRoot);
    const args = [
      ...argsPrefix,
      '--agent-id',
      navAgentId,
      '--agent-name',
      'Nav Agent',
      '--server-endpoint',
      'http://127.0.0.1:4001',
      '--search-roots',
      agentRoot,
      '--listen-port',
      String(navPort),
      '--no-heartbeat',
      '--log-dir',
      path.join(TEST_ROOT_DIR, 'nav_agent_logs'),
      '--log-retention',
      '1'
    ];

    const proc = spawn(command, args, { cwd, env: { ...process.env, RUST_LOG: 'info' }, stdio: 'pipe' });

    // Capture agent process output for debugging
    proc.stdout.on('data', (d) => process.stdout.write(d));
    proc.stderr.on('data', (d) => process.stderr.write(d));

    try {
      // Wait for agent to register - use smart waiting instead of fixed delay
      await waitForAgentReady(request, navAgentId, 10000);

      // Start deep in agent
      const agentDeepOrl = `orl://${navAgentId}@agent${agentSub}`;
      await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(agentDeepOrl)}`);
      await page.waitForLoadState('networkidle');

      // Go Up: subdir -> agentRoot
      await upButton.click();
      const agentRootOrl = `orl://${navAgentId}@agent${agentRoot}`;
      await expect(page).toHaveURL(new RegExp(encodeURIComponent(agentRootOrl)));

      // Go Up: agentRoot -> Agent Virtual Root (orl://id@agent/)
      // This verifies the "fallback to root" logic when going up from a search root
      await upButton.click();
      const agentVirtualRoot = `orl://${navAgentId}@agent/`;
      await expect(page).toHaveURL(new RegExp(encodeURIComponent(agentVirtualRoot)));
    } finally {
      await stopProcess(proc);
      try {
        await request.delete(`http://127.0.0.1:4001/api/v1/agents/${navAgentId}`);
      } catch {
        // ignore
      }
    }

    // --- S3 URL Logic Test ---
    // Since we don't have a real S3, we rely on the fact that the frontend updates the URL
    // immediately even if loading fails. This tests the path truncation logic for S3 ORLs.
    const s3DeepOrl = 'orl://dummy-profile@s3/my-bucket/folder/subfolder';
    await page.goto(`http://localhost:5173/explorer?orl=${encodeURIComponent(s3DeepOrl)}`);
    // Note: It will likely show an error or empty list, but we care about the URL state

    // Go Up: subfolder -> folder
    await upButton.click();
    const s3FolderOrl = 'orl://dummy-profile@s3/my-bucket/folder';
    await expect(page).toHaveURL(new RegExp(encodeURIComponent(s3FolderOrl)));

    // Go Up: folder -> my-bucket
    await upButton.click();
    const s3BucketOrl = 'orl://dummy-profile@s3/my-bucket';
    await expect(page).toHaveURL(new RegExp(encodeURIComponent(s3BucketOrl)));

    // Go Up: my-bucket -> S3 Root (orl://s3/)
    // Our logic pops path parts. if path is 'my-bucket', popping gives empty path -> 'orl://dummy-profile@s3/'
    // Wait, let's check svelte logic:
    // pathParts = url.pathname.split('/').filter(p => p);
    // For 'orl://...@s3/bucket', pathname is '/bucket'. Pop -> empty.
    // url.pathname = '/'. Result: 'orl://...@s3/'
    // But usually S3 root is 'orl://s3/' (no profile).
    // The current implementation preserves authority. So it goes to 'orl://dummy-profile@s3/'
    // This is acceptable behavior for "Up" within a profile context.
    await upButton.click();
    const s3ProfileRoot = 'orl://dummy-profile@s3/';
    await expect(page).toHaveURL(new RegExp(encodeURIComponent(s3ProfileRoot)));
  });
});
