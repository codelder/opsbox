/**
 * Settings Page E2E Tests
 *
 * Real integration tests for settings page (/settings):
 * - Planner script management (real API)
 * - LLM Backend CRUD (mock)
 * - S3 Profile CRUD (mock)
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
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
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
      await page.getByRole('tab', { name: '规划脚本' }).click();
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
      await expect(page.getByText('规划脚本').first()).toBeVisible();
    });

    test('should load existing planner scripts', async ({ page }) => {
      await page.reload();
      await page.getByRole('tab', { name: '规划脚本' }).click();
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
      await expect(page.getByText('规划脚本').first()).toBeVisible();
    });
  });

  test.describe('LLM Backend CRUD', () => {
    let mockBackends: Array<{
      name: string;
      provider: 'ollama' | 'openai';
      base_url: string;
      model: string;
      timeout_secs: number;
      has_api_key: boolean;
    }> = [];
    let mockLlmDefault: string | null = null;

    test.beforeEach(async ({ page }) => {
      mockBackends = [];
      mockLlmDefault = null;

      // LLM backends API - stateful mock
      await page.route('**/api/v1/logseek/settings/llm/backends**', async (route) => {
        const req = route.request();
        if (req.method() === 'GET') {
          await route.fulfill({
            status: 200,
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ backends: mockBackends, default: mockLlmDefault })
          });
        } else if (req.method() === 'POST') {
          const body = JSON.parse(req.postData() || '{}');
          const idx = mockBackends.findIndex((b) => b.name === body.name);
          if (idx >= 0) {
            mockBackends[idx] = { ...mockBackends[idx], ...body };
          } else {
            mockBackends.push({ ...body, has_api_key: !!body.api_key });
          }
          await route.fulfill({ status: 204 });
        } else {
          await route.continue();
        }
      });

      // Individual LLM backend operations (DELETE)
      await page.route('**/api/v1/logseek/settings/llm/backends/*', async (route) => {
        const req = route.request();
        if (req.method() === 'DELETE') {
          const url = req.url();
          const name = decodeURIComponent(url.split('/backends/')[1].split(/[?#]/)[0]);
          mockBackends = mockBackends.filter((b) => b.name !== name);
          if (mockLlmDefault === name) {
            mockLlmDefault = null;
          }
          await route.fulfill({ status: 204 });
        } else {
          await route.continue();
        }
      });

      // LLM default API
      await page.route('**/api/v1/logseek/settings/llm/default', async (route) => {
        const req = route.request();
        if (req.method() === 'POST') {
          const body = JSON.parse(req.postData() || '{}');
          mockLlmDefault = body.name;
          await route.fulfill({ status: 204 });
        } else {
          await route.fulfill({
            status: 200,
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(mockLlmDefault)
          });
        }
      });

      // Mock models endpoint
      await page.route('**/api/v1/logseek/settings/llm/models**', async (route) => {
        await route.fulfill({
          status: 200,
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ models: [] })
        });
      });

      await page.reload();
    });

    test('SETTINGS-04: Create LLM Backend', async ({ page }) => {
      await page.getByRole('tab', { name: '大模型' }).click();
      await expect(page.getByRole('button', { name: '新建后端' })).toBeVisible();

      await page.getByRole('button', { name: '新建后端' }).click();

      await page.locator('#llm-name').fill('ollama-local');
      await page.locator('#llm-provider').selectOption('ollama');
      await page.locator('#llm-base-url').fill('http://127.0.0.1:11434');
      await page.locator('#llm-model').fill('qwen3:8b');

      const saveBtn = page.getByRole('button', { name: /^保存$/ });
      await expect(saveBtn).toBeEnabled();
      await saveBtn.click();

      // Verify the form closes (isEditing becomes false) and the new backend appears in list
      await expect(page.getByRole('button', { name: '新建后端' })).toBeVisible({ timeout: 10000 });
      await expect(page.getByText('ollama-local')).toBeVisible();
      await expect(page.getByText('qwen3:8b')).toBeVisible();
    });

    test('SETTINGS-04 variant: Save disabled with empty required fields', async ({ page }) => {
      await page.getByRole('tab', { name: '大模型' }).click();
      await page.getByRole('button', { name: '新建后端' }).click();

      const saveBtn = page.getByRole('button', { name: /^保存$/ });
      await expect(saveBtn).toBeDisabled();

      await page.locator('#llm-name').fill('test');
      await expect(saveBtn).toBeDisabled();
    });

    test('SETTINGS-05: Set LLM Backend as Default', async ({ page }) => {
      mockBackends = [
        {
          name: 'ollama-local',
          provider: 'ollama',
          base_url: 'http://127.0.0.1:11434',
          model: 'qwen3:8b',
          timeout_secs: 60,
          has_api_key: false
        },
        {
          name: 'openai-prod',
          provider: 'openai',
          base_url: 'https://api.openai.com',
          model: 'gpt-4',
          timeout_secs: 120,
          has_api_key: true
        }
      ];

      await page.getByRole('tab', { name: '大模型' }).click();
      await expect(page.getByText('ollama-local')).toBeVisible();
      await expect(page.getByText('已默认')).toHaveCount(0);

      await page.getByRole('button', { name: '设为默认' }).first().click();

      await expect(page.getByText('已默认')).toBeVisible({ timeout: 10000 });
      const setDefaultButtons = page.getByRole('button', { name: '设为默认' });
      await expect(setDefaultButtons).toHaveCount(1);
    });

    test('SETTINGS-05 variant: Already default button disabled', async ({ page }) => {
      mockBackends = [
        {
          name: 'ollama-local',
          provider: 'ollama',
          base_url: 'http://127.0.0.1:11434',
          model: 'qwen3:8b',
          timeout_secs: 60,
          has_api_key: false
        }
      ];
      mockLlmDefault = 'ollama-local';

      await page.getByRole('tab', { name: '大模型' }).click();
      await expect(page.getByText('ollama-local')).toBeVisible();
      await expect(page.getByText('已默认')).toBeVisible();
      await expect(page.getByRole('button', { name: '设为默认' })).toHaveCount(0);
    });

    test('SETTINGS-06: Delete LLM Backend', async ({ page }) => {
      mockBackends = [
        {
          name: 'ollama-local',
          provider: 'ollama',
          base_url: 'http://127.0.0.1:11434',
          model: 'qwen3:8b',
          timeout_secs: 60,
          has_api_key: false
        }
      ];

      await page.getByRole('tab', { name: '大模型' }).click();
      await expect(page.getByText('ollama-local')).toBeVisible();

      page.on('dialog', async (dialog) => {
        expect(dialog.type()).toBe('confirm');
        expect(dialog.message()).toContain('ollama-local');
        await dialog.accept();
      });

      const deleteBtn = page
        .locator('div.rounded-lg.border.p-4')
        .filter({ has: page.locator('span.font-semibold', { hasText: 'ollama-local' }) })
        .getByRole('button', { name: '删除' });
      await deleteBtn.click();

      await expect(page.getByText('ollama-local')).not.toBeVisible({ timeout: 10000 });
    });

    test('SETTINGS-06 variant: Delete default backend clears default', async ({ page }) => {
      mockBackends = [
        {
          name: 'ollama-local',
          provider: 'ollama',
          base_url: 'http://127.0.0.1:11434',
          model: 'qwen3:8b',
          timeout_secs: 60,
          has_api_key: false
        },
        {
          name: 'openai-prod',
          provider: 'openai',
          base_url: 'https://api.openai.com',
          model: 'gpt-4',
          timeout_secs: 120,
          has_api_key: true
        }
      ];
      mockLlmDefault = 'ollama-local';

      await page.getByRole('tab', { name: '大模型' }).click();
      await expect(page.getByText('ollama-local')).toBeVisible();
      await expect(page.getByText('已默认')).toBeVisible();

      page.on('dialog', async (dialog) => {
        await dialog.accept();
      });

      const deleteBtn = page
        .locator('div.rounded-lg.border.p-4')
        .filter({ has: page.locator('span.font-semibold', { hasText: 'ollama-local' }) })
        .getByRole('button', { name: '删除' });
      await deleteBtn.click();

      await expect(page.getByText('ollama-local')).not.toBeVisible({ timeout: 10000 });
      await expect(page.getByText('已默认')).toHaveCount(0);
    });
  });

  test.describe('S3 Profile CRUD', () => {
    let mockProfiles: Array<{
      profile_name: string;
      endpoint: string;
      access_key: string;
      secret_key: string;
    }> = [];

    test.beforeEach(async ({ page }) => {
      mockProfiles = [];

      // S3 Profiles API - stateful mock
      await page.route('**/api/v1/logseek/profiles', async (route) => {
        const req = route.request();
        if (req.method() === 'GET') {
          await route.fulfill({
            status: 200,
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ profiles: mockProfiles })
          });
        } else if (req.method() === 'POST') {
          const body = JSON.parse(req.postData() || '{}');
          const idx = mockProfiles.findIndex((p) => p.profile_name === body.profile_name);
          if (idx >= 0) {
            mockProfiles[idx] = body;
          } else {
            mockProfiles.push(body);
          }
          await route.fulfill({ status: 204 });
        } else {
          await route.continue();
        }
      });

      // Individual profile operations (DELETE)
      await page.route('**/api/v1/logseek/profiles/*', async (route) => {
        const req = route.request();
        if (req.method() === 'DELETE') {
          const url = req.url();
          const name = decodeURIComponent(url.split('/profiles/')[1].split(/[?#]/)[0]);
          mockProfiles = mockProfiles.filter((p) => p.profile_name !== name);
          await route.fulfill({ status: 204 });
        } else {
          await route.continue();
        }
      });

      await page.reload();
    });

    test('SETTINGS-01: Create S3 Profile', async ({ page }) => {
      await expect(page.getByRole('button', { name: '新建 Profile' })).toBeVisible();
      await page.getByRole('button', { name: '新建 Profile' }).click();

      await page.locator('#profile-name').fill('test-minio');
      await page.locator('#profile-endpoint').fill('http://127.0.0.1:9000');
      await page.locator('#profile-access-key').fill('minioadmin');
      await page.locator('#profile-secret-key').fill('miniosecret');

      const saveBtn = page.getByRole('button', { name: '保存 Profile' });
      await expect(saveBtn).toBeEnabled();
      await saveBtn.click();

      // Verify the form closes and the new profile appears in list
      await expect(page.getByRole('button', { name: '新建 Profile' })).toBeVisible({ timeout: 10000 });
      await expect(page.getByText('test-minio')).toBeVisible();
      await expect(page.getByText('http://127.0.0.1:9000')).toBeVisible();
    });

    test('SETTINGS-01 variant: Save disabled with empty fields', async ({ page }) => {
      await page.getByRole('button', { name: '新建 Profile' }).click();
      const saveBtn = page.getByRole('button', { name: '保存 Profile' });
      await expect(saveBtn).toBeDisabled();
    });

    test('SETTINGS-02: Edit S3 Profile', async ({ page }) => {
      mockProfiles = [
        {
          profile_name: 'test-minio',
          endpoint: 'http://127.0.0.1:9000',
          access_key: 'minioadmin',
          secret_key: ''
        }
      ];

      await expect(page.getByText('test-minio')).toBeVisible();

      const editBtn = page
        .locator('div.rounded-lg.border.p-4')
        .filter({ has: page.locator('span.font-semibold', { hasText: 'test-minio' }) })
        .getByRole('button', { name: '编辑' });
      await editBtn.click();

      await expect(page.getByText('编辑 Profile: test-minio')).toBeVisible();
      await expect(page.locator('#profile-name')).toBeDisabled();
      await expect(page.locator('#profile-name')).toHaveValue('test-minio');

      await page.locator('#profile-endpoint').clear();
      await page.locator('#profile-endpoint').fill('http://192.168.1.100:9000');
      await page.locator('#profile-access-key').fill('newkey');
      await page.locator('#profile-secret-key').fill('newsecret');

      await page.getByRole('button', { name: '保存 Profile' }).click();

      // Verify the form closes and the updated endpoint appears in list
      await expect(page.getByRole('button', { name: '新建 Profile' })).toBeVisible({ timeout: 10000 });
      await expect(page.getByText('http://192.168.1.100:9000')).toBeVisible();
    });

    test('SETTINGS-03: Delete S3 Profile', async ({ page }) => {
      mockProfiles = [
        {
          profile_name: 'test-minio',
          endpoint: 'http://127.0.0.1:9000',
          access_key: 'minioadmin',
          secret_key: ''
        }
      ];

      await expect(page.getByText('test-minio')).toBeVisible();

      page.on('dialog', async (dialog) => {
        expect(dialog.type()).toBe('confirm');
        expect(dialog.message()).toContain('test-minio');
        await dialog.accept();
      });

      const deleteBtn = page
        .locator('div.rounded-lg.border.p-4')
        .filter({ has: page.locator('span.font-semibold', { hasText: 'test-minio' }) })
        .getByRole('button', { name: '删除' });
      await deleteBtn.click();

      await expect(page.getByText('test-minio')).not.toBeVisible({ timeout: 10000 });
    });

    test('SETTINGS-03 variant: Multiple profiles selective deletion', async ({ page }) => {
      mockProfiles = [
        {
          profile_name: 'profile-a',
          endpoint: 'http://127.0.0.1:9000',
          access_key: 'key-a',
          secret_key: ''
        },
        {
          profile_name: 'profile-b',
          endpoint: 'http://127.0.0.1:9001',
          access_key: 'key-b',
          secret_key: ''
        }
      ];

      await expect(page.getByText('profile-a')).toBeVisible();
      await expect(page.getByText('profile-b')).toBeVisible();

      page.on('dialog', async (dialog) => {
        await dialog.accept();
      });

      const deleteBtn = page
        .locator('div.rounded-lg.border.p-4')
        .filter({ has: page.locator('span.font-semibold', { hasText: 'profile-a' }) })
        .getByRole('button', { name: '删除' });
      await deleteBtn.click();

      await expect(page.getByText('profile-a')).not.toBeVisible({ timeout: 10000 });
      await expect(page.getByText('profile-b')).toBeVisible();
    });
  });

  test.describe('Agent Management (Real API)', () => {
    test('should display agent list from real API', async ({ page }) => {
      await page.getByRole('tab', { name: 'Agent' }).click();
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
      await expect(page.getByText('已注册 Agent')).toBeVisible();
    });

    test('should handle empty agent list gracefully', async ({ page }) => {
      await page.reload();
      await page.getByRole('tab', { name: 'Agent' }).click();
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
      await expect(page.getByText('已注册 Agent')).toBeVisible();
    });
  });

  test.describe('Server Log Settings (Real API)', () => {
    test('should display log settings section', async ({ page }) => {
      await page.getByRole('tab', { name: 'Server 日志' }).click();
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
      await expect(page.getByText('Server 日志设置')).toBeVisible();
    });

    test('should load current log configuration', async ({ page }) => {
      await page.reload();
      await page.getByRole('tab', { name: 'Server 日志' }).click();
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
      await expect(page.getByText('Server 日志设置')).toBeVisible();
    });
  });

  test.describe('Theme Toggle', () => {
    test('should toggle between light and dark theme', async ({ page }) => {
      const themeButton = page.getByRole('button', { name: /toggle/i });
      await themeButton.waitFor({ state: 'visible', timeout: 5000 });

      const initialClass = (await page.locator('html').getAttribute('class')) || '';
      const initialBg = await page.locator('html').evaluate(() =>
        getComputedStyle(document.documentElement).getPropertyValue('--background').trim()
      );

      await themeButton.click();
      await page.waitForTimeout(300);
      const darkClass = (await page.locator('html').getAttribute('class')) || '';
      const darkBg = await page.locator('html').evaluate(() =>
        getComputedStyle(document.documentElement).getPropertyValue('--background').trim()
      );
      expect(darkClass).toContain('dark');
      expect(darkBg).not.toBe(initialBg);

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
      await page.route('**/log/config', (route) =>
        route.fulfill({
          status: 500,
          body: JSON.stringify({ error: 'Internal error' })
        })
      );

      await page.reload();

      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
      await expect(page.getByRole('tab').first()).toBeVisible();
    });

    test('should show loading state', async ({ page }) => {
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
    });
  });

  test.describe('Settings Navigation', () => {
    test('should have settings button in header', async ({ page }) => {
      await page.goto('/');
      await page.waitForLoadState('networkidle');

      const settingsButton = page.getByRole('button', { name: /打开设置|settings/i });
      await settingsButton.waitFor({ state: 'visible', timeout: 5000 });
      await expect(settingsButton).toBeVisible();
    });

    test('should navigate to settings page', async ({ page }) => {
      await page.goto('/');
      await page.waitForLoadState('networkidle');

      const settingsButton = page.getByRole('button', { name: /打开设置|settings/i });
      await settingsButton.waitFor({ state: 'visible', timeout: 5000 });
      await settingsButton.click();
      await page.waitForURL(/\/settings(?:\?|$)/);

      expect(page.url()).toContain('/settings');
      await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
    });
  });
});
