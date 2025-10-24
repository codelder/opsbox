/**
 * Agent 相关 API 封装
 */

import type { AgentListResponse, AgentTag } from '../types';
import { getAgentApiBase, commonHeaders } from './config';

/**
 * 列出 Agent（可选标签筛选与在线过滤）
 */
export async function fetchAgents(opts?: { tags?: string; onlineOnly?: boolean }): Promise<AgentListResponse> {
  const API_BASE = getAgentApiBase();
  const params = new URLSearchParams();
  if (opts?.tags && opts.tags.trim()) params.set('tags', opts.tags.trim());
  if (typeof opts?.onlineOnly === 'boolean') params.set('online_only', String(!!opts.onlineOnly));
  const qs = params.toString();
  const url = `${API_BASE}${qs ? `?${qs}` : ''}`;
  const res = await fetch(url, { headers: { Accept: 'application/json' } });
  if (!res.ok) throw new Error(`加载 Agent 列表失败：HTTP ${res.status}`);
  return await res.json();
}

/** 获取某个 Agent 的标签 */
export async function fetchAgentTags(agentId: string): Promise<AgentTag[]> {
  const API_BASE = getAgentApiBase();
  const res = await fetch(`${API_BASE}/${encodeURIComponent(agentId)}/tags`, {
    headers: { Accept: 'application/json' }
  });
  if (!res.ok) throw new Error(`加载标签失败：HTTP ${res.status}`);
  return await res.json();
}

/** 批量设置标签（覆盖） */
export async function setAgentTags(agentId: string, tags: AgentTag[]): Promise<void> {
  const API_BASE = getAgentApiBase();
  const res = await fetch(`${API_BASE}/${encodeURIComponent(agentId)}/tags`, {
    method: 'POST',
    headers: commonHeaders,
    body: JSON.stringify({ tags })
  });
  if (!res.ok) throw new Error(`设置标签失败：HTTP ${res.status}`);
}

/** 添加单个标签 */
export async function addAgentTag(agentId: string, tag: AgentTag): Promise<void> {
  const API_BASE = getAgentApiBase();
  const res = await fetch(`${API_BASE}/${encodeURIComponent(agentId)}/tags/add`, {
    method: 'POST',
    headers: commonHeaders,
    body: JSON.stringify(tag)
  });
  if (!res.ok) throw new Error(`添加标签失败：HTTP ${res.status}`);
}

/** 移除单个标签 */
export async function removeAgentTag(agentId: string, tag: AgentTag): Promise<void> {
  const API_BASE = getAgentApiBase();
  const res = await fetch(`${API_BASE}/${encodeURIComponent(agentId)}/tags/remove`, {
    method: 'DELETE',
    headers: commonHeaders,
    body: JSON.stringify(tag)
  });
  if (!res.ok) throw new Error(`移除标签失败：HTTP ${res.status}`);
}

/** 清空所有标签 */
export async function clearAgentTags(agentId: string): Promise<void> {
  const API_BASE = getAgentApiBase();
  const res = await fetch(`${API_BASE}/${encodeURIComponent(agentId)}/tags/clear`, {
    method: 'DELETE'
  });
  if (!res.ok) throw new Error(`清空标签失败：HTTP ${res.status}`);
}
