/**
 * Logs API 测试
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  fetchServerLogConfig,
  updateServerLogLevel,
  updateServerLogRetention,
  fetchAgentLogConfig,
  updateAgentLogLevel,
  updateAgentLogRetention
} from './logs';

describe('Logs API', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Server Log Management', () => {
    it('fetchServerLogConfig 应该获取配置', async () => {
      const mockResult = { level: 'info', retention_count: 7, log_dir: '/tmp' };
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => mockResult
      } as Response);

      const res = await fetchServerLogConfig();
      expect(res).toEqual(mockResult);
      expect(globalThis.fetch).toHaveBeenCalledWith('/api/v1/log/config', expect.any(Object));
    });

    it('updateServerLogLevel 应该发送 PUT', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        json: async () => ({ message: 'ok' })
      } as Response);

      await updateServerLogLevel('debug');
      expect(globalThis.fetch).toHaveBeenCalledWith(
        '/api/v1/log/level',
        expect.objectContaining({ method: 'PUT', body: JSON.stringify({ level: 'debug' }) })
      );
    });

    it('updateServerLogRetention 应该发送 PUT', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        json: async () => ({ message: 'ok' })
      } as Response);

      await updateServerLogRetention(14);
      expect(globalThis.fetch).toHaveBeenCalledWith(
        '/api/v1/log/retention',
        expect.objectContaining({ method: 'PUT', body: JSON.stringify({ retention_count: 14 }) })
      );
    });
  });

  describe('Agent Log Management (via Proxy)', () => {
    it('fetchAgentLogConfig 应该获取配置', async () => {
      const mockResult = { level: 'error', retention_count: 3, log_dir: '/var/log' };
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => mockResult
      } as Response);

      const res = await fetchAgentLogConfig('agent-1');
      expect(res).toEqual(mockResult);
      expect(globalThis.fetch).toHaveBeenCalledWith(expect.stringContaining('/agent-1/log/config'), expect.any(Object));
    });

    it('处理 502 错误为 "Agent 离线"', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({ ok: false, status: 502 } as Response);
      await expect(fetchAgentLogConfig('agent-1')).rejects.toThrow('Agent 离线或无法连接');
    });

    it('updateAgentLogLevel 应该发送 PUT', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        json: async () => ({ message: 'ok' })
      } as Response);

      await updateAgentLogLevel('agent-1', 'trace');
      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/agent-1/log/level'),
        expect.objectContaining({ method: 'PUT', body: JSON.stringify({ level: 'trace' }) })
      );
    });

    it('updateAgentLogRetention 应该发送 PUT', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        json: async () => ({ message: 'ok' })
      } as Response);

      await updateAgentLogRetention('agent-1', 30);
      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/agent-1/log/retention'),
        expect.objectContaining({ method: 'PUT', body: JSON.stringify({ retention_count: 30 }) })
      );
    });
  });
});
