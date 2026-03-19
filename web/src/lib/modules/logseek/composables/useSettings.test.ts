/**
 * useSettings Composable 测试
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useSettings } from './useSettings.svelte';
import * as api from '../api';
import type { S3SettingsResponse } from '../types';

vi.mock('../api', () => ({
  fetchS3Settings: vi.fn(),
  saveS3Settings: vi.fn()
}));

describe('useSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('loadSettings 应该加载 S3 配置', async () => {
    const mockData: S3SettingsResponse = {
      endpoint: 'http://minio:9000',
      access_key: 'key',
      secret_key: 'secret',
      connection_error: null
    };
    vi.mocked(api.fetchS3Settings).mockResolvedValueOnce(mockData);

    const state = useSettings();
    await state.loadSettings();

    expect(state.endpoint).toBe('http://minio:9000');
    expect(state.accessKey).toBe('key');
    expect(state.secretKey).toBe('secret');
    expect(state.loadedOnce).toBe(true);
  });

  it('save 应该构建 payload 并调用 API', async () => {
    vi.mocked(api.saveS3Settings).mockResolvedValueOnce(undefined);
    vi.mocked(api.fetchS3Settings).mockResolvedValueOnce({
      endpoint: null as unknown as string,
      access_key: null as unknown as string,
      secret_key: null as unknown as string,
      connection_error: null
    } as unknown as S3SettingsResponse);

    const state = useSettings();
    state.endpoint = 'e';
    state.accessKey = 'a';
    state.secretKey = 's';

    await state.save();

    expect(api.saveS3Settings).toHaveBeenCalledWith({
      endpoint: 'e',
      access_key: 'a',
      secret_key: 's'
    });
    expect(state.saveSuccess).toBe(true);
  });

  it('loadSettings 失败时应保持 loadedOnce 为 false 并设置错误', async () => {
    vi.mocked(api.fetchS3Settings).mockRejectedValueOnce(new Error('Load error'));

    const state = useSettings();
    await state.loadSettings();

    expect(state.loadError).toBe('Load error');
    expect(state.loadedOnce).toBe(false);
  });
});
