<script lang="ts">
  /**
   * 搜索页面（重构版）
   * 使用 LogSeek 模块的 composables 和工具函数
   */
  import { SvelteSet } from 'svelte/reactivity';
  import { useSearch } from '$lib/modules/logseek';
  import LogSeekLogo from '$lib/components/LogSeekLogo.svelte';
  import SearchResultCard from './SearchResultCard.svelte';
  import SearchEmptyState from './SearchEmptyState.svelte';

  // 使用 composable 管理搜索状态
  const searchStore = useSearch();

  // 本地输入框状态
  let q = $state('');

  // 每个结果的 UI 状态（折叠、展开所有匹配、单行展开）
  const collapsedFiles = new SvelteSet<number>();
  const expandedAllMatches = new SvelteSet<number>();
  const expandedLines = new SvelteSet<string>();
  function isFileCollapsed(i: number) {
    return collapsedFiles.has(i);
  }
  function toggleFileCollapsed(i: number) {
    if (collapsedFiles.has(i)) collapsedFiles.delete(i);
    else collapsedFiles.add(i);
  }
  function isFileShowAll(i: number) {
    return expandedAllMatches.has(i);
  }
  function toggleFileShowAll(i: number) {
    const wasExpanded = expandedAllMatches.has(i);
    if (wasExpanded) {
      expandedAllMatches.delete(i);
      // 收起时，延迟滚动以等待DOM更新
      setTimeout(() => {
        const cardElement = document.querySelector(`[data-result-card="${i}"]`);
        if (cardElement) {
          cardElement.scrollIntoView({
            behavior: 'smooth',
            block: 'start',
            inline: 'nearest'
          });
          // 添加临时高亮效果
          cardElement.classList.add('highlight-card');
          setTimeout(() => {
            cardElement.classList.remove('highlight-card');
          }, 2000);
        }
      }, 100);
    } else {
      expandedAllMatches.add(i);
    }
  }
  function expandLine(key: string) {
    expandedLines.add(key);
  }

  // 从地址栏读取 ?q=，并在客户端启动搜索
  let searchInit = $state(false);
  $effect(() => {
    if (searchInit) return;
    searchInit = true;
    const params = new URL(window.location.href).searchParams;
    const initial = (params.get('q') || '').trim();
    q = initial;
    if (initial) searchStore.search(initial);
  });

  // 卸载清理
  $effect(() => {
    return () => {
      searchStore.cleanup();
    };
  });

  // 表单提交
  function handleSubmit(e: Event) {
    e.preventDefault();
    const next = q.trim();
    if (!next) return;
    searchStore.search(next);
  }
</script>

