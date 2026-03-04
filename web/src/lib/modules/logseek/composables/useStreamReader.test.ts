/**
 * useStreamReader Composable 测试
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useStreamReader } from './useStreamReader.svelte';

describe('useStreamReader', () => {
  let streamReader: any;

  beforeEach(() => {
    streamReader = useStreamReader();
  });

  it('initReader 应该初始化 reader 和 decoder', () => {
    const mockReader = { read: vi.fn() };
    const mockResponse = { body: { getReader: () => mockReader } } as any;

    streamReader.initReader(mockResponse);

    expect(streamReader.reader).toBe(mockReader);
  });

  it('readBatch 应该能解析结果并返回 produced 数量', async () => {
    const encoder = new TextEncoder();
    const data1 = JSON.stringify({ type: 'result', data: { path: 'file1', keywords: [], chunks: [] } }) + '\n';
    const data2 = JSON.stringify({ type: 'result', data: { path: 'file2', keywords: [], chunks: [] } }) + '\n';

    const mockReader = {
      read: vi
        .fn()
        .mockResolvedValueOnce({ done: false, value: encoder.encode(data1) })
        .mockResolvedValueOnce({ done: false, value: encoder.encode(data2) })
        .mockResolvedValueOnce({ done: true })
    };
    const mockResponse = { body: { getReader: () => mockReader } } as any;

    streamReader.initReader(mockResponse);

    const results: any[] = [];
    const { hasMore, produced } = await streamReader.readBatch(
      10,
      (r: any) => results.push(r),
      (e: any) => console.error(e)
    );

    expect(results.length).toBe(2);
    expect(produced).toBe(2);
    expect(hasMore).toBe(false);
    expect(results[0].path).toBe('file1');
    expect(results[1].path).toBe('file2');
  });

  it('应该能处理错误事件', async () => {
    const encoder = new TextEncoder();
    const errorEvent = JSON.stringify({ type: 'error', source: 's3', message: 'err' }) + '\n';

    const mockReader = {
      read: vi
        .fn()
        .mockResolvedValueOnce({ done: false, value: encoder.encode(errorEvent) })
        .mockResolvedValueOnce({ done: true })
    };
    const mockResponse = { body: { getReader: () => mockReader } } as any;

    streamReader.initReader(mockResponse);

    let eventCaptured: any;
    await streamReader.readBatch(
      10,
      () => {},
      () => {},
      (ev: any) => (eventCaptured = ev)
    );

    expect(eventCaptured).toEqual({ type: 'error', source: 's3', message: 'err' });
  });

  it('遇到 AbortError 应该直接。返回且不视作错误', async () => {
    const mockReader = {
      read: vi.fn().mockRejectedValue({ name: 'AbortError' })
    };
    const mockResponse = { body: { getReader: () => mockReader } } as any;

    streamReader.initReader(mockResponse);

    let errorOccurred = false;
    const { hasMore } = await streamReader.readBatch(
      10,
      () => {},
      () => {
        errorOccurred = true;
      }
    );

    expect(errorOccurred).toBe(false);
    expect(hasMore).toBe(false);
  });
});
