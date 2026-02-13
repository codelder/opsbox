/**
 * Global Setup for E2E Tests
 *
 * This runs BEFORE all tests and ensures:
 * 1. Cleanup of any leftover resources from previous (interrupted) runs
 * 2. Verification that required dependencies are available
 *
 * This is the safety net for cases where the previous test run was
 * interrupted (Ctrl+C, timeout, crash) and didn't run global-teardown.
 */

import { performCleanup } from './global-teardown';

async function globalSetup() {
  console.log('\n[Global Setup] Starting...');

  // 1. Clean up any leftover resources from previous runs
  // Force cleanup of ALL temp directories (not just old ones)
  // because if we're starting a new run, any existing temp dirs are stale
  performCleanup(true);

  // 2. Verify required commands are available
  const requiredCommands = ['node', 'pnpm'];
  for (const cmd of requiredCommands) {
    try {
      // Use `which` on Unix, `where` on Windows
      const whichCmd = process.platform === 'win32' ? 'where' : 'which';
      require('child_process').execSync(`${whichCmd} ${cmd}`, { stdio: 'ignore' });
    } catch {
      console.warn(`[Global Setup] Warning: '${cmd}' command not found in PATH`);
    }
  }

  console.log('[Global Setup] Completed\n');
}

export default globalSetup;
