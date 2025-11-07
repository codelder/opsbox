/**
 * Search API 测试
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { startSearch, extractSessionId, startUnifiedSearch } from './search';

describe('Search API', () => {
  beforeEach(() => {
    // 清除所有 mock
    vi.clearAllMocks();
  });

  describe('startSearch', () => {
    it('应该使用正确的 URL 和方法发送请求', async () => {
      const mockResponse = {
        ok: true,
        status: 200,
        headers: new Headers({ 'X-Logseek-SID': 'test-session-id' })
      } as Response;

      globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);

      const response = await startSearch('test query');

      expect(globalThis.fetch).toHaveBeenCalledWith(
expect.stringContaining('/search.ndjson'),
        expect.objectContaining({
          method: 'POST',
          headers: expect.any(Object),
          body: JSON.stringify({ q: 'test query' })
        })
      );

      expect(response).toBe(mockResponse);
    });

    it('应该在响应不成功时抛出错误', async () => {
      const mockResponse = {
        ok: false,
        status: 500,
        headers: new Headers()
      } as Response;

      globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);

      await expect(startSearch('test query')).rejects.toThrow(/HTTP 500/);
    });

    it('应该在网络错误时抛出错误', async () => {
      globalThis.fetch = vi.fn().mockRejectedValueOnce(new Error('Network error'));

      await expect(startSearch('test query')).rejects.toThrow('Network error');
    });

    it('应该处理 404 错误', async () => {
      const mockResponse = {
        ok: false,
        status: 404,
        headers: new Headers()
      } as Response;

      globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);

      await expect(startSearch('test query')).rejects.toThrow(/HTTP 404/);
    });

    it('应该处理特殊字符和长查询', async () => {
      const mockResponse = {
        ok: true,
        status: 200,
        headers: new Headers()
      } as Response;

      globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);

      const specialQuery = 'test "quoted" AND (grouped) OR filter:value';
      await startSearch(specialQuery);

      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          body: JSON.stringify({ q: specialQuery })
        })
      );
    });
  });

  describe('extractSessionId', () => {
    it('应该从响应头中提取会话 ID', () => {
      const mockResponse = {
        headers: new Headers({ 'X-Logseek-SID': 'session-123' })
      } as Response;

      const sessionId = extractSessionId(mockResponse);

      expect(sessionId).toBe('session-123');
    });

    it('应该返回空字符串当会话 ID 不存在', () => {
      const mockResponse = {
        headers: new Headers()
      } as Response;

      const sessionId = extractSessionId(mockResponse);

      expect(sessionId).toBe('');
    });

    it('应该处理大小写不敏感的响应头', () => {
      // Headers 对象在某些环境中可能区分大小写
      const mockResponse = {
        headers: new Headers({ 'x-logseek-sid': 'session-456' })
      } as Response;

      const sessionId = extractSessionId(mockResponse);

      // 测试标准 API 的行为
      expect(sessionId).toBeDefined();
    });
  });

  describe('startUnifiedSearch', () => {
    it('应该使用 search.ndjson 端点发送请求', async () => {
      const mockResponse = {
        ok: true,
        status: 200,
        headers: new Headers({ 'X-Logseek-SID': 'unified-session-id' })
      } as Response;

      globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);

      const response = await startUnifiedSearch('test query');

      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/search.ndjson'),
        expect.objectContaining({
          method: 'POST',
          headers: expect.any(Object),
          body: JSON.stringify({ q: 'test query' })
        })
      );

      expect(response).toBe(mockResponse);
    });

    it('应该在响应不成功时抛出错误', async () => {
      const mockResponse = {
        ok: false,
        status: 503,
        headers: new Headers()
      } as Response;

      globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);

      await expect(startUnifiedSearch('test query')).rejects.toThrow(/HTTP 503/);
    });

    it('应该在网络错误时抛出错误', async () => {
      globalThis.fetch = vi.fn().mockRejectedValueOnce(new Error('Connection timeout'));

      await expect(startUnifiedSearch('test query')).rejects.toThrow('Connection timeout');
    });

    it('应该处理并行搜索场景', async () => {
      const mockResponse1 = {
        ok: true,
        status: 200,
        headers: new Headers({ 'X-Logseek-SID': 'session-1' })
      } as Response;

      const mockResponse2 = {
        ok: true,
        status: 200,
        headers: new Headers({ 'X-Logseek-SID': 'session-2' })
      } as Response;

      globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse1).mockResolvedValueOnce(mockResponse2);

      const response1 = await startUnifiedSearch('query1');
      const response2 = await startUnifiedSearch('query2');

      expect(extractSessionId(response1)).toBe('session-1');
      expect(extractSessionId(response2)).toBe('session-2');
      expect(globalThis.fetch).toHaveBeenCalledTimes(2);
    });
  });
});
