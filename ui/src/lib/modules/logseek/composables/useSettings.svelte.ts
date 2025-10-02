/**
 * 设置状态管理 Composable
 * 提供 S3 对象存储设置相关的状态和方法
 */

import { fetchS3Settings, saveS3Settings } from '../api';
import type { S3SettingsPayload } from '../types';

/**
 * 设置状态和方法
 */
export function useSettings() {
  let endpoint = $state('');
  let bucket = $state('');
  let accessKey = $state('');
  let secretKey = $state('');
  let loadingSettings = $state(false);
  let loadError = $state<string | null>(null);
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let saveSuccess = $state(false);
  let loadedOnce = $state(false);
  let connectionError = $state<string | null>(null);

  /**
   * 加载设置
   */
  async function loadSettings(force: boolean = false): Promise<void> {
    if (loadingSettings || (loadedOnce && !force)) return;

    loadingSettings = true;
    loadError = null;
    connectionError = null;

    try {
      const data = await fetchS3Settings();
      endpoint = data.endpoint ?? '';
      bucket = data.bucket ?? '';
      accessKey = data.access_key ?? '';
      secretKey = data.secret_key ?? '';
      connectionError = data.connection_error ?? null;
      loadedOnce = true;
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      loadError = err.message ?? '无法读取设置';
    } finally {
      loadingSettings = false;
    }
  }

  /**
   * 保存设置
   */
  async function save(): Promise<void> {
    if (saving) return;

    saving = true;
    saveError = null;
    saveSuccess = false;
    connectionError = null;

    try {
      const payload: S3SettingsPayload = {
        endpoint,
        bucket,
        access_key: accessKey,
        secret_key: secretKey
      };

      await saveS3Settings(payload);
      await loadSettings(true);
      connectionError = null;
      saveSuccess = true;
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      saveError = err.message ?? '保存设置失败';
      connectionError = err.message ?? '保存设置失败';
    } finally {
      saving = false;
    }
  }

  return {
    // 状态
    get endpoint() {
      return endpoint;
    },
    set endpoint(value: string) {
      endpoint = value;
    },
    get bucket() {
      return bucket;
    },
    set bucket(value: string) {
      bucket = value;
    },
    get accessKey() {
      return accessKey;
    },
    set accessKey(value: string) {
      accessKey = value;
    },
    get secretKey() {
      return secretKey;
    },
    set secretKey(value: string) {
      secretKey = value;
    },
    get loadingSettings() {
      return loadingSettings;
    },
    get loadError() {
      return loadError;
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
    get loadedOnce() {
      return loadedOnce;
    },
    get connectionError() {
      return connectionError;
    },
    // 方法
    loadSettings,
    save
  };
}
