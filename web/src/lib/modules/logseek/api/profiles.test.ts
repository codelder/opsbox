/**
 * Profiles API 测试
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { listProfiles, saveProfile, deleteProfile } from './profiles';
import type { S3ProfilePayload } from '../types';

describe('Profiles API', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('listProfiles', () => {
    it('应该正确获取并返回 Profile 列表', async () => {
      const mockProfiles: S3ProfilePayload[] = [
        {
          profile_name: 'test-1',
          endpoint: 'http://s3.test1',
          access_key: 'key1',
          secret_key: 'secret1'
        },
        {
          profile_name: 'test-2',
          endpoint: 'http://s3.test2',
          access_key: 'key2',
          secret_key: 'secret2'
        }
      ];

      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({ profiles: mockProfiles })
      } as unknown as Response);

      const result = await listProfiles();

      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/profiles'),
        expect.objectContaining({ headers: expect.any(Object) })
      );
      expect(result).toEqual(mockProfiles);
    });

    it('当 profiles 字段缺失时应返回空数组', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({})
      } as unknown as Response);

      const result = await listProfiles();
      expect(result).toEqual([]);
    });

    it('当响应失败时应抛出错误', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: false,
        status: 403
      } as unknown as Response);

      await expect(listProfiles()).rejects.toThrow(/HTTP 403/);
    });
  });

  describe('saveProfile', () => {
    it('应该发送 POST 请求保存 Profile', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 204
      } as unknown as Response);

      const profile: S3ProfilePayload = {
        profile_name: 'new-profile',
        endpoint: 'http://minio:9000',
        access_key: 'minioadmin',
        secret_key: 'minioadmin'
      };

      await saveProfile(profile);

      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/profiles'),
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify(profile)
        })
      );
    });

    it('保存失败时应该解析并抛出 Problem Details 错误', async () => {
      const problem = { detail: '名称已被占用' };
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: false,
        status: 400,
        json: async () => problem
      } as unknown as Response);

      const profile: S3ProfilePayload = {
        profile_name: 'duplicate',
        endpoint: '',
        access_key: '',
        secret_key: ''
      };

      await expect(saveProfile(profile)).rejects.toThrow('名称已被占用');
    });

    it('当 JSON 解析失败时应抛出默认错误消息', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: false,
        status: 400,
        json: async () => {
          throw new Error('Invalid JSON');
        }
      } as unknown as Response);

      const profile: S3ProfilePayload = {
        profile_name: 'error',
        endpoint: '',
        access_key: '',
        secret_key: ''
      };

      await expect(saveProfile(profile)).rejects.toThrow(/HTTP 400/);
    });

    it('当权限不足(401)时应抛出错误', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: false,
        status: 401
      } as unknown as Response);

      const profile: S3ProfilePayload = {
        profile_name: 'no-auth',
        endpoint: '',
        access_key: '',
        secret_key: ''
      };

      await expect(saveProfile(profile)).rejects.toThrow(/HTTP 401/);
    });
  });

  describe('deleteProfile', () => {
    it('应该发送 DELETE 请求删除 Profile', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 204
      } as unknown as Response);

      await deleteProfile('test-profile');

      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/profiles/test-profile'),
        expect.objectContaining({
          method: 'DELETE'
        })
      );
    });

    it('应该对 Profile 名称进行 URL 编码', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 204
      } as unknown as Response);

      await deleteProfile('test profile@#');

      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/profiles/test%20profile%40%23'),
        expect.any(Object)
      );
    });

    it('删除失败时应抛出错误', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: false,
        status: 404,
        json: async () => ({ detail: '未找到配置' })
      } as unknown as Response);

      await expect(deleteProfile('none')).rejects.toThrow('未找到配置');
    });
  });
});
