/**
 * 搜索状态管理 Composable
 * 提供搜索相关的状态和方法
 */

import type { SearchJsonResult } from '../types';
import { extractSessionId, startUnifiedSearch } from '../api';
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
      loading = true;
      const response = await startUnifiedSearch(q, controller.signal);
      sid = extractSessionId(response);

      // 初始化流读取器
      streamReader.initReader(response);

      // 读取第一批数据（内部会设置 loading）
      await loadMoreInternal();
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message || '搜索失败';
      hasMore = false;
      loading = false;
    }
  }

  /**
   * 内部加载方法（不检查 loading 状态）
   */
  async function loadMoreInternal(pageSize: number = 20): Promise<void> {
    if (!hasMore) return;

    loading = true;
    const { hasMore: more } = await streamReader.readBatch(
      pageSize,
      (result) => {
        // 处理搜索结果
        results = [...results, result];
      },
      (err) => {
        // 处理流错误
        error = err;
      },
      (event) => {
        // 处理错误和完成事件
        if (event.type === 'error') {
          console.warn(`[搜索] 数据源 ${event.source} 错误：${event.message}`);
          // 笔记: Error 事件不中断搜索，其他源会继续发送结果
          // 如果有必要，可以在此会变更 UI 状态，例如显示警告信息
        } else if (event.type === 'complete') {
          console.log(`[搜索] 数据源 ${event.source} 完成, 耗时 ${event.elapsed_ms}ms`);
          // 可以跟踪各源的完成情况，用于水纳模式的下载较、挺上流量计算等
        }
      }
    );

    hasMore = more;
    loading = false;
  }

  /**
   * 加载更多结果（公开方法，检查 loading 状态）
   */
  async function loadMore(pageSize: number = 20): Promise<void> {
    if (!hasMore || loading) return;
    await loadMoreInternal(pageSize);
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
