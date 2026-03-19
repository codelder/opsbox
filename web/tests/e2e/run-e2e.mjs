import { rmSync, existsSync } from 'fs';
import { spawn } from 'child_process';
import path from 'path';
import { fileURLToPath } from 'url';

const RETRIES = 20;
const DELAY_MS = 500;
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const E2E_DATABASE_PATH = path.join(__dirname, 'opsbox-e2e.db');

function getE2EDatabaseArtifacts() {
  return [E2E_DATABASE_PATH, `${E2E_DATABASE_PATH}-wal`, `${E2E_DATABASE_PATH}-shm`];
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function cleanupDatabaseArtifacts() {
  for (const dbPath of getE2EDatabaseArtifacts()) {
    for (let attempt = 0; attempt < RETRIES; attempt += 1) {
      try {
        if (!existsSync(dbPath)) break;
        rmSync(dbPath, { force: true });
        break;
      } catch (error) {
        const code = error && typeof error === 'object' ? error.code : undefined;
        const retryable = code === 'EBUSY' || code === 'EPERM';
        if (!retryable || attempt === RETRIES - 1) {
          console.error(`[E2E Runner] Failed to remove ${dbPath}: ${error.message}`);
          break;
        }
        await sleep(DELAY_MS);
      }
    }
  }
}

const quotedArgs = process.argv
  .slice(2)
  .map((arg) => JSON.stringify(arg))
  .join(' ');
const command = `pnpm exec playwright test${quotedArgs ? ` ${quotedArgs}` : ''}`;
const child = spawn(command, {
  shell: true,
  stdio: 'inherit',
  env: process.env
});

child.on('exit', async (code, signal) => {
  await cleanupDatabaseArtifacts();

  if (signal) {
    process.kill(process.pid, signal);
    return;
  }

  process.exit(code ?? 1);
});
