/**
 * E2E Test Fixtures and Utilities
 *
 * Provides resource tracking, dynamic port allocation, and cleanup utilities
 * to ensure tests are reliable in both local and CI environments.
 */

import { test as base, type APIRequestContext } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import * as net from 'net';
import { spawn, type ChildProcessWithoutNullStreams } from 'child_process';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

/**
 * Resource Tracker - tracks all resources created during tests
 * and ensures cleanup even if tests fail or timeout.
 */
export class ResourceTracker {
  private processes: ChildProcessWithoutNullStreams[] = [];
  private directories: string[] = [];
  private agentCleanup: Array<{ id: string; request: APIRequestContext; url: string }> = [];
  private plannerCleanup: Array<{ app: string; request: APIRequestContext; url: string }> = [];
  private profileCleanup: Array<{ name: string; request: APIRequestContext; url: string }> = [];

  /**
   * Track a spawned process for cleanup
   */
  trackProcess(proc: ChildProcessWithoutNullStreams): ChildProcessWithoutNullStreams {
    this.processes.push(proc);
    return proc;
  }

  /**
   * Track a directory for cleanup
   */
  trackDirectory(dir: string): string {
    this.directories.push(dir);
    return dir;
  }

  /**
   * Track an agent registration for cleanup
   */
  trackAgent(id: string, request: APIRequestContext, baseUrl: string) {
    this.agentCleanup.push({ id, request, url: `${baseUrl}/api/v1/agents/${id}` });
  }

  /**
   * Track a planner script for cleanup
   */
  trackPlanner(app: string, request: APIRequestContext, baseUrl: string) {
    this.plannerCleanup.push({ app, request, url: `${baseUrl}/api/v1/logseek/settings/planners/scripts/${app}` });
  }

  /**
   * Track an S3 profile for cleanup
   */
  trackProfile(name: string, request: APIRequestContext, baseUrl: string) {
    this.profileCleanup.push({ name, request, url: `${baseUrl}/api/v1/logseek/profiles/${name}` });
  }

  /**
   * Cleanup all tracked resources
   */
  async cleanupAll(): Promise<void> {
    console.log('[ResourceTracker] Starting cleanup...');

    // 1. Stop all processes (with timeout)
    for (const proc of this.processes) {
      try {
        await this.stopProcess(proc);
      } catch (e) {
        console.error('[ResourceTracker] Failed to stop process:', e);
      }
    }

    // 2. Cleanup API resources (agents, planners, profiles)
    await Promise.allSettled([
      ...this.agentCleanup.map(async ({ request, url, id }) => {
        try {
          const resp = await request.delete(url);
          console.log(`[ResourceTracker] Deleted agent ${id}: ${resp.status()}`);
        } catch (e) {
          console.error(`[ResourceTracker] Failed to delete agent ${id}:`, e);
        }
      }),
      ...this.plannerCleanup.map(async ({ request, url, app }) => {
        try {
          const resp = await request.delete(url);
          console.log(`[ResourceTracker] Deleted planner ${app}: ${resp.status()}`);
        } catch (e) {
          console.error(`[ResourceTracker] Failed to delete planner ${app}:`, e);
        }
      }),
      ...this.profileCleanup.map(async ({ request, url, name }) => {
        try {
          const resp = await request.delete(url);
          console.log(`[ResourceTracker] Deleted profile ${name}: ${resp.status()}`);
        } catch (e) {
          console.error(`[ResourceTracker] Failed to delete profile ${name}:`, e);
        }
      }),
    ]);

    // 3. Cleanup directories
    for (const dir of this.directories) {
      try {
        if (fs.existsSync(dir)) {
          fs.rmSync(dir, { recursive: true, force: true });
          console.log(`[ResourceTracker] Removed directory: ${dir}`);
        }
      } catch (e) {
        console.error(`[ResourceTracker] Failed to remove directory ${dir}:`, e);
      }
    }

    console.log('[ResourceTracker] Cleanup completed');
  }

  private async stopProcess(proc: ChildProcessWithoutNullStreams, timeout = 5000): Promise<void> {
    if (proc.exitCode !== null) return;

    return new Promise((resolve) => {
      const timeoutId = setTimeout(() => {
        console.log('[ResourceTracker] Process did not exit gracefully, sending SIGKILL');
        proc.kill('SIGKILL');
      }, timeout);

      proc.once('exit', (code) => {
        clearTimeout(timeoutId);
        console.log(`[ResourceTracker] Process exited with code ${code}`);
        resolve();
      });

      // Send SIGINT first for graceful shutdown
      proc.kill('SIGINT');
    });
  }
}

/**
 * Get an available port on the system
 */
export async function getAvailablePort(startPort = 50000, maxAttempts = 100): Promise<number> {
  return new Promise((resolve, reject) => {
    const tryPort = (port: number, attempts: number) => {
      if (attempts >= maxAttempts) {
        reject(new Error(`Could not find available port after ${maxAttempts} attempts`));
        return;
      }

      const server = net.createServer();
      server.once('error', (err: NodeJS.ErrnoException) => {
        if (err.code === 'EADDRINUSE') {
          tryPort(port + 1, attempts + 1);
        } else {
          reject(err);
        }
      });
      server.once('listening', () => {
        server.close();
        resolve(port);
      });
      server.listen(port, '127.0.0.1');
    };

    tryPort(startPort, 0);
  });
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
 * Create a temporary directory with tracking
 */
export function createTempDir(tracker: ResourceTracker, prefix: string): string {
  const dir = path.join(__dirname, `${prefix}_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`);
  fs.mkdirSync(dir, { recursive: true });
  tracker.trackDirectory(dir);
  return dir;
}

/**
 * Stop a process with timeout
 */
export async function stopProcess(proc: ChildProcessWithoutNullStreams, timeout = 5000): Promise<void> {
  if (proc.exitCode !== null) return;

  return new Promise((resolve) => {
    const timeoutId = setTimeout(() => {
      proc.kill('SIGKILL');
    }, timeout);

    proc.once('exit', () => {
      clearTimeout(timeoutId);
      resolve();
    });

    proc.kill('SIGINT');
  });
}

/**
 * Wait for a process to be ready by polling a health endpoint
 */
export async function waitForHealthy(
  url: string,
  timeout = 30000,
  interval = 500
): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeout) {
    try {
      const resp = await fetch(url);
      if (resp.ok) {
        return true;
      }
    } catch {
      // Ignore connection errors
    }
    await new Promise((r) => setTimeout(r, interval));
  }
  return false;
}

// Extend Playwright test with resource tracker
export const test = base.extend<{
  resources: ResourceTracker;
}>({
  resources: async ({}, use) => {
    const tracker = new ResourceTracker();
    await use(tracker);
    // Always cleanup, even if test fails
    await tracker.cleanupAll();
  },
});

export { expect } from '@playwright/test';
