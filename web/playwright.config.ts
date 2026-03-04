/// <reference types="node" />
import { defineConfig, devices } from '@playwright/test';
import { fileURLToPath } from 'url';

// Resolve global setup/teardown paths for ESM
const globalSetupPath = fileURLToPath(new URL('./tests/e2e/global-setup.ts', import.meta.url));
const globalTeardownPath = fileURLToPath(new URL('./tests/e2e/global-teardown.ts', import.meta.url));

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'line',
  // Global setup: cleanup leftover resources from previous runs
  globalSetup: globalSetupPath,
  // Global teardown: cleanup resources from this run
  globalTeardown: globalTeardownPath,
  // Global timeout for entire test run
  globalTimeout: process.env.CI ? 600000 : 300000,
  use: {
    baseURL: 'http://localhost:5173',
    trace: 'on-first-retry'
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] }
    }
  ],
  webServer: [
    {
      command: 'pnpm run dev',
      url: 'http://127.0.0.1:5173',
      // 默认不复用：避免本地已有 dev server 使用了不同的 BACKEND_PORT/代理配置，导致 e2e 偶发 HTTP 500。
      // 如需本地复用（加速迭代）可显式设置 PW_REUSE_SERVER=1。
      reuseExistingServer: !process.env.CI && process.env.PW_REUSE_SERVER === '1',
      stdout: 'pipe',
      stderr: 'pipe',
      env: {
        BACKEND_PORT: '4001',
        VITE_HOST: '127.0.0.1'
      }
    },
    {
      command: 'sh -c "cd ../backend && cargo run --release -p opsbox-server -- --port 4001"',
      url: 'http://127.0.0.1:4001/healthy',
      reuseExistingServer: !process.env.CI && process.env.PW_REUSE_SERVER === '1',
      stdout: 'pipe',
      stderr: 'pipe',
      env: {
        // 让 e2e 时能看到 500 的根因（尤其是 panic/backtrace）
        RUST_LOG: process.env.RUST_LOG ?? 'info',
        RUST_BACKTRACE: process.env.RUST_BACKTRACE ?? '1'
      },
      timeout: process.env.CI ? 300000 : 120000 // CI 环境需要更长时间编译
    }
  ]
});
