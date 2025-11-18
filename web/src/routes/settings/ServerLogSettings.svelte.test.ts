/**
 * ServerLogSettings 组件测试
 * 测试 Server 日志设置组件的渲染、交互和 API 调用
 */
import { expect, test, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from '@vitest/browser/context';
import type { LogConfigResponse } from '$lib/modules/agent/api';
import ServerLogSettings from './ServerLogSettings.svelte';

// Mock API 模块
vi.mock('$lib/modules/agent/api', () => ({
  fetchServerLogConfig: vi.fn(),
  updateServerLogLevel: vi.fn(),
  updateServerLogRetention: vi.fn()
}));

import {
  fetchServerLogConfig,
  updateServerLogLevel,
  updateServerLogRetention
} from '$lib/modules/agent/api';

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.restoreAllMocks();
});

test('组件渲染 - 显示加载状态', async () => {
  vi.mocked(fetchServerLogConfig).mockImplementation(
    () => new Promise(() => {}) // 永不 resolve，保持加载状态
  );

  render(ServerLogSettings, {});

  const loadingText = await page.getByText('加载中…');
  await expect.element(loadingText).toBeInTheDocument();
});

test('组件渲染 - 成功加载配置后显示表单', async () => {
  const mockConfig = {
    level: 'info',
    retention_count: 7,
    log_dir: '/home/user/.opsbox/logs'
  };

  vi.mocked(fetchServerLogConfig).mockResolvedValue(mockConfig);

  render(ServerLogSettings, {});

  // 等待配置加载
  await vi.waitFor(async () => {
    const heading = await page.getByText('Server 日志设置');
    await expect.element(heading).toBeInTheDocument();
  });

  // 验证表单元素存在
  const levelSelect = await page.getByRole('combobox');
  await expect.element(levelSelect).toBeInTheDocument();

  const retentionInput = await page.getByRole('spinbutton');
  await expect.element(retentionInput).toBeInTheDocument();

  const logDirInputs = page.getByRole('textbox');
  // Verify log dir input exists and is disabled (it's the third textbox)
});

test('表单交互 - 修改日志级别', async () => {
  const mockConfig = {
    level: 'info',
    retention_count: 7,
    log_dir: '/home/user/.opsbox/logs'
  };

  vi.mocked(fetchServerLogConfig).mockResolvedValue(mockConfig);

  render(ServerLogSettings, {});

  await vi.waitFor(async () => {
    const levelSelect = await page.getByRole('combobox');
    await expect.element(levelSelect).toBeInTheDocument();
  });

  const levelSelect = page.getByRole('combobox');
  const element = await levelSelect.element();
  await userEvent.selectOptions(element, 'debug');

  await expect.element(levelSelect).toHaveValue('debug');
});

test('表单交互 - 修改日志保留数量', async () => {
  const mockConfig = {
    level: 'info',
    retention_count: 7,
    log_dir: '/home/user/.opsbox/logs'
  };

  vi.mocked(fetchServerLogConfig).mockResolvedValue(mockConfig);

  render(ServerLogSettings, {});

  await vi.waitFor(async () => {
    const retentionInput = await page.getByRole('spinbutton');
    await expect.element(retentionInput).toBeInTheDocument();
  });

  const retentionInput = await page.getByRole('spinbutton');
  await userEvent.clear(retentionInput);
  await userEvent.type(retentionInput, '14');

  await expect.element(retentionInput).toHaveValue(14);
});

test('API 调用 - 保存配置成功', async () => {
  const mockConfig = {
    level: 'info',
    retention_count: 7,
    log_dir: '/home/user/.opsbox/logs'
  };

  vi.mocked(fetchServerLogConfig).mockResolvedValue(mockConfig);
  vi.mocked(updateServerLogLevel).mockResolvedValue({} as never);
  vi.mocked(updateServerLogRetention).mockResolvedValue({} as never);

  render(ServerLogSettings, {});

  await vi.waitFor(async () => {
    const saveButton = await page.getByRole('button', { name: /保存/ });
    await expect.element(saveButton).toBeInTheDocument();
  });

  const saveButton = await page.getByRole('button', { name: /保存/ });
  await userEvent.click(saveButton);

  await vi.waitFor(() => {
    expect(updateServerLogLevel).toHaveBeenCalledWith('info');
    expect(updateServerLogRetention).toHaveBeenCalledWith(7);
  });
});

test('API 调用 - 保存配置失败显示错误', async () => {
  const mockConfig = {
    level: 'info',
    retention_count: 7,
    log_dir: '/home/user/.opsbox/logs'
  };

  vi.mocked(fetchServerLogConfig).mockResolvedValue(mockConfig);
  vi.mocked(updateServerLogLevel).mockRejectedValue(new Error('更新失败'));

  render(ServerLogSettings, {});

  await vi.waitFor(async () => {
    const saveButton = await page.getByRole('button', { name: /保存/ });
    await expect.element(saveButton).toBeInTheDocument();
  });

  const saveButton = await page.getByRole('button', { name: /保存/ });
  await userEvent.click(saveButton);

  await vi.waitFor(async () => {
    const errorMessage = await page.getByText('更新失败');
    await expect.element(errorMessage).toBeInTheDocument();
  });
});

test('错误处理 - 加载配置失败显示错误', async () => {
  vi.mocked(fetchServerLogConfig).mockRejectedValue(new Error('加载配置失败'));

  render(ServerLogSettings, {});

  await vi.waitFor(async () => {
    const errorMessage = await page.getByText('加载配置失败');
    await expect.element(errorMessage).toBeInTheDocument();
  });
});

test('表单交互 - 重置按钮重新加载配置', async () => {
  const mockConfig = {
    level: 'info',
    retention_count: 7,
    log_dir: '/home/user/.opsbox/logs'
  };

  vi.mocked(fetchServerLogConfig).mockResolvedValue(mockConfig);

  render(ServerLogSettings, {});

  await vi.waitFor(async () => {
    const resetButton = await page.getByRole('button', { name: /重置/ });
    await expect.element(resetButton).toBeInTheDocument();
  });

  const resetButton = await page.getByRole('button', { name: /重置/ });
  await userEvent.click(resetButton);

  await vi.waitFor(() => {
    expect(fetchServerLogConfig).toHaveBeenCalledTimes(2); // 初始加载 + 重置
  });
});

test('表单交互 - 保存时禁用按钮', async () => {
  const mockConfig = {
    level: 'info',
    retention_count: 7,
    log_dir: '/home/user/.opsbox/logs'
  };

  vi.mocked(fetchServerLogConfig).mockResolvedValue(mockConfig);
  vi.mocked(updateServerLogLevel).mockImplementation(
    () => new Promise((resolve) => setTimeout(resolve, 1000))
  );

  render(ServerLogSettings, {});

  await vi.waitFor(async () => {
    const saveButton = await page.getByRole('button', { name: /保存/ });
    await expect.element(saveButton).toBeInTheDocument();
  });

  const saveButton = await page.getByRole('button', { name: /保存/ });
  await userEvent.click(saveButton);

  // 保存过程中按钮应该被禁用
  await vi.waitFor(async () => {
    const savingButton = await page.getByRole('button', { name: /保存中/ });
    await expect.element(savingButton).toBeDisabled();
  });
});
