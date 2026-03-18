import { test, expect, toLocalOrlForScript, toAgentOrlForScript, type APIRequestContext } from './fixtures';
import { spawn, type ChildProcessWithoutNullStreams } from 'child_process';
import * as fs from 'fs';
import * as http from 'http';
import * as net from 'net';
import * as path from 'path';
import { fileURLToPath } from 'url';
import * as zlib from 'zlib';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '../../..');
const webDir = path.join(repoRoot, 'web');
const backendDir = path.join(repoRoot, 'backend');

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
  // 不使用预编译二进制：避免 workspace 依赖（如 logseek）变更后，旧二进制导致 e2e 结果不一致。
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
  fs.writeFileSync(outFile, zlib.gzipSync(Buffer.concat(blocks)));
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

function startMockS3Server(opts: {
  bucket: string;
  objects: Record<string, Buffer>;
}): Promise<{ endpoint: string; close: () => Promise<void> }> {
  const { bucket, objects } = opts;
  const keys = Object.keys(objects).sort();

  const server = http.createServer((req, res) => {
    const url = new URL(req.url ?? '/', 'http://127.0.0.1');
    const pathParts = url.pathname.split('/').filter(Boolean);
    const bucketFromPath = pathParts[0] ?? null;

    // Connection verification calls ListObjectsV2(bucket, max_keys=1).
    const isListObjectsV2 =
      url.searchParams.get('list-type') === '2' || url.searchParams.get('x-id') === 'ListObjectsV2';
    if (req.method === 'GET' && isListObjectsV2 && bucketFromPath === bucket) {
      const prefix = url.searchParams.get('prefix') ?? '';
      const maxKeys = Number(url.searchParams.get('max-keys') ?? '1000');
      const matchedKeys = keys.filter((k) => k.startsWith(prefix)).slice(0, Math.max(1, maxKeys));

      const contentsXml = matchedKeys
        .map((k) => {
          const body = objects[k]!;
          return `  <Contents>
    <Key>${k}</Key>
    <LastModified>2025-01-01T00:00:00.000Z</LastModified>
    <ETag>"deadbeef"</ETag>
    <Size>${body.length}</Size>
    <StorageClass>STANDARD</StorageClass>
  </Contents>`;
        })
        .join('\n');

      const xml = `<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Name>${bucket}</Name>
  <Prefix>${prefix}</Prefix>
  <KeyCount>${matchedKeys.length}</KeyCount>
  <MaxKeys>${maxKeys}</MaxKeys>
  <IsTruncated>false</IsTruncated>
${contentsXml}
</ListBucketResult>`;

      res.writeHead(200, { 'Content-Type': 'application/xml; charset=utf-8' });
      res.end(xml);
      return;
    }

    if ((req.method === 'GET' || req.method === 'HEAD') && bucketFromPath === bucket) {
      const key = decodeURIComponent(pathParts.slice(1).join('/'));
      const body = objects[key];
      if (!body) {
        res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
        res.end('not found');
        return;
      }

      const range = req.headers.range;
      if (typeof range === 'string' && range.startsWith('bytes=')) {
        const m = /^bytes=(\d+)-(\d+)?$/.exec(range);
        if (m) {
          const start = Number(m[1]);
          const end = m[2] ? Number(m[2]) : body.length - 1;
          const chunk = body.subarray(start, end + 1);
          res.writeHead(206, {
            'Content-Type': 'application/octet-stream',
            'Content-Length': String(chunk.length),
            'Content-Range': `bytes ${start}-${end}/${body.length}`,
            ETag: '"deadbeef"'
          });
          if (req.method === 'HEAD') res.end();
          else res.end(chunk);
          return;
        }
      }

      res.writeHead(200, {
        'Content-Type': 'application/octet-stream',
        'Content-Length': String(body.length),
        ETag: '"deadbeef"'
      });
      if (req.method === 'HEAD') res.end();
      else res.end(body);
      return;
    }

    res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
    res.end('not found');
  });

  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const addr = server.address();
      if (!addr || typeof addr === 'string') {
        reject(new Error('Failed to bind mock S3 server'));
        return;
      }
      resolve({
        endpoint: `http://127.0.0.1:${addr.port}`,
        close: () =>
          new Promise((r) => {
            server.close(() => r());
          })
      });
    });
  });
}

