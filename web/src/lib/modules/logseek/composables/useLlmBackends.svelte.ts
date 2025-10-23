/**
 * LLM 后端管理状态 Composable
 */

import { deleteLlmBackend, listLlmBackends, setDefaultLlm, upsertLlmBackend } from '../api';
import type { LlmBackendListItem, LlmBackendUpsertPayload } from '../types';

export function useLlmBackends() {
  let backends = $state<LlmBackendListItem[]>([]);
  let defaultName = $state<string | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let saveSuccess = $state(false);
  let deleting = $state(false);
  let deleteError = $state<string | null>(null);
  let settingDefault = $state(false);
  let setDefaultError = $state<string | null>(null);

  async function load(): Promise<void> {
    if (loading) return;
    loading = true;
    error = null;
    try {
      const data = await listLlmBackends();
      backends = data.backends;
      defaultName = data.defaultName;
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message ?? '无法加载大模型配置';
      backends = [];
      defaultName = null;
    } finally {
      loading = false;
    }
  }

  async function save(payload: LlmBackendUpsertPayload): Promise<boolean> {
    if (saving) return false;
    saving = true;
    saveError = null;
    saveSuccess = false;
    try {
      await upsertLlmBackend(payload);
      await load();
      saveSuccess = true;
      return true;
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      saveError = err.message ?? '保存失败';
      return false;
    } finally {
      saving = false;
    }
  }

  async function remove(name: string): Promise<boolean> {
    if (deleting) return false;
    deleting = true;
    deleteError = null;
    try {
      await deleteLlmBackend(name);
      await load();
      return true;
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      deleteError = err.message ?? '删除失败';
      return false;
    } finally {
      deleting = false;
    }
  }

  async function makeDefault(name: string): Promise<boolean> {
    if (settingDefault) return false;
    settingDefault = true;
    setDefaultError = null;
    try {
      await setDefaultLlm(name);
      defaultName = name;
      return true;
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      setDefaultError = err.message ?? '设置默认失败';
      return false;
    } finally {
      settingDefault = false;
    }
  }

  function clearSaveState() {
    saveError = null;
    saveSuccess = false;
  }

  function clearDeleteState() {
    deleteError = null;
  }

  return {
    // 状态
    get backends() {
      return backends;
    },
    get defaultName() {
      return defaultName;
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
    get settingDefault() {
      return settingDefault;
    },
    get setDefaultError() {
      return setDefaultError;
    },
    // 方法
    load,
    save,
    remove,
    makeDefault,
    clearSaveState,
    clearDeleteState
  };
}
