import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  server: {
    // 本地开发时允许外部访问；E2E/CI 下建议只绑定回环地址避免端口权限/冲突问题
    host: process.env.VITE_HOST || '0.0.0.0',
    port: Number(process.env.VITE_PORT) || 5173, // 支持通过环境变量覆盖端口
    proxy: {
      '/api': {
        target: process.env.BACKEND_PORT ? `http://127.0.0.1:${process.env.BACKEND_PORT}` : 'http://127.0.0.1:4000',
        changeOrigin: true
      }
    }
  },
  test: {
    expect: { requireAssertions: true },
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: ['**/node_modules/**', '**/dist/**', '**/tests/**', '**/*.d.ts'],
      thresholds: {
        lines: 70, // 业务逻辑行覆盖率
        functions: 70,
        branches: 60,
        statements: 70
      }
    },
    projects: [
      {
        extends: './vite.config.ts',
        test: {
          name: 'client',
          environment: 'browser',
          browser: {
            enabled: true,
            provider: 'playwright',
            instances: [{ browser: 'chromium' }]
          },
          include: ['src/**/*.svelte.{test,spec}.{js,ts}'],
          exclude: ['src/lib/server/**'],
          setupFiles: ['./vitest-setup-client.ts']
        }
      },
      {
        extends: './vite.config.ts',
        test: {
          name: 'server',
          environment: 'node',
          include: ['src/**/*.{test,spec}.{js,ts}'],
          exclude: ['src/**/*.svelte.{test,spec}.{js,ts}']
        }
      }
    ]
  }
});
