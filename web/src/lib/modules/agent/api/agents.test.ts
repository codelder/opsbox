/**
 * Agents API 测试
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { fetchAgents, fetchAgentTags, setAgentTags, addAgentTag, removeAgentTag, clearAgentTags } from './agents';

describe('Agents API', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('fetchAgents', () => {
    it('应该正确获取 Agent 列表', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({ agents: [{ id: '1', name: 'a' }] })
      } as Response);

      const res = await fetchAgents();
      expect(res.agents.length).toBe(1);
    });

    it('应该正确处理查询参数', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({})
      } as Response);

      await fetchAgents({ tags: 'tag1', onlineOnly: true });

      const url = (globalThis.fetch as any).mock.calls[0][0];
      const searchParams = new URL(url, 'http://localhost').searchParams;
      expect(searchParams.get('tags')).toBe('tag1');
      expect(searchParams.get('online_only')).toBe('true');
    });
  });

  describe('Tag Management', () => {
    it('fetchAgentTags 应该获取标签', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => [{ key: 'k', value: 'v' }]
      } as Response);

      const res = await fetchAgentTags('a1');
      expect(res).toEqual([{ key: 'k', value: 'v' }]);
      expect(globalThis.fetch).toHaveBeenCalledWith(expect.stringContaining('/a1/tags'), expect.any(Object));
    });

    it('setAgentTags 应该发送 POST', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({ ok: true });
      const tags = [{ key: 'k', value: 'v' }];
      await setAgentTags('a1', tags);
      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/a1/tags'),
        expect.objectContaining({ method: 'POST', body: JSON.stringify({ tags }) })
      );
    });

    it('addAgentTag 应该发送 POST to /add', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({ ok: true });
      const tag = { key: 'k', value: 'v' };
      await addAgentTag('a1', tag);
      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/a1/tags/add'),
        expect.objectContaining({ method: 'POST', body: JSON.stringify(tag) })
      );
    });

    it('removeAgentTag 应该发送 DELETE', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({ ok: true });
      const tag = { key: 'k', value: 'v' };
      await removeAgentTag('a1', tag);
      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/a1/tags/remove'),
        expect.objectContaining({ method: 'DELETE', body: JSON.stringify(tag) })
      );
    });

    it('clearAgentTags 应该发送 DELETE to /clear', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({ ok: true });
      await clearAgentTags('a1');
      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/a1/tags/clear'),
        expect.objectContaining({ method: 'DELETE' })
      );
    });
  });
});
