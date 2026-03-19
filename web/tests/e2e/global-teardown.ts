/**
 * Global Teardown for E2E Tests
 *
 * Cleans up any resources that may have been left behind due to:
 * - Test failures
 * - Test timeouts
 * - Process crashes
 * - Interrupted test runs
 *
 * This module also exports cleanup functions that can be called at startup
 * to ensure cleanup even if the previous run was interrupted.
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync, spawn } from 'child_process';
import { fileURLToPath } from 'url';
import { getE2EDatabaseArtifacts } from './e2e-env';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Prefixes for temporary directories created by e2e tests
const TEMP_DIR_PREFIXES = ['temp_', 'e2e_test_', 'e2e_temp_'];

// Maximum age (in ms) for temp directories before cleanup (30 seconds)
// Reduced from 5 minutes to ensure faster cleanup in CI
const MAX_AGE_MS = 30 * 1000;
const DB_CLEANUP_RETRIES = 8;
const DB_CLEANUP_DELAY_MS = 250;

function sleepSync(ms: number): void {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

/**
 * Kill orphaned processes
 */
function killOrphanedProcesses(): void {
  if (process.platform === 'win32') {
    // Kill the dedicated backend server if it is still listening on the e2e port.
    try {
      const result = execSync(
        'powershell -NoProfile -Command "Get-NetTCPConnection -LocalPort 4001 -State Listen -ErrorAction SilentlyContinue | Select-Object -ExpandProperty OwningProcess -Unique"',
        {
          encoding: 'utf-8',
          stdio: ['pipe', 'pipe', 'pipe']
        }
      );
      const pids = result.trim().split(/\s+/).filter(Boolean);
      for (const pid of pids) {
        try {
          execSync(`taskkill /PID ${pid} /F /T`, { stdio: 'ignore' });
          console.log(`[Cleanup] Stopped listener on port 4001 (PID ${pid})`);
        } catch {
          // ignore
        }
      }
    } catch {
      // ignore
    }

    // Clean up any leftover agent processes compiled for e2e runs.
    try {
      execSync('taskkill /IM opsbox-agent.exe /F /T', { stdio: 'ignore' });
      console.log('[Cleanup] Stopped orphaned opsbox-agent.exe processes');
    } catch {
      // ignore
    }

    return;
  }

  // 1. Kill any orphaned opsbox-agent processes
  try {
    const result = execSync('pgrep -f "opsbox-agent" 2>/dev/null || true', {
      encoding: 'utf-8',
      stdio: ['pipe', 'pipe', 'pipe']
    });
    const pids = result.trim().split('\n').filter(Boolean);
    if (pids.length > 0) {
      console.log(`[Cleanup] Found ${pids.length} orphaned agent process(es), terminating...`);
      try {
        execSync('pkill -f "opsbox-agent" 2>/dev/null || true', { stdio: 'ignore' });
      } catch {
        // ignore
      }
    }
  } catch {
    // pgrep might not exist on all systems
  }

  // 2. Kill any orphaned opsbox-server processes on non-standard ports
  try {
    execSync('pkill -f "opsbox-server.*--port.*400[2-9]" 2>/dev/null || true', { stdio: 'ignore' });
  } catch {
    // ignore
  }
}

/**
 * Calculate total size of a directory
 */
function getDirectorySize(dir: string): number {
  let size = 0;
  try {
    const entries = fs.readdirSync(dir, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        size += getDirectorySize(fullPath);
      } else if (entry.isFile()) {
        try {
          const stats = fs.statSync(fullPath);
          size += stats.size;
        } catch {
          // ignore
        }
      }
    }
  } catch {
    // ignore
  }
  return size;
}

/**
 * Format bytes to human readable string
 */
function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

/**
 * Clean up temporary directories
 * @param forceAll If true, clean all temp dirs regardless of age
 */
