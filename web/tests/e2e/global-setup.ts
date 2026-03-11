/**
 * Global Setup for E2E Tests
 *
 * This runs BEFORE all tests and ensures:
 * 1. Cleanup of any leftover resources from previous (interrupted) runs
 * 2. Verification that required dependencies are available
 * 3. Pre-compilation of agent binary to avoid parallel compilation contention
 *
 * This is the safety net for cases where the previous test run was
 * interrupted (Ctrl+C, timeout, crash) and didn't run global-teardown.
 */

import { execSync } from 'child_process';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { performCleanup } from './global-teardown';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function globalSetup() {
  console.log('\n[Global Setup] Starting...');

  // 1. Clean up any leftover resources from previous runs
  // Force cleanup of ALL temp directories (not just old ones)
  // because if we're starting a new run, any existing temp dirs are stale
  performCleanup(true, false);

  // 2. Verify required commands are available
  const requiredCommands = ['node', 'pnpm'];
  for (const cmd of requiredCommands) {
    try {
      // Use `which` on Unix, `where` on Windows
      const whichCmd = process.platform === 'win32' ? 'where' : 'which';
      execSync(`${whichCmd} ${cmd}`, { stdio: 'ignore' });
    } catch {
      console.warn(`[Global Setup] Warning: '${cmd}' command not found in PATH`);
    }
  }

  // 3. Pre-compile the agent binary to avoid parallel compilation contention
  // When tests run in parallel, multiple tests may try to compile the agent
  // simultaneously, causing file lock contention in Cargo's build directory.
  // Pre-compiling once ensures all tests can run `cargo run` without waiting.
  console.log('[Global Setup] Pre-compiling opsbox-agent...');
  const repoRoot = path.resolve(__dirname, '../../..');
  const backendDir = path.join(repoRoot, 'backend');

  try {
    // Use --release for consistency with how tests run the agent
    const startTime = Date.now();
    execSync('cargo build --release -p opsbox-agent', {
      cwd: backendDir,
      stdio: 'inherit', // Show compilation output
      timeout: 300000, // 5 minutes timeout for compilation
      env: { ...process.env, RUST_LOG: 'info' }
    });
    const elapsed = Date.now() - startTime;
    console.log(`[Global Setup] Agent compiled successfully in ${elapsed}ms`);
  } catch (error) {
    console.warn('[Global Setup] Warning: Failed to pre-compile agent:', error);
    // Don't throw - tests will still try to compile on demand
  }

  console.log('[Global Setup] Completed\n');
}

export default globalSetup;
