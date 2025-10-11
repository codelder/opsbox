/**
 * 设置 API 客户端
 * 封装 S3 对象存储设置相关的 API 调用
 */

import type { S3SettingsPayload, S3SettingsResponse } from '../types';
import { getApiBase, commonHeaders } from './config';

/**
 * 获取 S3 对象存储设置
 */
export async function fetchS3Settings(): Promise<S3SettingsResponse> {
  const API_BASE = getApiBase();
  const response = await fetch(`${API_BASE}/settings/s3`, {
    headers: { Accept: 'application/json' }
  });

  if (!response.ok) {
    throw new Error(`加载设置失败：HTTP ${response.status}`);
  }

  return await response.json();
}

/**
 * 保存 S3 对象存储设置
 */
export async function saveS3Settings(settings: S3SettingsPayload): Promise<void> {
  const API_BASE = getApiBase();
  const response = await fetch(`${API_BASE}/settings/s3`, {
    method: 'POST',
    headers: commonHeaders,
    body: JSON.stringify(settings)
  });

  if (!response.ok) {
    // 尝试解析 RFC 7807 Problem Details
    let message = `保存失败：HTTP ${response.status}`;
    try {
      const problem = await response.json();
      message = problem?.detail || problem?.title || message;
    } catch {
      // 忽略 JSON 解析错误，保留默认消息
    }
    throw new Error(message);
  }

  // 后端返回 204 No Content，无需解析响应体
}
