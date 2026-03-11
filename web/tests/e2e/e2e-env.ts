import * as path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export const E2E_DATABASE_PATH = path.join(__dirname, 'opsbox-e2e.db');

export function getE2EDatabaseArtifacts(): string[] {
  return [E2E_DATABASE_PATH, `${E2E_DATABASE_PATH}-wal`, `${E2E_DATABASE_PATH}-shm`];
}
