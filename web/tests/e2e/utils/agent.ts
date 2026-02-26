/**
 * Agent Test Utilities
 *
 * Shared utilities for E2E tests that need to spawn and manage opsbox-agent processes.
 * This module centralizes agent-related functionality to ensure consistency across tests.
 */

import { spawn, type ChildProcessWithoutNullStreams } from 'child_process';
import * as net from 'net';
import * as path from 'path';
import * as fs from 'fs';
import * as zlib from 'zlib';
import type { APIRequestContext } from '@playwright/test';

/**
 * Default timeout for agent readiness (30 seconds)
 * Increased from 15s to handle compilation delays in parallel test runs
 */
export const DEFAULT_AGENT_READY_TIMEOUT = 30000;

/**
 * Default interval for polling agent readiness
 */
export const DEFAULT_AGENT_READY_INTERVAL = 500;

/**
 * Get a free port on the system
 */
export function getFreePort(): Promise<number> {
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

/**
 * Find the agent command for the current repository
 */
export function findAgentCommand(repoRoot: string): { command: string; argsPrefix: string[]; cwd: string } {
  const backendDir = path.join(repoRoot, 'backend');
  return {
    command: 'cargo',
    argsPrefix: ['run', '--release', '-p', 'opsbox-agent', '--'],
    cwd: backendDir
  };
}

/**
 * Stop a process gracefully with timeout
 */
export async function stopProcess(proc: ChildProcessWithoutNullStreams, timeout = 5000): Promise<void> {
  if (proc.exitCode !== null) return;

  proc.kill('SIGINT');

  const exited = await Promise.race([
    new Promise<boolean>((resolve) => proc.once('exit', () => resolve(true))),
    new Promise<boolean>((resolve) => setTimeout(() => resolve(false), timeout))
  ]);

  if (exited) return;

  proc.kill('SIGKILL');
  await new Promise<void>((resolve) => proc.once('exit', () => resolve()));
}

/**
 * Wait for agent to register to server via API check
 *
 * @param request - Playwright APIRequestContext
 * @param agentId - The agent ID to wait for
 * @param maxWait - Maximum wait time in milliseconds (default: 30s)
 * @param interval - Polling interval in milliseconds (default: 500ms)
 * @throws Error if agent is not ready within the timeout
 */
export async function waitForAgentReady(
  request: APIRequestContext,
  agentId: string,
  maxWait = DEFAULT_AGENT_READY_TIMEOUT,
  interval = DEFAULT_AGENT_READY_INTERVAL
): Promise<void> {
  const start = Date.now();

  while (Date.now() - start < maxWait) {
    try {
      const response = await request.get(`http://127.0.0.1:4001/api/v1/agents/${agentId}`);
      if (response.ok()) {
        console.log(`Agent ${agentId} is ready after ${Date.now() - start}ms`);
        return;
      }
    } catch {
      // API call failed, agent not yet registered
    }
    await new Promise((r) => setTimeout(r, interval));
  }
  throw new Error(`Agent ${agentId} not ready after ${maxWait}ms`);
}

/**
 * Options for spawning an agent
 */
export interface SpawnAgentOptions {
  agentId: string;
  agentName: string;
  serverEndpoint?: string;
  searchRoots: string | string[];
  listenPort: number;
  logDir: string;
  logRetention?: number;
  noHeartbeat?: boolean;
  rustLog?: string;
}

/**
 * Spawn an opsbox-agent process
 *
 * @param repoRoot - Root directory of the repository
 * @param options - Agent spawn options
 * @returns The spawned process and its configuration
 */
export function spawnAgent(repoRoot: string, options: SpawnAgentOptions): ChildProcessWithoutNullStreams {
  const { command, argsPrefix, cwd } = findAgentCommand(repoRoot);

  const searchRoots = Array.isArray(options.searchRoots) ? options.searchRoots.join(',') : options.searchRoots;

  const args = [
    ...argsPrefix,
    '--agent-id',
    options.agentId,
    '--agent-name',
    options.agentName,
    '--server-endpoint',
    options.serverEndpoint ?? 'http://127.0.0.1:4001',
    '--search-roots',
    searchRoots,
    '--listen-port',
    String(options.listenPort),
    '--log-dir',
    options.logDir,
    '--log-retention',
    String(options.logRetention ?? 1)
  ];

  if (options.noHeartbeat !== false) {
    args.push('--no-heartbeat');
  }

  const proc = spawn(command, args, {
    cwd,
    env: { ...process.env, RUST_LOG: options.rustLog ?? 'info' },
    stdio: 'pipe'
  });

  // Forward stdout/stderr for debugging
  proc.stdout.on('data', (d) => process.stdout.write(d));
  proc.stderr.on('data', (d) => process.stderr.write(d));

  return proc;
}

/**
 * Generate a unique test ID based on timestamp and random suffix
 */
export function generateTestId(prefix: string): string {
  const timestamp = Date.now();
  const random = Math.random().toString(36).substring(2, 8);
  return `${prefix}_${timestamp}_${random}`;
}

/**
 * Create a temporary directory for tests
 */
export function createTempDir(baseDir: string, prefix: string): string {
  const dir = path.join(baseDir, `${prefix}_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`);
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

/**
 * Write a tar file with given entries (pure JS implementation)
 */
export function writeTarFile(outFile: string, entries: Array<{ name: string; content: string }>): void {
  const blocks: Buffer[] = [];

  function writeHeader(name: string, size: number): Buffer {
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

/**
 * Write a tar.gz file with given entries
 */
export function writeTarGzFile(outFile: string, entries: Array<{ name: string; content: string }>): void {
  const blocks: Buffer[] = [];

  function writeHeader(name: string, size: number): Buffer {
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
  const tarData = Buffer.concat(blocks);
  const gzipped = zlib.gzipSync(tarData);
  fs.writeFileSync(outFile, gzipped);
}
