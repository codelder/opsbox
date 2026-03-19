/**
 * 搜索状态管理 Composable
 * 提供搜索相关的状态和方法
 */

import type { SearchJsonResult, SearchStatistics } from '../types';
import { extractSessionId, startUnifiedSearch, deleteSearchSession } from '../api';
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

  // 新增：错误统计
  let sourceErrors = $state<Array<{ source: string; message: string }>>([]);
  let statistics = $state<SearchStatistics | null>(null);

  const streamReader = useStreamReader();

  /**
   * 开始新搜索
   */
  async function search(q: string): Promise<void> {
    // 取消之前的搜索
    if (controller) {
      controller.abort();
    }

    // 如果有旧的 sid，清理后端缓存
    if (sid) {
      deleteSearchSession(sid);
    }

    // 重置状态
    query = q;
    results = [];
    error = null;
    sid = '';
    hasMore = true;
    controller = new AbortController();

    // 重置错误统计
    sourceErrors = [];
    statistics = null;

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
          // 收集错误信息
          sourceErrors = [
            ...sourceErrors,
            {
              source: event.source,
              message: event.message
            }
          ];
        } else if (event.type === 'complete') {
          console.info(`[搜索] 数据源 ${event.source} 完成, 耗时 ${event.elapsed_ms}ms`);
        } else if (event.type === 'finished') {
          // 全局搜索完成，更新统计信息
          statistics = {
            totalSources: event.total_sources,
            successfulSources: event.successful_sources,
            failedSources: event.failed_sources,
            errors: sourceErrors,
            totalElapsedMs: event.total_elapsed_ms
          };
          console.info(
            `[搜索] 全局完成: 总源=${event.total_sources}, 成功=${event.successful_sources}, 失败=${event.failed_sources}, 耗时=${event.total_elapsed_ms}ms`
          );
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
    if (sid) {
      deleteSearchSession(sid);
    }
    results = [];
    error = null;
    sid = '';
    sourceErrors = [];
    statistics = null;
  }

  // 监听页面关闭/刷新事件，确保清理后端会话
  $effect(() => {
    const handlePageHide = () => {
      if (sid) {
        deleteSearchSession(sid);
      }
    };

    if (typeof window !== 'undefined') {
      window.addEventListener('pagehide', handlePageHide);
    }

    return () => {
      if (typeof window !== 'undefined') {
        window.removeEventListener('pagehide', handlePageHide);
      }
    };
  });

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
    // 新增：错误统计状态
    get sourceErrors() {
      return sourceErrors;
    },
    get statistics() {
      return statistics;
    },
    // 方法
    search,
    loadMore,
    cancel,
    cleanup
  };
}
