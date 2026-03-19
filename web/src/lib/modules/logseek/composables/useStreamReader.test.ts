/**
 * useStreamReader Composable 测试
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useStreamReader } from './useStreamReader.svelte';
import type { SearchJsonResult } from '../types';
import type { SearchEvent } from './useStreamReader.svelte';

describe('useStreamReader', () => {
  let streamReader: ReturnType<typeof useStreamReader>;

  beforeEach(() => {
    streamReader = useStreamReader();
  });

  it('initReader 应该初始化 reader 和 decoder', () => {
    const mockReader = { read: vi.fn() };
    const mockResponse = { body: { getReader: () => mockReader } } as unknown as Response;

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
    const mockResponse = { body: { getReader: () => mockReader } } as unknown as Response;

    streamReader.initReader(mockResponse);

    const results: SearchJsonResult[] = [];
    const { hasMore, produced } = await streamReader.readBatch(
      10,
      (result) => results.push(result),
      (error) => console.error(error)
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
    const mockResponse = { body: { getReader: () => mockReader } } as unknown as Response;

    streamReader.initReader(mockResponse);

    let eventCaptured: SearchEvent | undefined;
    await streamReader.readBatch(
      10,
      () => {},
      () => {},
      (event) => (eventCaptured = event)
    );

    expect(eventCaptured).toEqual({ type: 'error', source: 's3', message: 'err' });
  });

  it('遇到 AbortError 应该直接。返回且不视作错误', async () => {
    const mockReader = {
      read: vi.fn().mockRejectedValue({ name: 'AbortError' })
    };
    const mockResponse = { body: { getReader: () => mockReader } } as unknown as Response;

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
