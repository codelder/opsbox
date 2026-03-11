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
import { execSync } from 'child_process';
import { fileURLToPath } from 'url';
import { getE2EDatabaseArtifacts } from './e2e-env';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Prefixes for temporary directories created by e2e tests
const TEMP_DIR_PREFIXES = ['temp_', 'e2e_test_', 'e2e_temp_'];

// Maximum age (in ms) for temp directories before cleanup (30 seconds)
// Reduced from 5 minutes to ensure faster cleanup in CI
const MAX_AGE_MS = 30 * 1000;

/**
 * Kill orphaned processes
 */
function killOrphanedProcesses(): void {
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
    try {
      if (!fs.existsSync(dbPath)) continue;
      fs.rmSync(dbPath, { force: true });
      removed++;
      console.log(`[Cleanup] Removed database artifact: ${path.basename(dbPath)}`);
    } catch (e) {
      console.error(`[Cleanup] Failed to remove database artifact ${path.basename(dbPath)}:`, (e as Error).message);
    }
  }

  return removed;
}

/**
 * Full cleanup routine - can be called at startup or teardown
 */
export function performCleanup(forceAll = false, cleanupDatabase = true): void {
  console.log('\n[Cleanup] Starting...');

  // 1. Kill orphaned processes
  killOrphanedProcesses();

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

  // Kill orphaned processes
  killOrphanedProcesses();

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
