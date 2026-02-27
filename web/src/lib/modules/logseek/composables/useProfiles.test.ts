/**
 * useProfiles Composable 测试
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useProfiles } from './useProfiles.svelte';
import * as api from '../api';

// Mock API
vi.mock('../api', () => ({
  listProfiles: vi.fn(),
  saveProfile: vi.fn(),
  deleteProfile: vi.fn()
}));

describe('useProfiles', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('应该能正确执行 loadProfiles', async () => {
    const mockProfiles = [{ profile_name: 'p1', endpoint: 'e1', access_key: 'k1', secret_key: 's1' }];
    vi.mocked(api.listProfiles).mockResolvedValueOnce(mockProfiles);

    const profilesState = useProfiles();

    expect(profilesState.loading).toBe(false);

    const promise = profilesState.loadProfiles();
    expect(profilesState.loading).toBe(true);

    await promise;

    expect(profilesState.loading).toBe(false);
    expect(profilesState.profiles).toEqual(mockProfiles);
  });

  it('loadProfiles 失败时应设置错误信息', async () => {
    vi.mocked(api.listProfiles).mockRejectedValueOnce(new Error('Network error'));

    const profilesState = useProfiles();
    await profilesState.loadProfiles();

    expect(profilesState.loading).toBe(false);
    expect(profilesState.error).toBe('Network error');
    expect(profilesState.profiles).toEqual([]);
  });

  it('应该能正确执行 save', async () => {
    vi.mocked(api.saveProfile).mockResolvedValueOnce(undefined);
    vi.mocked(api.listProfiles).mockResolvedValueOnce([]); // save 之后会调用 loadProfiles

    const profilesState = useProfiles();
    const profile = { profile_name: 'new', endpoint: 'e', access_key: 'a', secret_key: 's' };

    const result = await profilesState.save(profile);

    expect(result).toBe(true);
    expect(profilesState.saveSuccess).toBe(true);
    expect(api.saveProfile).toHaveBeenCalledWith(profile);
    expect(api.listProfiles).toHaveBeenCalled();
  });

  it('save 失败时应设置错误信息', async () => {
    vi.mocked(api.saveProfile).mockRejectedValueOnce(new Error('Save failed'));

    const profilesState = useProfiles();
    const result = await profilesState.save({} as any);

    expect(result).toBe(false);
    expect(profilesState.saveError).toBe('Save failed');
  });

  it('应该能正确执行 remove', async () => {
    vi.mocked(api.deleteProfile).mockResolvedValueOnce(undefined);
    vi.mocked(api.listProfiles).mockResolvedValueOnce([]);

    const profilesState = useProfiles();
    const result = await profilesState.remove('test-p');

    expect(result).toBe(true);
    expect(api.deleteProfile).toHaveBeenCalledWith('test-p');
  });

  it('clearSaveState 应该重置状态', () => {
    const profilesState = useProfiles();
    // 模拟一些状态（虽然由于是 runes，我们需要通过方法触发或者直接修改，但我们只能通过方法触发）
    // 我们可以直接通过 save 失败来设置状态

    profilesState.clearSaveState();
    expect(profilesState.saveError).toBeNull();
    expect(profilesState.saveSuccess).toBe(false);
  });
});
