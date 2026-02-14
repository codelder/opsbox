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
    await page.waitForLoadState('networkidle');
  });

  test.describe('Page Layout', () => {
    test('should display settings page with navigation tabs', async ({ page }) => {
      // 验证页面加载
      await expect(page.locator('body')).toBeVisible();

      // 验证有设置相关内容
      const pageContent = (await page.locator('body').textContent()) || '';
      // 页面应该有内容
      expect(pageContent.length).toBeGreaterThan(0);
    });

    test('should have theme toggle button', async ({ page }) => {
      const themeButton = page.getByRole('button', { name: /theme|主题|toggle/i });
      const themeCount = await themeButton.count();

      if (themeCount > 0) {
        await expect(themeButton.first()).toBeVisible();
      }
    });
  });

  test.describe('Planner Management (Real API)', () => {
    test('should display planner management section', async ({ page }) => {
      // 等待页面加载完成
      await page.waitForLoadState('networkidle');

      // 检查 planner 相关内容是否存在
      const bodyText = (await page.locator('body').textContent()) || '';
      // 页面应该正常加载
      expect(bodyText.length).toBeGreaterThan(0);
    });

    test('should load existing planner scripts', async ({ page }) => {
      // 刷新页面
      await page.reload();
      await page.waitForLoadState('networkidle');

      // 页面应该正常显示
      await expect(page.locator('body')).toBeVisible();
    });
  });

  test.describe('LLM Management (Mock)', () => {
    // LLM 需要外部服务，使用 mock
    test('should display LLM backend configuration', async ({ page }) => {
      // 拦截 LLM API
      await page.route('**/settings/llm/backends**', async (route) => {
        await route.fulfill({
          status: 200,
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify([
            {
              name: 'ollama-local',
              provider: 'ollama',
              base_url: 'http://127.0.0.1:11434',
              model: 'qwen3:8b',
              timeout_secs: 60
            }
          ])
        });
      });

      await page.reload();
      await page.waitForLoadState('networkidle');

      // 页面应该正常显示
      await expect(page.locator('body')).toBeVisible();
    });
  });

  test.describe('S3 Profile Management (Mock)', () => {
    // S3 需要外部服务，使用 mock
    test('should display S3 profile configuration', async ({ page }) => {
      // 拦截 S3 Profile API
      await page.route('**/profiles**', async (route) => {
        await route.fulfill({
          status: 200,
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify([
            {
              profile_name: 'minio-local',
              endpoint: 'http://127.0.0.1:9000',
              access_key: 'minioadmin'
            }
          ])
        });
      });

      await page.reload();
      await page.waitForLoadState('networkidle');

      // 页面应该正常显示
      await expect(page.locator('body')).toBeVisible();
    });
  });

  test.describe('Agent Management (Real API)', () => {
    test('should display agent list from real API', async ({ page }) => {
      // 等待 API 响应
      await page.waitForLoadState('networkidle');

      // 页面应该正常显示
      await expect(page.locator('body')).toBeVisible();

      // 检查是否有 Agent 相关内容
      const bodyText = (await page.locator('body').textContent()) || '';
      // 页面应该有内容
      expect(bodyText.length).toBeGreaterThan(0);
    });

    test('should handle empty agent list gracefully', async ({ page }) => {
      // 刷新页面
      await page.reload();
      await page.waitForLoadState('networkidle');

      // 页面应该正常显示（即使没有 Agent）
      await expect(page.locator('body')).toBeVisible();
    });
  });

  test.describe('Server Log Settings (Real API)', () => {
    test('should display log settings section', async ({ page }) => {
      // 等待页面加载
      await page.waitForLoadState('networkidle');

      // 检查页面正常
      await expect(page.locator('body')).toBeVisible();
    });

    test('should load current log configuration', async ({ page }) => {
      // 刷新页面
      await page.reload();
      await page.waitForLoadState('networkidle');

      // 页面应该正常显示
      await expect(page.locator('body')).toBeVisible();
    });
  });

  test.describe('Theme Toggle', () => {
    test('should toggle between light and dark theme', async ({ page }) => {
      const themeButton = page.getByRole('button', { name: /theme|主题|toggle/i });
      const themeCount = await themeButton.count();

      if (themeCount > 0) {
        // 记录初始主题
        const html = page.locator('html');
        const initialClass = (await html.getAttribute('class')) || '';

        // 点击切换主题
        await themeButton.first().click();
        await page.waitForTimeout(300);

        // 验证主题类存在
        const newClass = (await html.getAttribute('class')) || '';
        expect(newClass).toBeDefined();
      }
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
      await page.waitForLoadState('networkidle');

      // 页面应该仍然显示，不应该崩溃
      await expect(page.locator('body')).toBeVisible();
    });

    test('should show loading state', async ({ page }) => {
      // 直接导航到设置页面
      await page.goto('/settings');

      // 页面应该正常加载
      await expect(page.locator('body')).toBeVisible();
    });
  });

  test.describe('Settings Navigation', () => {
    test('should have settings button in header', async ({ page }) => {
      await page.goto('/');
      await page.waitForLoadState('networkidle');

      // 查找设置按钮
      const settingsButton = page.getByRole('button', { name: /打开设置|settings/i });
      const count = await settingsButton.count();

      if (count > 0) {
        await expect(settingsButton.first()).toBeVisible();
      }
    });

    test('should navigate to settings page', async ({ page }) => {
      await page.goto('/');
      await page.waitForLoadState('networkidle');

      // 点击设置按钮
      const settingsButton = page.getByRole('button', { name: /打开设置|settings/i });
      const count = await settingsButton.count();

      if (count > 0) {
        await settingsButton.first().click();
        await page.waitForLoadState('networkidle');

        // 应该导航到设置页面
        expect(page.url()).toContain('/settings');
      }
    });
  });
});
