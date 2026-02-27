/**
 * useLlmBackends Composable 测试
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useLlmBackends } from './useLlmBackends.svelte';
import * as api from '../api';

vi.mock('../api', () => ({
  listLlmBackends: vi.fn(),
  upsertLlmBackend: vi.fn(),
  deleteLlmBackend: vi.fn(),
  setDefaultLlm: vi.fn()
}));

describe('useLlmBackends', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('load 应该加载后端列表', async () => {
    const mockData = { backends: [{ name: 'b1' }], defaultName: 'b1' };
    vi.mocked(api.listLlmBackends).mockResolvedValueOnce(mockData as any);

    const state = useLlmBackends();
    await state.load();

    expect(state.backends).toEqual(mockData.backends);
    expect(state.defaultName).toBe('b1');
  });

  it('save 应该成功并重新加载', async () => {
    vi.mocked(api.upsertLlmBackend).mockResolvedValueOnce(undefined);
    vi.mocked(api.listLlmBackends).mockResolvedValueOnce({ backends: [], defaultName: null });

    const state = useLlmBackends();
    const payload = { name: 'n', provider: 'ollama' } as any;
    const result = await state.save(payload);

    expect(result).toBe(true);
    expect(state.saveSuccess).toBe(true);
    expect(api.upsertLlmBackend).toHaveBeenCalledWith(payload);
  });

  it('remove 应该成功', async () => {
    vi.mocked(api.deleteLlmBackend).mockResolvedValueOnce(undefined);
    vi.mocked(api.listLlmBackends).mockResolvedValueOnce({ backends: [], defaultName: null });

    const state = useLlmBackends();
    const result = await state.remove('test');

    expect(result).toBe(true);
    expect(api.deleteLlmBackend).toHaveBeenCalledWith('test');
  });

  it('makeDefault 应该成功', async () => {
    vi.mocked(api.setDefaultLlm).mockResolvedValueOnce(undefined);

    const state = useLlmBackends();
    const result = await state.makeDefault('new-def');

    expect(result).toBe(true);
    expect(state.defaultName).toBe('new-def');
  });
});
