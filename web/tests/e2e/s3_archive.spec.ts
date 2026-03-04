import { test, expect, type APIRequestContext } from '@playwright/test';
import { spawn, type ChildProcessWithoutNullStreams } from 'child_process';
import * as fs from 'fs';
import * as http from 'http';
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

async function ensureBackendUp(request: APIRequestContext) {
  try {
    await waitForHttpOk(request, 'http://127.0.0.1:4001/healthy', 1000);
    return { started: false as const, proc: null as ChildProcessWithoutNullStreams | null };
  } catch {
    // fallthrough: start it
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
    // fallthrough: start it
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

function createTarBuffer(entries: Array<{ name: string; content: string }>) {
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
  return Buffer.concat(blocks);
}

function createTarGzBuffer(entries: Array<{ name: string; content: string }>) {
  return zlib.gzipSync(createTarBuffer(entries));
}

function startMockS3Server(opts: {
  bucket: string;
  key: string;
  body: Buffer;
}): Promise<{ endpoint: string; close: () => Promise<void> }> {
  const { bucket, key, body } = opts;

  const server = http.createServer((req, res) => {
    const url = new URL(req.url ?? '/', 'http://127.0.0.1');
    const host = (req.headers.host ?? '').split(':')[0];

    const pathParts = url.pathname.split('/').filter(Boolean);
    const bucketFromPath = pathParts[0] ?? null;
    const bucketFromHost = host.startsWith(`${bucket}.`) ? bucket : null;
    const effectiveBucket = bucketFromPath === bucket ? bucket : bucketFromHost;

    const isListObjectsV2 =
      url.searchParams.get('list-type') === '2' || url.searchParams.get('x-id') === 'ListObjectsV2';
    if (req.method === 'GET' && isListObjectsV2 && effectiveBucket === bucket) {
      const prefix = url.searchParams.get('prefix') ?? '';
      const maxKeys = url.searchParams.get('max-keys') ?? '1';
      const xml = `<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Name>${bucket}</Name>
  <Prefix>${prefix}</Prefix>
  <KeyCount>1</KeyCount>
  <MaxKeys>${maxKeys}</MaxKeys>
  <IsTruncated>false</IsTruncated>
  <Contents>
    <Key>${key}</Key>
    <LastModified>2025-01-01T00:00:00.000Z</LastModified>
    <ETag>"deadbeef"</ETag>
    <Size>${body.length}</Size>
    <StorageClass>STANDARD</StorageClass>
  </Contents>
</ListBucketResult>`;
      res.writeHead(200, { 'Content-Type': 'application/xml; charset=utf-8' });
      res.end(xml);
      return;
    }

    if (effectiveBucket === bucket && (req.method === 'GET' || req.method === 'HEAD')) {
      const effectiveKey = decodeURIComponent((bucketFromPath === bucket ? pathParts.slice(1) : pathParts).join('/'));
      if (effectiveKey === key) {
        const range = req.headers.range;
        if (typeof range === 'string' && range.startsWith('bytes=')) {
          const m = /^bytes=(\d+)-(\d+)?$/.exec(range);
          if (m) {
            const start = Number(m[1]);
            const end = m[2] ? Number(m[2]) : body.length - 1;
            const chunk = body.subarray(start, end + 1);
            res.writeHead(206, {
              'Content-Type': 'application/gzip',
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
          'Content-Type': 'application/gzip',
          'Content-Length': String(body.length),
          ETag: '"deadbeef"'
        });
        if (req.method === 'HEAD') res.end();
        else res.end(body);
        return;
      }
    }

    if (effectiveBucket === bucket && req.method === 'HEAD' && bucketFromPath === bucket && pathParts.length === 1) {
      res.writeHead(200, { ETag: '"deadbeef"' });
      res.end();
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

test.describe('S3 Archive E2E', () => {
  test.describe.configure({ mode: 'serial' });

  const API_LOGSEEK_BASE = 'http://127.0.0.1:4001/api/v1/logseek';
  const RUN_ID = Date.now();

  const PROFILE = `e2e_s3_${RUN_ID}`;
  const BUCKET = 'logs-bucket';
  const KEY = '2025/01/app.tar.gz';
  const TEST_APP = `e2e_test_s3_${RUN_ID}`;
  const UNI_ID = `E2E_S3_${RUN_ID}`;

  let backendProc: ChildProcessWithoutNullStreams | null = null;
  let webProc: ChildProcessWithoutNullStreams | null = null;
  let startedBackend = false;
  let startedWeb = false;
  let mockS3: { endpoint: string; close: () => Promise<void> } | null = null;

  test.beforeAll(async ({ request }) => {
    test.setTimeout(120000);
    const backend = await ensureBackendUp(request);
    backendProc = backend.proc;
    startedBackend = backend.started;

    const web = await ensureWebUp(request);
    webProc = web.proc;
    startedWeb = web.started;

    const tarGz = createTarGzBuffer([
      {
        name: 'archived-tgz.log',
        content: `2025-01-01 00:00:00 [panic] something bad happened ${UNI_ID}\n`
      }
    ]);
    mockS3 = await startMockS3Server({ bucket: BUCKET, key: KEY, body: tarGz });

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

    const script = `
SOURCES = ["orl://${PROFILE}@s3/${BUCKET}/${KEY}?glob=*.log"]
`;

    const scriptResp = await request.post(`${API_LOGSEEK_BASE}/settings/planners/scripts`, {
      data: { app: TEST_APP, script }
    });
    expect(scriptResp.ok()).toBeTruthy();
  });

  test.afterAll(async ({ request }) => {
    try {
      await request.delete(`${API_LOGSEEK_BASE}/settings/planners/scripts/${TEST_APP}`);
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
    if (webProc && startedWeb) {
      await stopProcess(webProc);
    }
    if (backendProc && startedBackend) {
      await stopProcess(backendProc);
    }
  });

  test('should render s3 tar.gz archive entry results', async ({ page, request }) => {
    // 先直连后端验证搜索接口（能拿到 Problem Details 的详细错误，避免只看到 UI 的 HTTP 500）
    const q = `app:${TEST_APP} "${UNI_ID}"`;
    const apiResp = await request.post(`${API_LOGSEEK_BASE}/search.ndjson`, {
      data: { q },
      timeout: 15000
    });
    if (!apiResp.ok()) {
      const detail = await apiResp.text();
      throw new Error(`Backend /search.ndjson failed: HTTP ${apiResp.status()} body=${detail}`);
    }

    await page.goto('http://127.0.0.1:5173/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(q);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果');
    await expect(page.getByRole('button', { name: 'S3 云存储' })).toBeVisible();

    const entryLink = page.getByRole('link', { name: 'archived-tgz.log' });
    await expect(entryLink).toBeVisible();
    await expect(entryLink).toHaveAttribute(
      'href',
      /file=orl%3A%2F%2F[^%]+%40s3%2F.*app\.tar\.gz%3Fentry%3Darchived.*?tgz.*?log/
    );
    await expect(page.locator('mark.highlight', { hasText: UNI_ID })).toHaveCount(1);
  });
});
