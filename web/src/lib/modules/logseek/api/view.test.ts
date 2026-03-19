/**
 * View API 测试
 */

import { describe, it, expect, beforeEach, vi, type Mock } from 'vitest';
import { fetchViewCache, fetchViewDownload } from './view';

describe('View API', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('fetchViewCache', () => {
    it('应该使用正确的查询参数发送 GET 请求', async () => {
      const mockResult = {
        sid: 'sid1',
        file: 'orl://local/test.log',
        start: 1,
        end: 10,
        lines: ['line1', 'line2'],
        total_lines: 2
      };

      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => mockResult
      } as unknown as Response);

      const result = await fetchViewCache('sid1', 'orl://local/test.log', 1, 10);

      expect(result).toEqual(mockResult);
      expect(globalThis.fetch).toHaveBeenCalledWith(expect.stringContaining('/view.cache.json?'), expect.any(Object));

      const url = (globalThis.fetch as Mock).mock.calls[0][0] as string;
      const searchParams = new URL(url, 'http://localhost').searchParams;
      expect(searchParams.get('sid')).toBe('sid1');
      expect(searchParams.get('file')).toBe('orl://local/test.log');
      expect(searchParams.get('start')).toBe('1');
      expect(searchParams.get('end')).toBe('10');
    });

    it('失败时应尝试解析错误 JSON 并抛出消息', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: false,
        status: 404,
        text: async () => JSON.stringify({ message: '文件未找到' })
      } as unknown as Response);

      await expect(fetchViewCache('s', 'f', 1, 2)).rejects.toThrow('文件未找到');
    });

    it('JSON 解析失败时应回退到默认 HTTP 错误消息', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: false,
        status: 500,
        text: async () => 'Internal Server Error'
      } as unknown as Response);

      await expect(fetchViewCache('s', 'f', 1, 2)).rejects.toThrow(/HTTP 500/);
    });
  });

  describe('fetchViewDownload', () => {
    it('应该构建正确的下载 URL', async () => {
      const mockResponse = { ok: true } as unknown as Response;
      globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);

      const result = await fetchViewDownload('sid1', 'orl://local/test.log');

      expect(result).toBe(mockResponse);
      const url = (globalThis.fetch as Mock).mock.calls[0][0] as string;
      const searchParams = new URL(url, 'http://localhost').searchParams;
      expect(searchParams.get('sid')).toBe('sid1');
      expect(searchParams.get('file')).toBe('orl://local/test.log');
    });

    it('失败时应抛出错误', async () => {
      globalThis.fetch = vi.fn().mockResolvedValueOnce({
        ok: false,
        status: 403,
        text: async () => 'Forbidden'
      } as unknown as Response);

      await expect(fetchViewDownload('s', 'f')).rejects.toThrow(/HTTP 403/);
    });
  });
});
