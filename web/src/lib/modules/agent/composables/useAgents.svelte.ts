/**
 * Agent 列表与标签管理的状态封装（Svelte 5 Runes）
 */

import { fetchAgents, addAgentTag, removeAgentTag } from '../api';
import type { AgentInfo } from '../types';

export function useAgents() {
  // 列表状态
  let agents = $state<AgentInfo[]>([]);
  let total = $state(0);
  let loading = $state(false);
  let error = $state<string | null>(null);

  // 过滤条件
  let onlineOnly = $state(true);
  let tagFilter = $state(''); // 逗号分隔的 key=value 列表

  async function load(): Promise<void> {
    if (loading) return;
    loading = true;
    error = null;
    try {
      const { agents: list, total: t } = await fetchAgents({
        tags: tagFilter.trim(),
        onlineOnly
      });
      agents = list;
      total = t;
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message ?? '加载 Agent 列表失败';
    } finally {
      loading = false;
    }
  }

  async function addTag(agentId: string, key: string, value: string) {
    try {
      await addAgentTag(agentId, { key, value });
      // 简化处理：更新后整体刷新列表
      await load();
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message ?? '添加标签失败';
    }
  }

  async function removeTag(agentId: string, key: string, value: string) {
    try {
      await removeAgentTag(agentId, { key, value });
      await load();
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message ?? '移除标签失败';
    }
  }

  return {
    // 列表
    get agents() {
      return agents;
    },
    get total() {
      return total;
    },
    get loading() {
      return loading;
    },
    get error() {
      return error;
    },

    // 过滤条件
    get onlineOnly() {
      return onlineOnly;
    },
    set onlineOnly(v: boolean) {
      onlineOnly = v;
    },
    get tagFilter() {
      return tagFilter;
    },
    set tagFilter(v: string) {
      tagFilter = v;
    },

    // 方法
    load,
    addTag,
    removeTag
  };
}
