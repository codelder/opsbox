/**
 * 搜索状态管理 Composable
 * 提供搜索相关的状态和方法
 */

import type { SearchJsonResult } from '../types';
import { startSearch, extractSessionId } from '../api';
import { useStreamReader } from './useStreamReader.svelte';

/**
 * 搜索状态和方法
 */
export function useSearch() {
  let query = $state('');
  let results = $state<SearchJsonResult[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let sid = $state('');
  let hasMore = $state(true);
  let controller: AbortController | null = $state(null);

  const streamReader = useStreamReader();

  /**
   * 开始新搜索
   */
  async function search(q: string): Promise<void> {
    // 取消之前的搜索
    if (controller) {
      controller.abort();
    }

    // 重置状态
    query = q;
    results = [];
    error = null;
    sid = '';
    hasMore = true;
    controller = new AbortController();

    try {
      const response = await startSearch(q);
      sid = extractSessionId(response);

      // 初始化流读取器
      streamReader.initReader(response);

      // 读取第一批数据
      await loadMore();
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message || '搜索失败';
      hasMore = false;
    }
  }

  /**
   * 加载更多结果
   */
  async function loadMore(pageSize: number = 20): Promise<void> {
    if (!hasMore || loading) return;

    loading = true;
    const { hasMore: more } = await streamReader.readBatch(
      pageSize,
      (result) => {
        results = [...results, result];
      },
      (err) => {
        error = err;
      }
    );

    hasMore = more;
    loading = false;
  }

  /**
   * 取消搜索
   */
  function cancel(): void {
    if (controller) {
      controller.abort();
      controller = null;
    }
    streamReader.cleanup();
    hasMore = false;
    loading = false;
  }

  /**
   * 清理资源
   */
  function cleanup(): void {
    cancel();
    results = [];
    error = null;
    sid = '';
  }

  return {
    // 状态
    get query() {
      return query;
    },
    get results() {
      return results;
    },
    get loading() {
      return loading;
    },
    get error() {
      return error;
    },
    get sid() {
      return sid;
    },
    get hasMore() {
      return hasMore;
    },
    // 方法
    search,
    loadMore,
    cancel,
    cleanup
  };
}