<!-- 页面标题与状态栏 -->
<div class="min-h-screen bg-gradient-to-br from-slate-100 to-gray-200 dark:from-gray-900 dark:to-gray-800">
  <div class="mx-auto max-w-[1560px] px-4 py-8">
    <!-- 顶部区域重新设计 -->
    <div class="mb-12">
      <!-- Logo 区域 -->
      <div class="mb-8 text-center">
        <div id="logo-label">
          <LogSeekLogo size="medium" asLabel htmlFor="search" hoverable />
        </div>
        <p class="mt-3 text-lg font-medium text-gray-600 dark:text-gray-300">快速搜索和浏览日志文件</p>
      </div>

      <!-- 搜索框区域 -->
      <form class="mx-auto max-w-4xl" onsubmit={handleSubmit}>
        <div class="group relative">
          <div class="pointer-events-none absolute inset-y-0 left-0 z-10 flex items-center pl-4">
            <svg
              class="h-6 w-6 text-gray-400 transition-colors duration-200 group-focus-within:text-blue-500"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path
                stroke-linecap="round"
                fill="none"
                stroke-linejoin="round"
                stroke-width="2"
                d="m21 21-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
              />
            </svg>
          </div>
          <input
            id="search"
            class="h-16 w-full rounded-3xl border-0 bg-white/90 py-4 pr-16 pl-14 text-lg placeholder-gray-500 shadow-2xl ring-1 shadow-gray-200/50 ring-gray-200/50 backdrop-blur-sm transition-all duration-300 hover:shadow-blue-200/25 hover:ring-blue-300/50 focus:bg-white focus:ring-2 focus:shadow-blue-300/30 focus:ring-blue-500 focus:outline-none dark:bg-gray-800/90 dark:text-white dark:placeholder-gray-400 dark:shadow-gray-900/30 dark:ring-gray-600/50 dark:hover:ring-blue-400/50 dark:focus:ring-blue-400"
            disabled={searchStore.loading}
            bind:value={q}
            placeholder="输入查询串或自然语言搜索…"
            autocomplete="off"
          />
          {#if searchStore.loading}
            <div class="absolute inset-y-0 right-0 flex items-center pr-5">
              <div class="h-7 w-7 animate-spin rounded-full border-2 border-blue-600 border-t-transparent"></div>
            </div>
          {:else if q}
            <button
              type="button"
              class="absolute inset-y-0 right-0 flex items-center pr-5 text-gray-400 transition-colors duration-200 hover:text-gray-600"
              onclick={() => {
                q = '';
                searchStore.cleanup();
              }}
              aria-label="清除搜索内容"
            >
              <svg class="h-6 w-6" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          {/if}
        </div>
      </form>
    </div>

    <!-- 结果区域 -->
    <div class="space-y-8">
      {#each searchStore.results as item, i (item.path + '-' + i)}
        {#if item && item.path && item.chunks}
          <SearchResultCard
            {item}
            index={i}
            sid={searchStore.sid}
            isCollapsed={isFileCollapsed(i)}
            isShowAll={isFileShowAll(i)}
            {expandedLines}
            onToggleCollapse={() => toggleFileCollapsed(i)}
            onToggleShowAll={() => toggleFileShowAll(i)}
            onExpandLine={expandLine}
          />
        {:else}
          <!-- 兼容其他对象：兜底显示 -->
          <div class="rounded border border-gray-200 bg-white p-3 dark:border-gray-700 dark:bg-gray-800">
            <pre class="text-sm leading-relaxed break-all whitespace-pre-wrap">{JSON.stringify(item, null, 2)}</pre>
          </div>
        {/if}
      {/each}

      <!-- 空状态和错误状态 -->
      {#if searchStore.error}
        <SearchEmptyState
          type="error"
          errorMessage={searchStore.error}
          onRetry={() => {
            if (q) searchStore.search(q);
          }}
        />
      {:else if !searchStore.loading && searchStore.results.length === 0 && q && !searchStore.hasMore}
        <SearchEmptyState type="no-results" />
      {:else if !searchStore.loading && !searchStore.error && searchStore.results.length === 0 && !q}
        <SearchEmptyState type="initial" />
      {/if}
    </div>

    <!-- 分页控制按钮 -->
    <div class="mt-12 flex items-center justify-center">
      {#if searchStore.hasMore}
        <button
          class="group inline-flex items-center rounded-2xl bg-gradient-to-r from-blue-600 to-blue-700 px-8 py-4 text-base font-semibold text-white shadow-xl shadow-blue-500/25 transition-all duration-300 hover:from-blue-700 hover:to-blue-800 hover:shadow-2xl hover:shadow-blue-500/30 focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:ring-offset-gray-50 focus:outline-none disabled:from-gray-400 disabled:to-gray-500 disabled:shadow-gray-400/25 dark:focus:ring-offset-gray-900 dark:disabled:from-gray-600 dark:disabled:to-gray-700"
          onclick={() => searchStore.loadMore()}
          disabled={searchStore.loading}
        >
          {#if searchStore.loading}
            <div class="mr-3 h-5 w-5 animate-spin rounded-full border-2 border-white border-t-transparent"></div>
            加载中…
          {:else}
            <svg
              class="mr-3 h-5 w-5 transition-transform duration-200 group-hover:translate-y-0.5"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
            </svg>
            加载更多结果
          {/if}
        </button>
      {:else if searchStore.results.length > 0}
        <div class="text-center">
          <div
            class="inline-flex items-center rounded-full bg-green-100 px-6 py-3 text-sm font-medium text-green-800 ring-1 ring-green-200 dark:bg-green-900/30 dark:text-green-300 dark:ring-green-800/50"
          >
            <svg class="mr-2 h-4 w-4" viewBox="0 0 24 24" stroke="currentColor">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
            已显示所有搜索结果
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  /* 动态添加的临时高亮效果类，用于搜索结果卡片收起时的视觉反馈 - Used dynamically in toggleFileShowAll() */
  :global(.highlight-card) {
    border: 3px solid rgba(59, 130, 246, 0.5);
    box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.2);
    animation: highlight-pulse 2s ease-in-out;
  }

  /* 动态添加的临时高亮效果类，用于搜索结果卡片收起时的视觉反馈 - Used dynamically in toggleFileShowAll() */
  :global(.dark .highlight-card) {
    border: 3px solid rgba(96, 165, 250, 0.5);
    box-shadow: 0 0 0 2px rgba(96, 165, 250, 0.2);
  }

  @keyframes highlight-pulse {
    0%,
    100% {
      transform: scale(1);
      opacity: 1;
    }
    50% {
      transform: scale(1.002);
      opacity: 0.95;
    }
  }
</style>
