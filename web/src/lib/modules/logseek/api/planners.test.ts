/**
 * Planners API 测试
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  listPlanners,
  getPlanner,
  savePlanner,
  deletePlanner,
  testPlanner,
  getDefaultPlanner,
  setDefaultPlanner
} from './planners';

describe('Planners API', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('listPlanners', () => {
    it('应该正确获取列表', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({ items: [{ app: 'a', updated_at: 0 }], default: 'a' })
      } as unknown as Response);

      const res = await listPlanners();
      expect(res.items.length).toBe(1);
      expect(res.default).toBe('a');
    });
  });

  describe('getDefaultPlanner / setDefaultPlanner', () => {
    it('should get default planner', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => 'app-default'
      } as unknown as Response);

      const res = await getDefaultPlanner();
      expect(res).toBe('app-default');
    });

    it('should set default planner', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({ ok: true, status: 204 } as unknown as Response);

      await setDefaultPlanner('new-app');

      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/settings/planners/default'),
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ app: 'new-app' })
        })
      );
    });
  });

  describe('getPlanner', () => {
    it('应该根据 app 名称获取脚本', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({ app: 'test', script: 'print("hi")', updated_at: 100 })
      } as unknown as Response);

      const res = await getPlanner('test');
      expect(res.app).toBe('test');
      expect(res.script).toBe('print("hi")');
    });
  });

  describe('savePlanner', () => {
    it('应该发送 POST 请求保存脚本', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({ ok: true, status: 200 } as unknown as Response);

      const payload = { app: 'test', script: 'print("a")' };
      await savePlanner(payload);

      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/settings/planners/scripts'),
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify(payload)
        })
      );
    });
  });

  describe('deletePlanner', () => {
    it('应该发送 DELETE 请求', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({ ok: true, status: 204 } as unknown as Response);

      await deletePlanner('test-app');

      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/settings/planners/scripts/test-app'),
        expect.objectContaining({ method: 'DELETE' })
      );
    });
  });

  describe('testPlanner', () => {
    it('应该发送 POST 请求进行测试并返回结果', async () => {
      const mockResult = {
        cleaned_query: 'err',
        sources: [],
        debug_logs: ['log 1']
      };
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => mockResult
      } as unknown as Response);

      const payload = { app: 'test', q: 'err' };
      const res = await testPlanner(payload);

      expect(res).toEqual(mockResult);
      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/settings/planners/test'),
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify(payload)
        })
      );
    });
  });
});
