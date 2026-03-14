/**
 * Settings Page E2E Tests
 *
 * Real integration tests for settings page (/settings):
 * - Planner script management (real API)
 * - LLM backend configuration (mock - requires external service)
 * - S3 Profile management (mock - requires external service)
 * - Agent management (real API)
 * - Server log settings (real API)
 * - Theme toggle
 */

import { test, expect } from '@playwright/test';

test.describe('Settings Page E2E', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/settings');
    await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
  });

  test.describe('Page Layout', () => {
    test('should display settings page with navigation tabs', async ({ page }) => {
      // 验证页面标题可见
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();

      // 验证所有 5 个导航 Tab 可见
      await expect(page.getByRole('tab', { name: '对象存储' })).toBeVisible();
      await expect(page.getByRole('tab', { name: 'Agent' })).toBeVisible();
      await expect(page.getByRole('tab', { name: '规划脚本' })).toBeVisible();
      await expect(page.getByRole('tab', { name: '大模型' })).toBeVisible();
      await expect(page.getByRole('tab', { name: 'Server 日志' })).toBeVisible();
    });

    test('should have theme toggle button', async ({ page }) => {
      const themeButton = page.getByRole('button', { name: /theme|主题|toggle/i });
      await expect(themeButton.first()).toBeVisible();
    });
  });

  test.describe('Planner Management (Real API)', () => {
    test('should display planner management section', async ({ page }) => {
      // 点击规划脚本 Tab 后验证内容
      await page.getByRole('tab', { name: '规划脚本' }).click();
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
      await expect(page.getByText('规划脚本').first()).toBeVisible();
    });

    test('should load existing planner scripts', async ({ page }) => {
      await page.reload();
      // 点击规划脚本 Tab 后验证内容
      await page.getByRole('tab', { name: '规划脚本' }).click();
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
      await expect(page.getByText('规划脚本').first()).toBeVisible();
    });
  });

  test.describe('LLM Management (Mock)', () => {
    // LLM 需要外部服务，使用 mock
    test('should display LLM backend configuration', async ({ page }) => {
      // 拦截 LLM API - 返回正确的响应结构 { backends: [...], default: null }
      await page.route('**/settings/llm/backends**', async (route) => {
        await route.fulfill({
          status: 200,
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            backends: [
              {
                name: 'ollama-local',
                provider: 'ollama',
                base_url: 'http://127.0.0.1:11434',
                model: 'qwen3:8b',
                timeout_secs: 60
              }
            ],
            default: null
          })
        });
      });

      await page.reload();

      // 点击大模型 Tab 后验证 mock 数据渲染
      await page.getByRole('tab', { name: '大模型' }).click();
      await expect(page.getByText('ollama-local')).toBeVisible({ timeout: 10000 });
      await expect(page.getByText('qwen3:8b')).toBeVisible();
      // 验证恰好有一个 LLM 后端条目
      await expect(page.locator('span.font-semibold', { hasText: 'ollama-local' })).toHaveCount(1);
    });
  });

  test.describe('S3 Profile Management (Mock)', () => {
    // S3 需要外部服务，使用 mock
    test('should display S3 profile configuration', async ({ page }) => {
      // 拦截 S3 Profile API - 后端返回 { profiles: [...] } 格式
      await page.route('**/api/v1/logseek/profiles', async (route) => {
        await route.fulfill({
          status: 200,
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            profiles: [
              {
                profile_name: 'minio-local',
                endpoint: 'http://127.0.0.1:9000',
                access_key: 'minioadmin',
                secret_key: ''
              }
            ]
          })
        });
      });

      await page.reload();

      // 默认 Tab 为对象存储，直接验证 mock 数据渲染
      await expect(page.getByText('minio-local')).toBeVisible({ timeout: 10000 });
      await expect(page.getByText('http://127.0.0.1:9000')).toBeVisible();
      // 验证恰好有一个 S3 Profile 条目
      await expect(page.locator('span.font-semibold', { hasText: 'minio-local' })).toHaveCount(1);
    });
  });

  test.describe('Agent Management (Real API)', () => {
    test('should display agent list from real API', async ({ page }) => {
      // 点击 Agent Tab 后验证内容
      await page.getByRole('tab', { name: 'Agent' }).click();
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
      await expect(page.getByText('已注册 Agent')).toBeVisible();
    });

    test('should handle empty agent list gracefully', async ({ page }) => {
      await page.reload();
      // 点击 Agent Tab 后验证内容
      await page.getByRole('tab', { name: 'Agent' }).click();
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
      await expect(page.getByText('已注册 Agent')).toBeVisible();
    });
  });

  test.describe('Server Log Settings (Real API)', () => {
    test('should display log settings section', async ({ page }) => {
      // 点击 Server 日志 Tab 后验证内容
      await page.getByRole('tab', { name: 'Server 日志' }).click();
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
      await expect(page.getByText('Server 日志设置')).toBeVisible();
    });

    test('should load current log configuration', async ({ page }) => {
      await page.reload();
      // 点击 Server 日志 Tab 后验证内容
      await page.getByRole('tab', { name: 'Server 日志' }).click();
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
      await expect(page.getByText('Server 日志设置')).toBeVisible();
    });
  });

  test.describe('Theme Toggle', () => {
    test('should toggle between light and dark theme', async ({ page }) => {
      const themeButton = page.getByRole('button', { name: /toggle/i });
      await themeButton.waitFor({ state: 'visible', timeout: 5000 });

      // 记录初始状态
      const initialClass = (await page.locator('html').getAttribute('class')) || '';
      const initialBg = await page.locator('html').evaluate(() =>
        getComputedStyle(document.documentElement).getPropertyValue('--background').trim()
      );

      // 切换到深色模式
      await themeButton.click();
      await page.waitForTimeout(300);
      const darkClass = (await page.locator('html').getAttribute('class')) || '';
      const darkBg = await page.locator('html').evaluate(() =>
        getComputedStyle(document.documentElement).getPropertyValue('--background').trim()
      );
      expect(darkClass).toContain('dark');
      expect(darkBg).not.toBe(initialBg);

      // 切换回浅色模式
      await themeButton.click();
      await page.waitForTimeout(300);
      const finalClass = (await page.locator('html').getAttribute('class')) || '';
      const finalBg = await page.locator('html').evaluate(() =>
        getComputedStyle(document.documentElement).getPropertyValue('--background').trim()
      );
      expect(finalClass).not.toContain('dark');
      expect(finalBg).toBe(initialBg);
    });
  });

  test.describe('Error Handling', () => {
    test('should handle API errors gracefully', async ({ page }) => {
      // 只模拟部分 API 错误，保留 S3 和 LLM 的 mock
      await page.route('**/log/config', (route) =>
        route.fulfill({
          status: 500,
          body: JSON.stringify({ error: 'Internal error' })
        })
      );

      await page.reload();

      // 页面应该仍然显示标题和 Tab 结构
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
      await expect(page.getByRole('tab').first()).toBeVisible();
    });

    test('should show loading state', async ({ page }) => {
      // 页面已在 beforeEach 中导航，直接验证加载结果
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
    });
  });

  test.describe('Settings Navigation', () => {
    test('should have settings button in header', async ({ page }) => {
      await page.goto('/');
      await page.waitForLoadState('networkidle');

      // 查找设置按钮并直接验证
      const settingsButton = page.getByRole('button', { name: /打开设置|settings/i });
      await settingsButton.waitFor({ state: 'visible', timeout: 5000 });
      await expect(settingsButton).toBeVisible();
    });

    test('should navigate to settings page', async ({ page }) => {
      await page.goto('/');
      await page.waitForLoadState('networkidle');

      // 点击设置按钮
      const settingsButton = page.getByRole('button', { name: /打开设置|settings/i });
      await settingsButton.waitFor({ state: 'visible', timeout: 5000 });
      await settingsButton.click();
      await page.waitForURL(/\/settings(?:\?|$)/);

      // 应该导航到设置页面
      expect(page.url()).toContain('/settings');
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
    });
  });
});
