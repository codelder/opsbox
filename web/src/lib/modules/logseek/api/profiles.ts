/**
 * Profile API 客户端
 * 封装 S3 Profile 管理相关的 API 调用
 */

import type { S3ProfilePayload, S3ProfileListResponse } from '../types';
import { getApiBase, commonHeaders } from './config';

/**
 * 获取所有 S3 Profiles
 */
export async function listProfiles(): Promise<S3ProfilePayload[]> {
  const API_BASE = getApiBase();
  const response = await fetch(`${API_BASE}/profiles`, {
    headers: { Accept: 'application/json' }
  });

  if (!response.ok) {
    throw new Error(`获取 Profile 列表失败：HTTP ${response.status}`);
  }

  const data: S3ProfileListResponse = await response.json();
  return data.profiles || [];
}

/**
 * 保存或更新 S3 Profile
 */
export async function saveProfile(profile: S3ProfilePayload): Promise<void> {
  const API_BASE = getApiBase();
  const response = await fetch(`${API_BASE}/profiles`, {
    method: 'POST',
    headers: commonHeaders,
    body: JSON.stringify(profile)
  });

  if (!response.ok) {
    // 尝试解析 RFC 7807 Problem Details
    let message = `保存 Profile 失败：HTTP ${response.status}`;
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

/**
 * 删除 S3 Profile
 */
export async function deleteProfile(profileName: string): Promise<void> {
  const API_BASE = getApiBase();
  const response = await fetch(`${API_BASE}/profiles/${encodeURIComponent(profileName)}`, {
    method: 'DELETE',
    headers: commonHeaders
  });

  if (!response.ok) {
    // 尝试解析 RFC 7807 Problem Details
    let message = `删除 Profile 失败：HTTP ${response.status}`;
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
