/**
 * NL2Q API 测试
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { convertNaturalLanguage } from './nl2q';

describe('NL2Q API', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('应该正确转换自然语言为查询字符串', async () => {
    globalThis.fetch = vi.fn().mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ q: 'error AND level:error' })
    } as unknown as Response);

    const result = await convertNaturalLanguage('查找错误日志');

    expect(result).toBe('error AND level:error');
    expect(globalThis.fetch).toHaveBeenCalledWith(
      expect.stringContaining('/nl2q'),
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ nl: '查找错误日志' })
      })
    );
  });

  it('当响应体为空白时应抛出错误', async () => {
    globalThis.fetch = vi.fn().mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ q: ' ' })
    } as unknown as Response);

    await expect(convertNaturalLanguage('test')).rejects.toThrow('AI 返回空结果');
  });

  it('当 HTTP 响应失败时应抛出错误', async () => {
    globalThis.fetch = vi.fn().mockResolvedValueOnce({
      ok: false,
      status: 500
    } as unknown as Response);

    await expect(convertNaturalLanguage('test')).rejects.toThrow(/HTTP 500/);
  });
});
