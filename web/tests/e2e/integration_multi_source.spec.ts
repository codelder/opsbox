import { test, expect, type APIRequestContext } from '@playwright/test';
import { spawn, type ChildProcessWithoutNullStreams } from 'child_process';
import * as fs from 'fs';
import * as net from 'net';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '../../..');
const webDir = path.join(repoRoot, 'web');
const backendDir = path.join(repoRoot, 'backend');

// --- Helpers ---

async function waitForHttpOk(request: APIRequestContext, url: string, timeoutMs: number) {
  const start = Date.now();
  let lastError: unknown = null;
  while (Date.now() - start < timeoutMs) {
    try {
      const resp = await request.get(url, { timeout: 1000 });
      if (resp.ok()) return;
      lastError = new Error(`HTTP ${resp.status()} ${url}`);
    } catch (e) {
      lastError = e;
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw lastError ?? new Error(`Timeout waiting for ${url}`);
}

function findServerCommand(): { command: string; args: string[]; cwd: string } {
  const candidate = path.join(backendDir, 'target', 'release', 'opsbox-server');
  if (fs.existsSync(candidate)) {
    return { command: candidate, args: ['--port', '4001'], cwd: backendDir };
  }
  return {
    command: 'cargo',
    args: ['run', '--release', '-p', 'opsbox-server', '--', '--port', '4001'],
    cwd: backendDir
  };
}

function findAgentCommand(): { command: string; argsPrefix: string[]; cwd: string } {
  return {
    command: 'cargo',
    argsPrefix: ['run', '--release', '-p', 'opsbox-agent', '--'],
    cwd: backendDir
  };
}

async function ensureBackendUp(request: APIRequestContext) {
  try {
    await waitForHttpOk(request, 'http://127.0.0.1:4001/healthy', 1000);
    return { started: false as const, proc: null as ChildProcessWithoutNullStreams | null };
  } catch {
    // fallthrough
  }

  const { command, args, cwd } = findServerCommand();
  const proc = spawn(command, args, {
    cwd,
    env: { ...process.env, RUST_LOG: process.env.RUST_LOG ?? 'info' },
    stdio: 'pipe'
  });
  proc.stdout.on('data', (d) => process.stdout.write(d));
  proc.stderr.on('data', (d) => process.stderr.write(d));

  await waitForHttpOk(request, 'http://127.0.0.1:4001/healthy', 120000);
  return { started: true as const, proc };
}

async function ensureWebUp(request: APIRequestContext) {
  try {
    await waitForHttpOk(request, 'http://127.0.0.1:5173/', 1000);
    return { started: false as const, proc: null as ChildProcessWithoutNullStreams | null };
  } catch {
    // fallthrough
  }

  const proc = spawn('pnpm', ['run', 'dev', '--', '--host', '127.0.0.1', '--port', '5173'], {
    cwd: webDir,
    env: { ...process.env, BACKEND_PORT: '4001', VITE_HOST: '127.0.0.1' },
    stdio: 'pipe'
  });
  proc.stdout.on('data', (d) => process.stdout.write(d));
  proc.stderr.on('data', (d) => process.stderr.write(d));

  await waitForHttpOk(request, 'http://127.0.0.1:5173/', 60000);
  return { started: true as const, proc };
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

// --- Test Suite ---

test.describe('Multi-Source Robustness E2E', () => {
  test.describe.configure({ mode: 'serial' });

  const API_LOGSEEK_BASE = 'http://127.0.0.1:4001/api/v1/logseek';
  const API_AGENT_BASE = 'http://127.0.0.1:4001/api/v1/agents';

  const RUN_ID = Date.now();
  const APP = `e2e_test_robustness_${RUN_ID}`;
  const AGENT_ID = `e2e-agent-robust-${RUN_ID}`;

  const TEST_ROOT_DIR = path.join(__dirname, `temp_robustness_${RUN_ID}`);
  const AGENT_ROOT_DIR = path.join(TEST_ROOT_DIR, 'agent');
  const LOCAL_ROOT_DIR = path.join(TEST_ROOT_DIR, 'local');
  const AGENT_LOG_DIR = path.join(TEST_ROOT_DIR, 'agent_runtime_logs');
  const AGENT_FILE = path.join(AGENT_ROOT_DIR, 'agent.log');
  const LOCAL_FILE = path.join(LOCAL_ROOT_DIR, 'local.log');

  let backendProc: ChildProcessWithoutNullStreams | null = null;
  let startedBackend = false;
  let webProc: ChildProcessWithoutNullStreams | null = null;
  let startedWeb = false;
  let agentProc: ChildProcessWithoutNullStreams | null = null;
  let agentPort: number | null = null;

  test.beforeAll(async ({ request }) => {
    const backend = await ensureBackendUp(request);
    backendProc = backend.proc;
    startedBackend = backend.started;

    const web = await ensureWebUp(request);
    webProc = web.proc;
    startedWeb = web.started;

    fs.mkdirSync(AGENT_ROOT_DIR, { recursive: true });
    fs.mkdirSync(AGENT_LOG_DIR, { recursive: true });
    fs.mkdirSync(LOCAL_ROOT_DIR, { recursive: true });

    // Create logs for both sources
    fs.writeFileSync(AGENT_FILE, `2025-01-01 12:00:00 [INFO] Agent is alive ${RUN_ID}\n`);
    fs.writeFileSync(LOCAL_FILE, `2025-01-01 12:00:00 [INFO] Local is alive ${RUN_ID}\n`);

    agentPort = await getFreePort();
    const { command, argsPrefix, cwd } = findAgentCommand();
    const args = [
      ...argsPrefix,
      '--agent-id',
      AGENT_ID,
      '--agent-name',
      'E2E Robustness Agent',
      '--server-endpoint',
      'http://127.0.0.1:4001',
      '--search-roots',
      AGENT_ROOT_DIR,
      '--listen-port',
      String(agentPort),
      '--no-heartbeat',
      '--log-dir',
      AGENT_LOG_DIR,
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

    await waitForHttpOk(request, `http://127.0.0.1:${agentPort}/health`, 60000);
    await waitForHttpOk(request, `${API_AGENT_BASE}/${AGENT_ID}`, 30000);

    // Configure Planner with BOTH Agent and Local sources
    const fullLocalPath = path.resolve(LOCAL_ROOT_DIR);
    const script = `SOURCES = [
      "orl://${AGENT_ID}@agent${AGENT_FILE}",
      "orl://local${fullLocalPath}?glob=**/*.log"
    ]`;

    const scriptResp = await request.post(`${API_LOGSEEK_BASE}/settings/planners/scripts`, {
      data: { app: APP, script }
    });
    expect(scriptResp.ok()).toBeTruthy();
  });

  test.afterAll(async ({ request }) => {
    try {
      await request.delete(`${API_LOGSEEK_BASE}/settings/planners/scripts/${APP}`);
    } catch {}
    if (agentProc) await stopProcess(agentProc);
    if (webProc && startedWeb) await stopProcess(webProc);
    if (backendProc && startedBackend) await stopProcess(backendProc);
    fs.rmSync(TEST_ROOT_DIR, { recursive: true, force: true });
  });

  // Test Partial Resilience
  test('should return partial results (from Local) when Agent source fails', async ({ page }) => {
    test.setTimeout(60000);

    await page.goto('/search');

    // 1. Initial Search: Should find BOTH Agent and Local logs
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${APP} "alive"`);
    await searchInput.press('Enter');

    // Expecting 2 results initially
    await expect(page.locator('.text-lg.font-semibold')).toContainText(/2 个结果/, { timeout: 15000 });
    await expect(page.getByText(`${RUN_ID}`).first()).toBeVisible();

    // 2. Kill Agent to simulate partial failure
    console.log('Testing: Killing agent process for Partial Failure test...');
    if (agentProc) {
      agentProc.kill('SIGKILL');
      await new Promise<void>((resolve) => agentProc!.once('exit', () => resolve()));
      agentProc = null;
    }

    // 3. Search Again: Should still find Local results!
    await searchInput.fill(`app:${APP} "alive"`); // Trigger re-search
    await searchInput.press('Enter');

    // Expectations:
    // 1. Service should NOT crash.
    // 2. Should show result from Local Source.

    await expect(async () => {
      const text = await page.locator('.text-lg.font-semibold').textContent();
      // Should find at least 1 result (Local)
      expect(text).toMatch(/[1-2] 个结果/);
    }).toPass({ timeout: 30000 });

    await expect(page.getByText('Local is alive')).toBeVisible();

    // Agent result should ideally be missing, OR user gets an error notification.
    // The key validation here is that we got Local results.
  });
});