test.describe('Mixed Sources Integration E2E', () => {
  test.describe.configure({ mode: 'serial' });

  const API_LOGSEEK_BASE = 'http://127.0.0.1:4001/api/v1/logseek';
  const API_AGENT_BASE = 'http://127.0.0.1:4001/api/v1/agents';

  const RUN_ID = Date.now();
  const APP = `e2e_test_mixed_${RUN_ID}`;
  const MARKER = `E2E_MIXED_${RUN_ID}`;

  const AGENT_ID = `e2e-agent-mixed-${RUN_ID}`;
  const PROFILE = `e2e_s3_mixed_${RUN_ID}`;
  const BUCKET = 'logs-bucket';

  const TEST_ROOT_DIR = path.join(__dirname, `temp_mixed_${RUN_ID}`);
  const LOCAL_ROOT_DIR = path.join(TEST_ROOT_DIR, 'local');
  const AGENT_ROOT_DIR = path.join(TEST_ROOT_DIR, 'agent');
  const AGENT_LOG_DIR = path.join(TEST_ROOT_DIR, 'agent_runtime_logs');
  const AGENT_LOGS_SUBPATH = 'logs';
  const AGENT_LOGS_DIR = path.join(AGENT_ROOT_DIR, AGENT_LOGS_SUBPATH);

  const LOCAL_DIR_SUBDIR = path.join(LOCAL_ROOT_DIR, 'dir');
  const LOCAL_DIR_FILE = path.join(LOCAL_DIR_SUBDIR, 'local-dir.log');
  const LOCAL_FILES_FILE = path.join(LOCAL_ROOT_DIR, 'local-files.log');
  const LOCAL_ARCHIVE_FILE = path.join(LOCAL_ROOT_DIR, 'local-archive.tar.gz');

  const AGENT_DIR_SUBDIR = path.join(AGENT_LOGS_DIR, 'dir');
  const AGENT_DIR_FILE = path.join(AGENT_DIR_SUBDIR, 'agent-dir.log');
  const AGENT_FILES_FILE = path.join(AGENT_LOGS_DIR, 'agent-files.log');
  const AGENT_ARCHIVE_FILE = path.join(AGENT_LOGS_DIR, 'agent-archive.tar.gz');

  const S3_ARCHIVE_KEY = 'archive/mixed.tar.gz';

  let backendProc: ChildProcessWithoutNullStreams | null = null;
  let startedBackend = false;
  let webProc: ChildProcessWithoutNullStreams | null = null;
  let startedWeb = false;
  let agentProc: ChildProcessWithoutNullStreams | null = null;
  let agentPort: number | null = null;
  let mockS3: { endpoint: string; close: () => Promise<void> } | null = null;

  test.beforeAll(async ({ request }) => {
    test.setTimeout(120000);
    const backend = await ensureBackendUp(request);
    backendProc = backend.proc;
    startedBackend = backend.started;

    const web = await ensureWebUp(request);
    webProc = web.proc;
    startedWeb = web.started;

    fs.mkdirSync(LOCAL_ROOT_DIR, { recursive: true });
    fs.mkdirSync(AGENT_LOGS_DIR, { recursive: true });
    fs.mkdirSync(AGENT_LOG_DIR, { recursive: true });
    fs.mkdirSync(LOCAL_DIR_SUBDIR, { recursive: true });
    fs.mkdirSync(AGENT_DIR_SUBDIR, { recursive: true });

    fs.writeFileSync(LOCAL_DIR_FILE, `2025-01-01 12:00:00 [INFO] local dir ${MARKER}\n`);
    fs.writeFileSync(LOCAL_FILES_FILE, `2025-01-01 12:00:00 [INFO] local files ${MARKER}\n`);
    writeTarGzFile(LOCAL_ARCHIVE_FILE, [
      {
        name: 'internal/local-archived.log',
        content: `2025-01-01 12:00:00 [INFO] local archive ${MARKER}\n`
      }
    ]);

    fs.writeFileSync(AGENT_DIR_FILE, `2025-01-01 12:00:00 [INFO] agent dir ${MARKER}\n`);
    fs.writeFileSync(AGENT_FILES_FILE, `2025-01-01 12:00:00 [INFO] agent files ${MARKER}\n`);
    writeTarGzFile(AGENT_ARCHIVE_FILE, [
      {
        name: 'internal/agent-archived.log',
        content: `2025-01-01 12:00:00 [INFO] agent archive ${MARKER}\n`
      }
    ]);

    const s3Archive = zlib.gzipSync(
      (() => {
        const tarBlocks: Buffer[] = [];

        const header = (name: string, size: number) => {
          const buf = Buffer.alloc(512, 0);
          const writeString = (offset: number, length: number, value: string) => {
            buf.write(value, offset, Math.min(length, Buffer.byteLength(value)), 'utf8');
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
          buf.fill(0x20, 148, 156);
          writeString(156, 1, '0');
          writeString(257, 6, 'ustar\0');
          writeString(263, 2, '00');

          let checksum = 0;
          for (const byte of buf) checksum += byte;
          const checksumStr = checksum.toString(8).padStart(6, '0') + '\0 ';
          writeString(148, 8, checksumStr);

          return buf;
        };

        const content = Buffer.from(`2025-01-01 12:00:00 [INFO] s3 archive ${MARKER}\n`, 'utf8');
        tarBlocks.push(header('archived-s3.log', content.length));
        tarBlocks.push(content);
        const remainder = content.length % 512;
        if (remainder !== 0) tarBlocks.push(Buffer.alloc(512 - remainder, 0));

        tarBlocks.push(Buffer.alloc(1024, 0));
        return Buffer.concat(tarBlocks);
      })()
    );

    mockS3 = await startMockS3Server({
      bucket: BUCKET,
      objects: { [S3_ARCHIVE_KEY]: s3Archive }
    });

    const profileResp = await request.post(`${API_LOGSEEK_BASE}/profiles`, {
      data: {
        profile_name: PROFILE,
        endpoint: mockS3.endpoint,
        bucket: BUCKET,
        access_key: 'test',
        secret_key: 'test'
      }
    });
    expect(profileResp.ok()).toBeTruthy();

    agentPort = await getFreePort();
    const { command, argsPrefix, cwd } = findAgentCommand();
    const args = [
      ...argsPrefix,
      '--agent-id',
      AGENT_ID,
      '--agent-name',
      'E2E Mixed Agent',
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

    // opsbox-agent 通过 `cargo run --release` 启动时，首次编译可能较慢（尤其是依赖 AWS SDK 时）
    await waitForHttpOk(request, `http://127.0.0.1:${agentPort}/health`, 120000);
    await waitForHttpOk(request, `${API_AGENT_BASE}/${AGENT_ID}`, 60000);

    // 当前后端支持的组合：
    // - Local: dir / files / archive
    // - Agent: dir / files / archive
    // - S3: archive（暂不支持 dir/files，EntryStreamFactory 会直接报错）
    const script = `
SOURCES = [
  "${toLocalOrlForScript(LOCAL_DIR_SUBDIR, '?glob=**/*.log')}",
  "${toLocalOrlForScript(LOCAL_FILES_FILE)}",
  "${toLocalOrlForScript(LOCAL_ARCHIVE_FILE, '?glob=**/*.log')}",
  "${toAgentOrlForScript(AGENT_ID, AGENT_DIR_SUBDIR, '?glob=**/*.log')}",
  "${toAgentOrlForScript(AGENT_ID, AGENT_FILES_FILE)}",
  "${toAgentOrlForScript(AGENT_ID, AGENT_ARCHIVE_FILE, '?glob=**/*.log')}",
  "orl://${PROFILE}@s3/${BUCKET}/${S3_ARCHIVE_KEY}?glob=**/*.log"
]
`;

    const scriptResp = await request.post(`${API_LOGSEEK_BASE}/settings/planners/scripts`, {
      data: { app: APP, script }
    });
    expect(scriptResp.ok()).toBeTruthy();
  });

  test.afterAll(async ({ request }) => {
    try {
      await request.delete(`${API_LOGSEEK_BASE}/settings/planners/scripts/${APP}`);
    } catch {
      // ignore
    }
    try {
      await request.delete(`${API_LOGSEEK_BASE}/profiles/${PROFILE}`);
    } catch {
      // ignore
    }

    if (mockS3) {
      await mockS3.close();
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

  test('should search across all supported endpoint/target combos', async ({ page }) => {
    await page.goto('http://127.0.0.1:5173/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${APP} "${MARKER}"`);
    await searchInput.press('Enter');

    // 每个 source 只放 1 个命中，合计 7 个结果。
    await expect(page.locator('.text-lg.font-semibold')).toContainText('7 个结果', { timeout: 15000 });

    // 快速 sanity：每类 endpoint 至少出一个按钮。
    await expect(page.getByRole('button', { name: '本地文件' })).toBeVisible();
    await expect(page.getByRole('button', { name: '远程代理' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'S3 云存储' })).toBeVisible();

    // 再确认几条关键文件名能被渲染出来（避免只验“结果数”漏掉展示问题）。
    await expect(page.getByRole('link', { name: 'local-dir.log' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'agent-dir.log' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'archived-s3.log' })).toBeVisible();
  });

  test('should filter results using path qualifiers across mixed sources', async ({ page }) => {
    await page.goto('http://127.0.0.1:5173/search');
    const searchInput = page.getByPlaceholder('搜索...');

    // 1. Filter by specific file pattern (Include) using path:
    // Both Local and Agent have a file ending in 'dir.log' ('local-dir.log', 'agent-dir.log')
    // and others do not ('local-files.log', 'agent-files.log').
    // Query: path:*dir.log
    await searchInput.fill(`app:${APP} "${MARKER}" path:*dir.log`);
    await searchInput.press('Enter');

    // Expected: local-dir.log and agent-dir.log.
    // Total 2 results.
    await expect(page.locator('.text-lg.font-semibold')).toContainText('2 个结果', { timeout: 15000 });
    await expect(page.getByRole('link', { name: 'local-dir.log' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'agent-dir.log' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'local-files.log' })).toBeHidden();
    await expect(page.getByRole('link', { name: 'archived-s3.log' })).toBeHidden();

    // 2. Exclude Agent files using -path:
    // Query: -path:**/*agent*
    // Filter out any path containing "agent".
    // Agent files: agent-dir.log, agent-files.log, internal/agent-archived.log
    // Local files: local-dir.log, local-files.log, internal/local-archived.log
    // S3: archived-s3.log
    await searchInput.fill(`app:${APP} "${MARKER}" -path:**/*agent*`);
    await searchInput.press('Enter');

    // Expected: 7 Total - 3 Agent = 4 Results (3 Local + 1 S3)
    await expect(page.locator('.text-lg.font-semibold')).toContainText('4 个结果', { timeout: 15000 });
    await expect(page.getByRole('link', { name: 'local-files.log' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'archived-s3.log' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'agent-files.log' })).toBeHidden();
  });
});
