/**
 * LLM 设置 API 客户端
 */

import type { LlmBackendListItem, LlmBackendListResponse, LlmBackendUpsertPayload } from '../types';
import { getApiBase, commonHeaders } from './config';

export async function listLlmBackends(): Promise<{ backends: LlmBackendListItem[]; defaultName: string | null }> {
  const API_BASE = getApiBase();
  const resp = await fetch(`${API_BASE}/settings/llm/backends`, { headers: { Accept: 'application/json' } });
  if (!resp.ok) {
    throw new Error(`加载大模型配置失败：HTTP ${resp.status}`);
  }
  const data: LlmBackendListResponse = await resp.json();
  return { backends: data.backends || [], defaultName: data.default ?? null };
}

export async function upsertLlmBackend(payload: LlmBackendUpsertPayload): Promise<void> {
  const API_BASE = getApiBase();
  const resp = await fetch(`${API_BASE}/settings/llm/backends`, {
    method: 'POST',
    headers: commonHeaders,
    body: JSON.stringify(payload)
  });
  if (!resp.ok) {
    let msg = `保存失败：HTTP ${resp.status}`;
    try {
      const problem = await resp.json();
      msg = problem?.detail || problem?.title || msg;
    } catch {
      /* 忽略错误，保留默认错误消息 */
    }
    throw new Error(msg);
  }
}

export async function deleteLlmBackend(name: string): Promise<void> {
  const API_BASE = getApiBase();
  const resp = await fetch(`${API_BASE}/settings/llm/backends/${encodeURIComponent(name)}`, {
    method: 'DELETE',
    headers: commonHeaders
  });
  if (!resp.ok) {
    let msg = `删除失败：HTTP ${resp.status}`;
    try {
      const problem = await resp.json();
      msg = problem?.detail || problem?.title || msg;
    } catch {
      /* 忽略错误，保留默认错误消息 */
    }
    throw new Error(msg);
  }
}

export async function getDefaultLlm(): Promise<string | null> {
  const API_BASE = getApiBase();
  const resp = await fetch(`${API_BASE}/settings/llm/default`, { headers: { Accept: 'application/json' } });
  if (!resp.ok) {
    throw new Error(`获取默认大模型失败：HTTP ${resp.status}`);
  }
  const name: string | null = await resp.json();
  return name ?? null;
}

export async function setDefaultLlm(name: string): Promise<void> {
  const API_BASE = getApiBase();
  const resp = await fetch(`${API_BASE}/settings/llm/default`, {
    method: 'POST',
    headers: commonHeaders,
    body: JSON.stringify({ name })
  });
  if (!resp.ok) {
    let msg = `设置默认大模型失败：HTTP ${resp.status}`;
    try {
      const problem = await resp.json();
      msg = problem?.detail || problem?.title || msg;
    } catch {
      /* 忽略错误，保留默认错误消息 */
    }
    throw new Error(msg);
  }
}
