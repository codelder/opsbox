/**
 * useSearch Composable 测试
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useSearch } from './useSearch.svelte';
import * as api from '../api';
import * as streamReaderModule from './useStreamReader.svelte';

vi.mock('../api', () => ({
  startUnifiedSearch: vi.fn(),
  extractSessionId: vi.fn(() => 'test-sid'),
  deleteSearchSession: vi.fn()
}));

const mockStreamReader = {
  initReader: vi.fn(),
  readBatch: vi.fn().mockResolvedValue({ hasMore: false }),
  cleanup: vi.fn()
};

vi.mock('./useStreamReader.svelte', () => ({
  useStreamReader: vi.fn(() => mockStreamReader)
}));

describe('useSearch', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('search 应该初始化并读取第一批数据', async () => {
    const mockResponse = { ok: true };
    vi.mocked(api.startUnifiedSearch).mockResolvedValueOnce(mockResponse as any);

    const state = useSearch();
    await state.search('test query');

    expect(state.query).toBe('test query');
    expect(api.startUnifiedSearch).toHaveBeenCalled();
    expect(mockStreamReader.initReader).toHaveBeenCalledWith(mockResponse);
    expect(mockStreamReader.readBatch).toHaveBeenCalled();
  });

  it('search 失败时应设置错误信息', async () => {
    vi.mocked(api.startUnifiedSearch).mockRejectedValueOnce(new Error('Search failed'));

    const state = useSearch();
    await state.search('test');

    expect(state.error).toBe('Search failed');
    expect(state.loading).toBe(false);
    expect(state.hasMore).toBe(false);
  });

  it('cancel 应该中止控制器并清理读取器', () => {
    const state = useSearch();

    state.cancel();

    expect(mockStreamReader.cleanup).toHaveBeenCalled();
    expect(state.loading).toBe(false);
    expect(state.hasMore).toBe(false);
  });
});
