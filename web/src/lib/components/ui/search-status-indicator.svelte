<script lang="ts">
  /**
   * 搜索状态指示器组件
   * 在右上角显示搜索状态图标，有错误时高亮显示
   * 点击可展开查看错误详情
   */
  import { AlertTriangle, CheckCircle, Clock, ChevronDown, ChevronUp, X } from 'lucide-svelte';
  import type { SearchStatistics } from '$lib/modules/logseek/types';
  import { Button } from '$lib/components/ui/button';
  import { Badge } from '$lib/components/ui/badge';

  interface Props {
    statistics: SearchStatistics | null;
    loading?: boolean;
  }

  let { statistics, loading = false }: Props = $props();

  let isOpen = $state(false);

  // 计算是否有错误
  const hasErrors = $derived(statistics?.failedSources ?? 0 > 0);
  const errorCount = $derived(statistics?.failedSources ?? 0);

  // 格式化耗时
  function formatElapsed(ms: number): string {
    if (ms < 1000) return `${ms}ms`;
    if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
    return `${Math.floor(ms / 60000)}m ${Math.floor((ms % 60000) / 1000)}s`;
  }

  // 截断路径显示
  function truncatePath(path: string, maxLen: number = 50): string {
    if (path.length <= maxLen) return path;
    // 尝试保留文件名
    const lastSlash = path.lastIndexOf('/');
    if (lastSlash > 0 && path.length - lastSlash < maxLen) {
      return '...' + path.slice(lastSlash);
    }
    return path.slice(0, maxLen - 3) + '...';
  }
</script>

{#if loading || statistics}
  <div class="relative">
    <!-- 触发按钮 -->
    <button
      class="flex items-center gap-1.5 rounded-full px-2 py-1 text-sm transition-colors
        {hasErrors
        ? 'bg-amber-100 text-amber-700 hover:bg-amber-200 dark:bg-amber-900/30 dark:text-amber-400 dark:hover:bg-amber-900/50'
        : 'bg-green-100 text-green-700 hover:bg-green-200 dark:bg-green-900/30 dark:text-green-400 dark:hover:bg-green-900/50'}"
      onclick={() => (isOpen = !isOpen)}
      aria-label={hasErrors ? `${errorCount} 个数据源搜索失败` : '搜索完成'}
    >
      {#if loading}
        <div class="h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent"></div>
      {:else if hasErrors}
        <AlertTriangle class="h-4 w-4" />
        <Badge variant="destructive" class="ml-0.5 h-5 px-1.5 text-xs">{errorCount}</Badge>
      {:else}
        <CheckCircle class="h-4 w-4" />
      {/if}
      {#if isOpen}
        <ChevronUp class="h-3 w-3" />
      {:else}
        <ChevronDown class="h-3 w-3" />
      {/if}
    </button>

    <!-- 展开面板 -->
    {#if isOpen && statistics}
      <div
        class="absolute top-full right-0 z-50 mt-2 w-80 rounded-lg border border-gray-200 bg-white p-4 shadow-lg dark:border-gray-700 dark:bg-gray-800"
        role="dialog"
        aria-label="搜索状态详情"
      >
        <!-- 标题 -->
        <div class="mb-3 flex items-center justify-between">
          <h3 class="text-sm font-medium text-gray-900 dark:text-gray-100">搜索状态</h3>
          <button
            class="rounded p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-600 dark:hover:bg-gray-700 dark:hover:text-gray-300"
            onclick={() => (isOpen = false)}
            aria-label="关闭"
          >
            <X class="h-4 w-4" />
          </button>
        </div>

        <!-- 统计概览 -->
        <div class="mb-3 flex items-center gap-4 text-sm text-gray-600 dark:text-gray-400">
          <span>
            <span class="font-medium text-gray-900 dark:text-gray-100">{statistics.totalSources}</span> 数据源
          </span>
          <span class="text-green-600 dark:text-green-400">
            {statistics.successfulSources} 成功
          </span>
          {#if statistics.failedSources > 0}
            <span class="text-amber-600 dark:text-amber-400">
              {statistics.failedSources} 失败
            </span>
          {/if}
        </div>

        <!-- 耗时 -->
        <div class="mb-3 flex items-center gap-1.5 text-xs text-gray-500 dark:text-gray-400">
          <Clock class="h-3.5 w-3.5" />
          <span>耗时 {formatElapsed(statistics.totalElapsedMs)}</span>
        </div>

        <!-- 错误列表 -->
        {#if statistics.errors.length > 0}
          <div class="border-t border-gray-200 pt-3 dark:border-gray-700">
            <h4 class="mb-2 text-xs font-medium tracking-wide text-gray-500 uppercase dark:text-gray-400">失败详情</h4>
            <ul class="max-h-48 space-y-2 overflow-y-auto">
              {#each statistics.errors as err}
                <li class="rounded-md bg-gray-50 p-2 dark:bg-gray-900/50">
                  <div class="flex items-start gap-2">
                    <AlertTriangle class="mt-0.5 h-3.5 w-3.5 shrink-0 text-amber-500" />
                    <div class="min-w-0 flex-1">
                      <div class="truncate font-mono text-xs text-gray-700 dark:text-gray-300" title={err.source}>
                        {truncatePath(err.source)}
                      </div>
                      <div class="mt-0.5 text-xs text-gray-500 dark:text-gray-400">
                        {err.message}
                      </div>
                    </div>
                  </div>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
      </div>
    {/if}
  </div>
{/if}