function cleanupTempDirectories(forceAll = false): { cleaned: number; size: number } {
  let cleanedDirs = 0;
  let cleanedSize = 0;

  try {
    const entries = fs.readdirSync(__dirname, { withFileTypes: true });

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;

      const name = entry.name;
      const fullPath = path.join(__dirname, name);

      // Check if it matches any temp prefix
      const isTempDir = TEMP_DIR_PREFIXES.some((prefix) => name.startsWith(prefix));
      if (!isTempDir) continue;

      try {
        const stats = fs.statSync(fullPath);
        const age = Date.now() - stats.mtimeMs;

        // Clean if forceAll or if directory is old enough
        if (forceAll || age > MAX_AGE_MS) {
          const size = getDirectorySize(fullPath);
          cleanedSize += size;

          fs.rmSync(fullPath, { recursive: true, force: true });
          cleanedDirs++;
          console.log(`[Cleanup] Removed: ${name} (${formatBytes(size)})`);
        }
      } catch (e) {
        console.error(`[Cleanup] Failed to remove ${name}:`, (e as Error).message);
      }
    }
  } catch (e) {
    console.error('[Cleanup] Error reading directory:', (e as Error).message);
  }

  return { cleaned: cleanedDirs, size: cleanedSize };
}

function cleanupE2EDatabase(): number {
  let removed = 0;

  for (const dbPath of getE2EDatabaseArtifacts()) {
    for (let attempt = 0; attempt < DB_CLEANUP_RETRIES; attempt++) {
      try {
        if (!fs.existsSync(dbPath)) break;
        fs.rmSync(dbPath, { force: true });
        removed++;
        console.log(`[Cleanup] Removed database artifact: ${path.basename(dbPath)}`);
        break;
      } catch (e) {
        const error = e as NodeJS.ErrnoException;
        const isRetryable = error.code === 'EBUSY' || error.code === 'EPERM';
        const isLastAttempt = attempt === DB_CLEANUP_RETRIES - 1;

        if (!isRetryable || isLastAttempt) {
          if (isRetryable) {
            scheduleDeferredDatabaseCleanup(dbPath);
            console.log(`[Cleanup] Deferred database artifact removal: ${path.basename(dbPath)}`);
          } else {
            console.error(`[Cleanup] Failed to remove database artifact ${path.basename(dbPath)}:`, error.message);
          }
          break;
        }

        sleepSync(DB_CLEANUP_DELAY_MS);
      }
    }
  }

  return removed;
}

function scheduleDeferredDatabaseCleanup(dbPath: string): void {
  try {
    if (process.platform === 'win32') {
      const child = spawn('cmd.exe', ['/d', '/c', `ping 127.0.0.1 -n 4 >nul && del /f /q "${dbPath}"`], {
        detached: true,
        stdio: 'ignore'
      });
      child.unref();
      return;
    }

    const child = spawn('sh', ['-c', `sleep 1.5; rm -f ${JSON.stringify(dbPath)}`], {
      detached: true,
      stdio: 'ignore'
    });
    child.unref();
  } catch {
    // ignore deferred cleanup failures
  }
}

/**
 * Full cleanup routine - can be called at startup or teardown
 */
export function performCleanup(forceAll = false, cleanupDatabase = true, killProcesses = true): void {
  console.log('\n[Cleanup] Starting...');

  // 1. Kill orphaned processes
  if (killProcesses) {
    killOrphanedProcesses();
  }

  // 2. Clean temp directories
  const { cleaned, size } = cleanupTempDirectories(forceAll);

  // 3. Remove the dedicated e2e database artifacts
  const removedDbArtifacts = cleanupDatabase ? cleanupE2EDatabase() : 0;

  if (cleaned > 0) {
    console.log(`[Cleanup] Cleaned ${cleaned} temp directories, freed ${formatBytes(size)}`);
  } else {
    console.log('[Cleanup] No temp directories to clean');
  }

  if (removedDbArtifacts > 0) {
    console.log(`[Cleanup] Removed ${removedDbArtifacts} e2e database artifact(s)`);
  }

  console.log('[Cleanup] Completed\n');
}

/**
 * Global teardown function called by Playwright
 */
async function globalTeardown() {
  console.log('\n[Global Teardown] Starting cleanup...');

  // Clean temp directories
  const { cleaned, size } = cleanupTempDirectories(false);
  const removedDbArtifacts = cleanupE2EDatabase();

  if (cleaned > 0) {
    console.log(`[Global Teardown] Cleaned ${cleaned} temp directories, freed ${formatBytes(size)}`);
  } else {
    console.log('[Global Teardown] No temp directories to clean');
  }

  if (removedDbArtifacts > 0) {
    console.log(`[Global Teardown] Removed ${removedDbArtifacts} e2e database artifact(s)`);
  }

  console.log('[Global Teardown] Completed\n');
}

export default globalTeardown;
