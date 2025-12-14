import { test, expect, type APIRequestContext } from '@playwright/test';
import { spawn, type ChildProcessWithoutNullStreams } from 'child_process';
import * as fs from 'fs';
import * as net from 'net';
import * as path from 'path';
import * as zlib from 'zlib';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

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
  const tarBuf = Buffer.concat(blocks);
  fs.writeFileSync(outFile, zlib.gzipSync(tarBuf));
}

function writeGzFile(outFile: string, content: string) {
  fs.writeFileSync(outFile, zlib.gzipSync(Buffer.from(content, 'utf8')));
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

function findAgentCommand(repoRoot: string): { command: string; argsPrefix: string[]; cwd: string } {
  const backendDir = path.join(repoRoot, 'backend');
  const candidate = path.join(backendDir, 'target', 'release', 'opsbox-agent');
  if (fs.existsSync(candidate)) {
    return { command: candidate, argsPrefix: [], cwd: backendDir };
  }

  return { command: 'cargo', argsPrefix: ['run', '-p', 'opsbox-agent', '--'], cwd: backendDir };
}

function findServerCommand(repoRoot: string): { command: string; args: string[]; cwd: string } {
  const backendDir = path.join(repoRoot, 'backend');
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

async function ensureBackendUp(request: APIRequestContext, repoRoot: string) {
  try {
    await waitForHttpOk(request, 'http://127.0.0.1:4001/healthy', 1000);
    return { started: false as const, proc: null as ChildProcessWithoutNullStreams | null };
  } catch {
    // fallthrough: start it
  }

  const { command, args, cwd } = findServerCommand(repoRoot);
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

async function ensureWebUp(request: APIRequestContext, repoRoot: string) {
  try {
    await waitForHttpOk(request, 'http://127.0.0.1:5173/', 1000);
    return { started: false as const, proc: null as ChildProcessWithoutNullStreams | null };
  } catch {
    // fallthrough: start it
  }

  const webDir = path.join(repoRoot, 'web');
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

test.describe('Agent Integration E2E', () => {
  test.describe.configure({ mode: 'serial' });

  const API_LOGSEEK_BASE = 'http://127.0.0.1:4001/api/v1/logseek';
  const API_AGENT_BASE = 'http://127.0.0.1:4001/api/v1/agents';
  const RUN_ID = Date.now();

  const AGENT_ID = `e2e-agent-${RUN_ID}`;
  const TEST_APP_AGENT = `e2e_test_agent_${RUN_ID}`;
  const TEST_APP_AGENT_ARCHIVE = `e2e_test_agent_archive_${RUN_ID}`;
  const TEST_APP_AGENT_TARGZ = `e2e_test_agent_targz_${RUN_ID}`;
  const TEST_APP_AGENT_DIR_MULTI_GZ = `e2e_test_agent_dir_multi_gz_${RUN_ID}`;
  const UNI_ID_AGENT = `E2E_AGENT_${RUN_ID}`;
  const UNI_ID_AGENT_ARCHIVE = `E2E_AGENT_ARCHIVE_${RUN_ID}`;
  const UNI_ID_AGENT_TARGZ = `E2E_AGENT_TARGZ_${RUN_ID}`;
  const UNI_ID_AGENT_DIR_MULTI_GZ = `E2E_AGENT_DIR_MULTI_GZ_${RUN_ID}`;

  const repoRoot = path.resolve(__dirname, '../../..');

  const TEST_ROOT_DIR = path.join(__dirname, `temp_agent_${RUN_ID}`);
  const TEST_LOGS_DIR = path.join(TEST_ROOT_DIR, 'logs');
  const TEST_LOG_FILE = path.join(TEST_LOGS_DIR, 'agent.log');
  const TEST_ARCHIVE_FILE = path.join(TEST_LOGS_DIR, 'agent-archive.tar');
  const TEST_TARGZ_FILE = path.join(TEST_LOGS_DIR, 'agent-archive.tar.gz');
  const TEST_MULTI_GZ_1 = path.join(TEST_LOGS_DIR, 'multi-1.log.gz');
  const TEST_MULTI_GZ_2 = path.join(TEST_LOGS_DIR, 'multi-2.log.gz');
  const TEST_AGENT_LOG_DIR = path.join(TEST_ROOT_DIR, 'agent_runtime_logs');

  let backendProc: ChildProcessWithoutNullStreams | null = null;
  let webProc: ChildProcessWithoutNullStreams | null = null;
  let startedBackend = false;
  let startedWeb = false;
  let agentProc: ChildProcessWithoutNullStreams | null = null;
  let agentPort: number | null = null;

  test.beforeAll(async ({ request }) => {
    const backend = await ensureBackendUp(request, repoRoot);
    backendProc = backend.proc;
    startedBackend = backend.started;

    const web = await ensureWebUp(request, repoRoot);
    webProc = web.proc;
    startedWeb = web.started;

    fs.mkdirSync(TEST_LOGS_DIR, { recursive: true });
    fs.mkdirSync(TEST_AGENT_LOG_DIR, { recursive: true });
    fs.writeFileSync(TEST_LOG_FILE, `2025-01-01 12:00:00 [INFO] agent result ${UNI_ID_AGENT}\n`);
    writeTarFile(TEST_ARCHIVE_FILE, [
      {
        name: 'internal/archived.log',
        content: `2025-01-01 12:00:00 [INFO] agent archive result ${UNI_ID_AGENT_ARCHIVE}\n`
      }
    ]);
    writeTarGzFile(TEST_TARGZ_FILE, [
      {
        name: 'internal/archived-tgz.log',
        content: `2025-01-01 12:00:00 [INFO] agent tar.gz result ${UNI_ID_AGENT_TARGZ}\n`
      }
    ]);
    writeGzFile(TEST_MULTI_GZ_1, `2025-01-01 12:00:00 [INFO] gz 1 ${UNI_ID_AGENT_DIR_MULTI_GZ}\n`);
    writeGzFile(TEST_MULTI_GZ_2, `2025-01-01 12:00:00 [INFO] gz 2 ${UNI_ID_AGENT_DIR_MULTI_GZ}\n`);

    agentPort = await getFreePort();

    const { command, argsPrefix, cwd } = findAgentCommand(repoRoot);

    const args = [
      ...argsPrefix,
      '--agent-id',
      AGENT_ID,
      '--agent-name',
      'E2E Local Agent',
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
      env: { ...process.env, RUST_LOG: process.env.RUST_LOG ?? 'info' },
      stdio: 'pipe'
    });
    agentProc.stdout.on('data', (d) => process.stdout.write(d));
    agentProc.stderr.on('data', (d) => process.stderr.write(d));

    await waitForHttpOk(request, `http://127.0.0.1:${agentPort}/health`, 15000);

    // 确认 Agent 已注册到 server（new_by_agent_id 依赖 agent-manager 里 host/listen_port 标签）
    await waitForHttpOk(request, `${API_AGENT_BASE}/${AGENT_ID}`, 15000);

    const script = `
SOURCES = [{
    'endpoint': { 'kind': 'agent', 'agent_id': '${AGENT_ID}', 'subpath': 'logs' },
    'target':   { 'type': 'dir', 'path': '.', 'recursive': True },
    'filter_glob': '*.log',
    'display_name': 'E2E Agent Logs'
}]
`;

    const response = await request.post(`${API_LOGSEEK_BASE}/settings/planners/scripts`, {
      data: {
        app: TEST_APP_AGENT,
        script
      }
    });
    expect(response.ok()).toBeTruthy();

    const scriptArchive = `
SOURCES = [{
    'endpoint': { 'kind': 'agent', 'agent_id': '${AGENT_ID}', 'subpath': 'logs' },
    'target':   { 'type': 'archive', 'path': 'agent-archive.tar' },
    'filter_glob': '*.log',
    'display_name': 'E2E Agent Archive'
}]
`;

    const responseArchive = await request.post(`${API_LOGSEEK_BASE}/settings/planners/scripts`, {
      data: {
        app: TEST_APP_AGENT_ARCHIVE,
        script: scriptArchive
      }
    });
    expect(responseArchive.ok()).toBeTruthy();

    const scriptTarGz = `
SOURCES = [{
    'endpoint': { 'kind': 'agent', 'agent_id': '${AGENT_ID}', 'subpath': 'logs' },
    'target':   { 'type': 'archive', 'path': 'agent-archive.tar.gz' },
    'filter_glob': '*.log',
    'display_name': 'E2E Agent TarGz'
}]
`;

    const responseTarGz = await request.post(`${API_LOGSEEK_BASE}/settings/planners/scripts`, {
      data: {
        app: TEST_APP_AGENT_TARGZ,
        script: scriptTarGz
      }
    });
    expect(responseTarGz.ok()).toBeTruthy();

    const scriptDirMultiGz = `
SOURCES = [{
    'endpoint': { 'kind': 'agent', 'agent_id': '${AGENT_ID}', 'subpath': 'logs' },
    'target':   { 'type': 'dir', 'path': '.', 'recursive': True },
    'filter_glob': '*.log.gz',
    'display_name': 'E2E Agent Dir Multi GZ'
}]
`;

    const responseDirMultiGz = await request.post(`${API_LOGSEEK_BASE}/settings/planners/scripts`, {
      data: {
        app: TEST_APP_AGENT_DIR_MULTI_GZ,
        script: scriptDirMultiGz
      }
    });
    expect(responseDirMultiGz.ok()).toBeTruthy();
  });

  test.afterAll(async ({ request }) => {
    try {
      await request.delete(`${API_LOGSEEK_BASE}/settings/planners/scripts/${TEST_APP_AGENT}`);
    } catch {
      // ignore
    }
    try {
      await request.delete(`${API_LOGSEEK_BASE}/settings/planners/scripts/${TEST_APP_AGENT_ARCHIVE}`);
    } catch {
      // ignore
    }
    try {
      await request.delete(`${API_LOGSEEK_BASE}/settings/planners/scripts/${TEST_APP_AGENT_TARGZ}`);
    } catch {
      // ignore
    }
    try {
      await request.delete(`${API_LOGSEEK_BASE}/settings/planners/scripts/${TEST_APP_AGENT_DIR_MULTI_GZ}`);
    } catch {
      // ignore
    }
    try {
      await request.delete(`${API_AGENT_BASE}/${AGENT_ID}`);
    } catch {
      // ignore
    }
    if (agentProc) {
      await stopProcess(agentProc);
    }
    if (webProc && startedWeb) {
      await stopProcess(webProc);
    }
    if (backendProc && startedBackend) {
      await stopProcess(backendProc);
    }
    fs.rmSync(TEST_ROOT_DIR, { recursive: true, force: true });
  });

  test('should search agent files using app: directive', async ({ page }) => {
    await page.goto('http://127.0.0.1:5173/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP_AGENT} "${UNI_ID_AGENT}"`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果', { timeout: 10000 });
    await expect(page.getByText(UNI_ID_AGENT)).toBeVisible();

    await expect(page.getByRole('button', { name: '远程代理' })).toBeVisible();
    await page.getByRole('button', { name: '远程代理' }).click();
    await expect(page.getByRole('button', { name: 'logs' })).toBeVisible();

    await expect(page.getByRole('link', { name: 'agent.log' })).toBeVisible();
  });

  test('should open agent file view and render content', async ({ page }) => {
    await page.goto('http://127.0.0.1:5173/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP_AGENT} "${UNI_ID_AGENT}"`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果', { timeout: 10000 });

    const popupPromise = page.waitForEvent('popup');
    await page.getByRole('link', { name: 'agent.log' }).click();
    const viewPage = await popupPromise;

    await viewPage.waitForURL(/\/view\?/);
    await expect(viewPage.getByRole('heading', { name: 'agent.log' })).toBeVisible({ timeout: 10000 });
    await expect(viewPage.locator('.code-content').getByText(UNI_ID_AGENT)).toBeVisible({ timeout: 10000 });

    await viewPage.close();
  });

  test('should search agent archive entries using app: directive', async ({ page }) => {
    await page.goto('http://127.0.0.1:5173/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP_AGENT_ARCHIVE} "${UNI_ID_AGENT_ARCHIVE}"`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果', { timeout: 10000 });
    await expect(page.getByText(UNI_ID_AGENT_ARCHIVE)).toBeVisible();

    const entryLink = page.getByRole('link', { name: 'archived.log' });
    await expect(entryLink).toHaveAttribute(
      'href',
      new RegExp(`file=ls%3A%2F%2Fagent%2F${encodeURIComponent(AGENT_ID)}%2Farchive%2F.*agent-archive\\.tar`)
    );
  });

  test('should open agent archive entry view and render content', async ({ page }) => {
    await page.goto('http://127.0.0.1:5173/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP_AGENT_ARCHIVE} "${UNI_ID_AGENT_ARCHIVE}"`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果', { timeout: 10000 });

    const popupPromise = page.waitForEvent('popup');
    await page.getByRole('link', { name: 'archived.log' }).click();
    const viewPage = await popupPromise;

    await viewPage.waitForURL(/\/view\?/);
    await expect(viewPage.getByRole('heading', { name: 'archived.log' })).toBeVisible({ timeout: 10000 });
    await expect(viewPage.locator('.code-content').getByText(UNI_ID_AGENT_ARCHIVE)).toBeVisible({ timeout: 10000 });

    await viewPage.close();
  });

  test('should search agent tar.gz archive entries using app: directive', async ({ page }) => {
    await page.goto('http://127.0.0.1:5173/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP_AGENT_TARGZ} "${UNI_ID_AGENT_TARGZ}"`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果', { timeout: 10000 });
    await expect(page.getByText(UNI_ID_AGENT_TARGZ)).toBeVisible();

    const entryLink = page.getByRole('link', { name: 'archived-tgz.log' });
    await expect(entryLink).toHaveAttribute(
      'href',
      new RegExp(`file=ls%3A%2F%2Fagent%2F${encodeURIComponent(AGENT_ID)}%2Farchive%2F.*agent-archive\\.tar\\.gz`)
    );
  });

  test('should open agent tar.gz archive entry view and render content', async ({ page }) => {
    await page.goto('http://127.0.0.1:5173/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP_AGENT_TARGZ} "${UNI_ID_AGENT_TARGZ}"`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果', { timeout: 10000 });

    const popupPromise = page.waitForEvent('popup');
    await page.getByRole('link', { name: 'archived-tgz.log' }).click();
    const viewPage = await popupPromise;

    await viewPage.waitForURL(/\/view\?/);
    await expect(viewPage.getByRole('heading', { name: 'archived-tgz.log' })).toBeVisible({ timeout: 10000 });
    await expect(viewPage.locator('.code-content').getByText(UNI_ID_AGENT_TARGZ)).toBeVisible({ timeout: 10000 });

    await viewPage.close();
  });

  test('should search multiple gz files in agent dir using app: directive', async ({ page }) => {
    await page.goto('http://127.0.0.1:5173/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP_AGENT_DIR_MULTI_GZ} "${UNI_ID_AGENT_DIR_MULTI_GZ}"`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText('2 个结果', { timeout: 10000 });
    await expect(page.locator('mark.highlight', { hasText: UNI_ID_AGENT_DIR_MULTI_GZ })).toHaveCount(2);

    await expect(page.getByRole('link', { name: 'multi-1.log.gz' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'multi-2.log.gz' })).toBeVisible();
  });
});
