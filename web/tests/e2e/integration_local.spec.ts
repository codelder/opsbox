import { test, expect } from '@playwright/test';
import * as fs from 'fs';
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

    // checksum: treat this field as spaces during calculation
    header.fill(0x20, 148, 156);
    writeString(156, 1, '0'); // regular file
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

  // end of archive: two 512-byte blocks of zeros
  blocks.push(Buffer.alloc(1024, 0));
  fs.writeFileSync(outFile, Buffer.concat(blocks));
}

function writeGzFile(outFile: string, content: string) {
  const compressed = zlib.gzipSync(Buffer.from(content, 'utf8'));
  fs.writeFileSync(outFile, compressed);
}

test.describe('Local Integration E2E', () => {
  // 该文件在 playwright fullyParallel=true 时可能会被多个 worker 并发执行。
  // 用例会写入/删除本地文件与后端 planner 脚本，因此这里强制串行，且使用唯一目录避免互相干扰。
  test.describe.configure({ mode: 'serial' });

  const RUN_ID = Date.now();
  const TEST_LOG_DIR = path.join(__dirname, `temp_logs_${RUN_ID}`);
  const TEST_LOG_FILE = path.join(TEST_LOG_DIR, 'e2e.log');
  const TEST_ARCHIVE_FILE = path.join(TEST_LOG_DIR, 'e2e-archive.tar');
  const TEST_GZ_FILE = path.join(TEST_LOG_DIR, 'e2e-compressed.log.gz');
  const API_BASE = 'http://127.0.0.1:4001/api/v1/logseek';
  const TEST_APP_DIR = `e2e_test_dir_${RUN_ID}`;
  const TEST_APP_ARCHIVE = `e2e_test_archive_${RUN_ID}`;
  const TEST_APP_GZ = `e2e_test_gz_${RUN_ID}`;
  const TEST_APP_DIR_GZ = `e2e_test_dir_gz_${RUN_ID}`;
  const UNI_ID_DIR = `E2E_DIR_${RUN_ID}`;
  const UNI_ID_ARCHIVE = `E2E_ARCHIVE_${RUN_ID}`;
  const UNI_ID_GZ = `E2E_GZ_${RUN_ID}`;
  const UNI_ID_DIR_GZ = `E2E_DIR_GZ_${RUN_ID}`;

  test.beforeAll(async ({ request }) => {
    // 1. Create temp log directory and file
    if (!fs.existsSync(TEST_LOG_DIR)) {
      fs.mkdirSync(TEST_LOG_DIR, { recursive: true });
    }

    fs.writeFileSync(TEST_LOG_FILE, `2025-01-01 12:00:00 [INFO] Test log entry ${UNI_ID_DIR}\n`);
    writeTarFile(TEST_ARCHIVE_FILE, [
      {
        name: 'internal/archive.log',
        content: `2025-01-01 12:00:00 [INFO] Archived log entry ${UNI_ID_ARCHIVE}\n`
      }
    ]);
    writeGzFile(
      TEST_GZ_FILE,
      `2025-01-01 12:00:00 [INFO] Gzipped log entry ${UNI_ID_GZ}\n2025-01-01 12:00:01 [WARN] Another line in gz file\n`
    );
    // Create another gz file in the directory for dir target test
    const TEST_GZ_FILE_IN_DIR = path.join(TEST_LOG_DIR, 'e2e-dir-gz.log.gz');
    writeGzFile(
      TEST_GZ_FILE_IN_DIR,
      `2025-01-01 12:00:00 [INFO] Gzipped log entry in dir ${UNI_ID_DIR_GZ}\n2025-01-01 12:00:01 [WARN] Another line in dir gz file\n`
    );

    // 2. Prepare Planner Script
    // Must use absolute path for endpoint.root
    const absRoot = path.resolve(TEST_LOG_DIR);

    const scriptDir = `
SOURCES = [{
    'endpoint': { 'kind': 'local', 'root': '${absRoot}' },
    'target':   { 'type': 'dir', 'path': '.', 'recursive': True },
    'filter_glob': '*.log',
    'display_name': 'E2E Test Logs'
}]
`;

    const scriptArchive = `
SOURCES = [{
    'endpoint': { 'kind': 'local', 'root': '${absRoot}' },
    'target':   { 'type': 'archive', 'path': 'e2e-archive.tar' },
    'filter_glob': '**/*.log',
    'display_name': 'E2E Test Archive'
}]
`;

    const scriptGz = `
SOURCES = [{
    'endpoint': { 'kind': 'local', 'root': '${absRoot}' },
    'target':   { 'type': 'archive', 'path': 'e2e-compressed.log.gz' },
    'display_name': 'E2E Test GZ'
}]
`;

    const scriptDirGz = `
SOURCES = [{
    'endpoint': { 'kind': 'local', 'root': '${absRoot}' },
    'target':   { 'type': 'dir', 'path': '.', 'recursive': True },
    'filter_glob': '*.gz',
    'display_name': 'E2E Test Dir with GZ'
}]
`;

    const responseDir = await request.post(`${API_BASE}/settings/planners/scripts`, {
      data: {
        app: TEST_APP_DIR,
        script: scriptDir
      }
    });
    expect(responseDir.ok()).toBeTruthy();

    const responseArchive = await request.post(`${API_BASE}/settings/planners/scripts`, {
      data: {
        app: TEST_APP_ARCHIVE,
        script: scriptArchive
      }
    });
    expect(responseArchive.ok()).toBeTruthy();

    const responseGz = await request.post(`${API_BASE}/settings/planners/scripts`, {
      data: {
        app: TEST_APP_GZ,
        script: scriptGz
      }
    });
    expect(responseGz.ok()).toBeTruthy();

    const responseDirGz = await request.post(`${API_BASE}/settings/planners/scripts`, {
      data: {
        app: TEST_APP_DIR_GZ,
        script: scriptDirGz
      }
    });
    expect(responseDirGz.ok()).toBeTruthy();
  });

  test.afterAll(async ({ request }) => {
    // Cleanup: Delete Script
    try {
      await request.delete(`${API_BASE}/settings/planners/scripts/${TEST_APP_DIR}`);
    } catch {
      // Ignore cleanup errors
    }
    try {
      await request.delete(`${API_BASE}/settings/planners/scripts/${TEST_APP_ARCHIVE}`);
    } catch {
      // Ignore cleanup errors
    }
    try {
      await request.delete(`${API_BASE}/settings/planners/scripts/${TEST_APP_GZ}`);
    } catch {
      // Ignore cleanup errors
    }
    try {
      await request.delete(`${API_BASE}/settings/planners/scripts/${TEST_APP_DIR_GZ}`);
    } catch {
      // Ignore cleanup errors
    }

    // Cleanup: Delete Files
    fs.rmSync(TEST_LOG_DIR, { recursive: true, force: true });
  });

  test('should search real local files using app: directive', async ({ page }) => {
    await page.goto('/search');

    // Type search query with app directive
    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP_DIR} "${UNI_ID_DIR}"`);
    await searchInput.press('Enter');

    // Wait for results
    // Since we are using a real backend, it might take a moment
    await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果', { timeout: 10000 });

    // Verify Result Card Content (Primary Goal)
    await expect(page.getByText(UNI_ID_DIR)).toBeVisible();
    await expect(page.getByRole('link', { name: 'e2e.log' })).toBeVisible();

    // Verify Sidebar
    // "Local" endpoint type should be visible
    await expect(page.getByRole('button', { name: '本地文件' })).toBeVisible();
  });

  test('should search real local archive entries using app: directive', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP_ARCHIVE} "${UNI_ID_ARCHIVE}"`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果', { timeout: 10000 });
    await expect(page.getByText(UNI_ID_ARCHIVE)).toBeVisible();

    const archiveLink = page.getByRole('link', { name: 'archive.log' });
    await expect(archiveLink).toHaveAttribute('href', /file=odfi%3A%2F%2Flocal%2F.*archive\.tar/);

    await expect(page.getByRole('button', { name: '本地文件' })).toBeVisible();
  });

  test('should search real local gz file entries using app: directive', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP_GZ} "${UNI_ID_GZ}"`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果', { timeout: 10000 });
    await expect(page.getByText(UNI_ID_GZ)).toBeVisible();

    const gzLink = page.getByRole('link', { name: /e2e-compressed\.log(\.gz)?/ });
    await expect(gzLink).toHaveAttribute('href', /file=odfi%3A%2F%2Flocal%2F.*e2e-compressed\.log\.gz/);

    await expect(page.getByRole('button', { name: '本地文件' })).toBeVisible();
  });

  test('should search gz files in local directory using app: directive', async ({ page }) => {
    await page.goto('/search');

    const searchInput = page.getByPlaceholder('搜索...');
    await searchInput.fill(`app:${TEST_APP_DIR_GZ} "${UNI_ID_DIR_GZ}"`);
    await searchInput.press('Enter');

    await expect(page.locator('.text-lg.font-semibold')).toContainText('1 个结果', { timeout: 10000 });
    await expect(page.getByText(UNI_ID_DIR_GZ)).toBeVisible();

    // 目录扫描场景下，gz 文件仍以普通文件 URL（dir）展示，文件名会包含 .gz
    const gzLink = page.getByRole('link', { name: /e2e-dir-gz\.log(\.gz)?/ });
    await expect(gzLink).toHaveAttribute('href', /file=odfi%3A%2F%2Flocal%2F.*e2e-dir-gz\.log/);

    await expect(page.getByRole('button', { name: '本地文件' })).toBeVisible();
  });
});
