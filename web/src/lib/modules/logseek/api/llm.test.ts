/**
 * LLM API 测试
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  listLlmBackends,
  upsertLlmBackend,
  deleteLlmBackend,
  getDefaultLlm,
  setDefaultLlm,
  listLlmModelsByParams,
  listLlmModelsByBackend
} from './llm';
import type { LlmBackendUpsertPayload } from '../types';

describe('LLM API', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('listLlmBackends', () => {
    it('应该正确获取后端列表和默认后端', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({
          backends: [
            {
              name: 'ollama-local',
              provider: 'ollama',
              base_url: '...',
              model: '...',
              timeout_secs: 60,
              has_api_key: false
            }
          ],
          default: 'ollama-local'
        })
      } as unknown as Response);

      const result = await listLlmBackends();

      expect(result.backends.length).toBe(1);
      expect(result.defaultName).toBe('ollama-local');
    });

    it('后端列表为空时应处理', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({ backends: [] })
      } as unknown as Response);

      const result = await listLlmBackends();
      expect(result.backends).toEqual([]);
      expect(result.defaultName).toBeNull();
    });
  });

  describe('upsertLlmBackend', () => {
    it('应该发送 POST 请求保存后端', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({ ok: true, status: 200 } as unknown as Response);

      const payload: LlmBackendUpsertPayload = {
        name: 'test',
        provider: 'ollama',
        base_url: 'http://localhost:11434',
        model: 'llama3',
        timeout_secs: 60,
        update_secret: true
      };

      await upsertLlmBackend(payload);

      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/settings/llm/backends'),
        expect.objectContaining({ method: 'POST', body: JSON.stringify(payload) })
      );
    });

    it('失败时应抛出错误消息', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: false,
        status: 400,
        json: async () => ({ detail: 'Invalid URL' })
      } as unknown as Response);

      await expect(upsertLlmBackend({} as any)).rejects.toThrow('Invalid URL');
    });
  });

  describe('deleteLlmBackend', () => {
    it('应该发送 DELETE 请求', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({ ok: true, status: 204 } as unknown as Response);

      await deleteLlmBackend('test-backend');

      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/settings/llm/backends/test-backend'),
        expect.objectContaining({ method: 'DELETE' })
      );
    });
  });

  describe('getDefaultLlm / setDefaultLlm', () => {
    it('should get default llm name', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => 'test-default'
      } as unknown as Response);

      const result = await getDefaultLlm();
      expect(result).toBe('test-default');
    });

    it('should set default llm name', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({ ok: true, status: 204 } as unknown as Response);

      await setDefaultLlm('new-default');

      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/settings/llm/default'),
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ name: 'new-default' })
        })
      );
    });
  });

  describe('listLlmModels', () => {
    it('should list models by params', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({ models: ['model1', 'model2'] })
      } as unknown as Response);

      const result = await listLlmModelsByParams({ provider: 'ollama', base_url: '...' });
      expect(result).toEqual(['model1', 'model2']);
    });

    it('should list models by backend name', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({ models: ['m1'] })
      } as unknown as Response);

      const result = await listLlmModelsByBackend('test-b');
      expect(result).toEqual(['m1']);
      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/backends/test-b/models'),
        expect.any(Object)
      );
    });
  });
});
