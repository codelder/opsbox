/**
 * Settings API 测试
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { fetchS3Settings, saveS3Settings } from './settings';

describe('Settings API', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('fetchS3Settings', () => {
    it('应该正确获取 S3 设置', async () => {
      const mockSettings = { endpoint: 'e', access_key: 'a', secret_key: 's', configured: true };
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => mockSettings
      } as unknown as Response);

      const result = await fetchS3Settings();

      expect(result).toEqual(mockSettings);
      expect(globalThis.fetch).toHaveBeenCalledWith(expect.stringContaining('/settings/s3'), expect.any(Object));
    });

    it('带 verify 参数时应构建正确 URL', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({})
      } as unknown as Response);

      await fetchS3Settings(true);

      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/settings/s3?verify=true'),
        expect.any(Object)
      );
    });
  });

  describe('saveS3Settings', () => {
    it('应该发送 POST 请求保存设置', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({ ok: true, status: 204 } as unknown as Response);

      const payload = { endpoint: 'e', access_key: 'a', secret_key: 's' };
      await saveS3Settings(payload);

      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/settings/s3'),
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify(payload)
        })
      );
    });
  });
});
