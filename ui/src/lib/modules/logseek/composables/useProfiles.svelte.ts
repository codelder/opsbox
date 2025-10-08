/**
 * Profile 管理状态 Composable
 * 提供 S3 Profile 管理相关的状态和方法
 */

import { listProfiles, saveProfile, deleteProfile } from '../api';
import type { S3ProfilePayload } from '../types';

/**
 * Profile 管理状态和方法
 */
export function useProfiles() {
  let profiles = $state<S3ProfilePayload[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let saveSuccess = $state(false);
  let deleting = $state(false);
  let deleteError = $state<string | null>(null);

  /**
   * 加载所有 Profiles
   */
  async function loadProfiles(): Promise<void> {
    if (loading) return;

    loading = true;
    error = null;

    try {
      profiles = await listProfiles();
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message ?? '无法加载 Profile 列表';
      profiles = [];
    } finally {
      loading = false;
    }
  }

  /**
   * 保存 Profile
   */
  async function save(profile: S3ProfilePayload): Promise<boolean> {
    if (saving) return false;

    saving = true;
    saveError = null;
    saveSuccess = false;

    try {
      await saveProfile(profile);
      await loadProfiles(); // 重新加载列表
      saveSuccess = true;
      return true;
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      saveError = err.message ?? '保存 Profile 失败';
      return false;
    } finally {
      saving = false;
    }
  }

  /**
   * 删除 Profile
   */
  async function remove(profileName: string): Promise<boolean> {
    if (deleting) return false;

    deleting = true;
    deleteError = null;

    try {
      await deleteProfile(profileName);
      await loadProfiles(); // 重新加载列表
      return true;
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      deleteError = err.message ?? '删除 Profile 失败';
      return false;
    } finally {
      deleting = false;
    }
  }

  /**
   * 清除保存状态
   */
  function clearSaveState(): void {
    saveError = null;
    saveSuccess = false;
  }

  /**
   * 清除删除状态
   */
  function clearDeleteState(): void {
    deleteError = null;
  }

  return {
    // 状态
    get profiles() {
      return profiles;
    },
    get loading() {
      return loading;
    },
    get error() {
      return error;
    },
    get saving() {
      return saving;
    },
    get saveError() {
      return saveError;
    },
    get saveSuccess() {
      return saveSuccess;
    },
    get deleting() {
      return deleting;
    },
    get deleteError() {
      return deleteError;
    },
    // 方法
    loadProfiles,
    save,
    remove,
    clearSaveState,
    clearDeleteState
  };
}
