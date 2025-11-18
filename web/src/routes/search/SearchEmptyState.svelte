<script lang="ts">
  /**
   * 搜索空状态组件
   * 显示错误、无结果、等待输入等状态
   */
  interface Props {
    /**
     * 状态类型
     */
    type: 'error' | 'no-results' | 'initial';
    /**
     * 错误消息（仅 error 类型使用）
     */
    errorMessage?: string;
    /**
     * 重试回调（仅 error 类型使用）
     */
    onRetry?: () => void;
  }

  let { type, errorMessage, onRetry }: Props = $props();
</script>

<div class="mx-auto max-w-lg text-center">
  {#if type === 'error'}
    <div class="rounded-2xl bg-red-50 p-8 shadow-lg ring-1 ring-red-200 dark:bg-red-900/20 dark:ring-red-800/50">
      <div class="mx-auto flex h-16 w-16 items-center justify-center rounded-full bg-red-100 dark:bg-red-900/50">
        <svg class="h-8 w-8 text-red-600 dark:text-red-400" viewBox="0 0 24 24" stroke="currentColor">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.732 16.5c-.77.833.192 2.5 1.732 2.5z"
          />
        </svg>
      </div>
      <h3 class="mt-4 text-lg font-semibold text-red-900 dark:text-red-200">搜索出错</h3>
      <p class="mt-2 text-sm text-red-700 dark:text-red-300">{errorMessage || '发生未知错误'}</p>
      {#if onRetry}
        <button
          class="mt-4 inline-flex items-center rounded-lg bg-red-600 px-4 py-2 text-sm font-medium text-white transition-colors duration-200 hover:bg-red-700 focus:ring-2 focus:ring-red-500 focus:ring-offset-2 focus:outline-none"
          onclick={onRetry}
        >
          <svg class="mr-2 -ml-1 h-4 w-4" viewBox="0 0 24 24" stroke="currentColor">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
            />
          </svg>
          重新搜索
        </button>
      {/if}
    </div>
  {:else if type === 'no-results'}
    <div class="rounded-2xl bg-[var(--surface)] p-8 shadow-lg ring-1 ring-[var(--border)]">
      <div class="mx-auto flex h-16 w-16 items-center justify-center rounded-full bg-[var(--surface-2)]">
        <svg class="h-8 w-8 text-[var(--muted)]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
      </div>
      <h3 class="mt-4 text-lg font-semibold text-[var(--text)]">无匹配结果</h3>
      <p class="mt-2 text-sm text-[var(--muted)]">尝试使用不同的关键词或更广泛的搜索词汇</p>
    </div>
  {:else if type === 'initial'}
    <div class="rounded-2xl bg-[var(--surface)] p-8 shadow-lg ring-1 ring-[var(--border)]">
      <div class="mx-auto flex h-16 w-16 items-center justify-center rounded-full bg-[var(--surface-2)]">
        <svg class="h-8 w-8 text-[var(--muted)]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
      </div>
      <h3 class="mt-4 text-xl font-semibold text-[var(--text)]">开始搜索</h3>
      <p class="mt-2 text-sm text-[var(--muted)]">在上方输入框中输入关键词或自然语言查询，开始搜索日志</p>
    </div>
  {/if}
</div>
