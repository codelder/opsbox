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
<div class="min-h-screen bg-(--bg)">
  <div class="mx-auto max-w-[1560px] px-4 py-8">
    <!-- 顶部区域重新设计 -->
    <div class="mb-10">
      <!-- Logo 与描述区域 -->
      <div class="mb-6 text-center">
        <div id="logo-label" class="inline-block">
          <LogSeekLogo size="medium" asLabel htmlFor="search" hoverable />
        </div>
        <p class="mt-2 text-sm text-[var(--muted)]">快速搜索和浏览日志文件</p>
      </div>

      <!-- 搜索框区域 -->
      <form class="mx-auto max-w-4xl" onsubmit={handleSubmit}>
        <div class="group relative">
          <div class="pointer-events-none absolute inset-y-0 left-0 z-10 flex items-center pl-5">
            <svg
              class="h-5 w-5 text-[var(--muted)] transition-colors duration-200 group-focus-within:text-[var(--primary)]"
              viewBox="0 0 24 24"
              stroke="currentColor"
              fill="none"
              stroke-width="2"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="m21 21-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
          </div>
          <input
            id="search"
            class="h-14 w-full rounded-2xl border border-[var(--border)] bg-[var(--surface)] py-4 pr-16 pl-14 text-base text-[var(--text)] placeholder-[var(--muted)] shadow-lg shadow-black/5 transition-all duration-300 hover:border-[var(--primary)]/50 hover:shadow-xl hover:shadow-black/10 focus:border-[var(--primary)] focus:ring-4 focus:ring-[var(--ring)] focus:outline-none disabled:opacity-60"
            disabled={searchStore.loading}
            bind:value={q}
            placeholder="输入查询串或自然语言搜索…"
            autocomplete="off"
          />
          {#if searchStore.loading}
            <div class="absolute inset-y-0 right-0 flex items-center pr-5">
              <div
                class="h-5 w-5 animate-spin rounded-full border-2 border-[var(--primary)] border-t-transparent"
              ></div>
            </div>
          {:else if q}
            <button
              type="button"
              class="absolute inset-y-0 right-0 flex items-center pr-5 text-[var(--muted)] transition-colors duration-200 hover:text-[var(--text)]"
              onclick={() => {
                q = '';
                searchStore.cleanup();
              }}
              aria-label="清除搜索内容"
            >
              <svg class="h-5 w-5" viewBox="0 0 24 24" stroke="currentColor" fill="none" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
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
      {#if searchStore.error && !searchStore.loading}
        <SearchEmptyState
          type="error"
          errorMessage={searchStore.error}
          onRetry={() => {
            if (q) searchStore.search(q);
          }}
        />
      {:else if !searchStore.loading && searchStore.results.length === 0 && q && !searchStore.hasMore && !searchStore.error}
        <SearchEmptyState type="no-results" />
      {:else if !searchStore.loading && !searchStore.error && searchStore.results.length === 0 && !q}
        <SearchEmptyState type="initial" />
      {/if}
    </div>

    <!-- 分页控制按钮 -->
    <div class="mt-10 flex items-center justify-center">
      {#if searchStore.hasMore}
        <button
          class="group inline-flex items-center rounded-xl bg-[var(--primary)] px-6 py-3 text-sm font-semibold text-[var(--primary-foreground)] shadow-lg shadow-black/10 transition-all duration-300 hover:opacity-90 hover:shadow-xl hover:shadow-black/15 focus:ring-4 focus:ring-[var(--ring)] focus:outline-none disabled:opacity-50"
          onclick={() => searchStore.loadMore()}
          disabled={searchStore.loading}
        >
          {#if searchStore.loading}
            <div
              class="mr-2 h-4 w-4 animate-spin rounded-full border-2 border-[var(--primary-foreground)] border-t-transparent"
            ></div>
            {searchStore.results.length === 0 ? '搜索中…' : '加载中…'}
          {:else}
            <svg
              class="mr-2 h-4 w-4 transition-transform duration-200 group-hover:translate-y-0.5"
              viewBox="0 0 24 24"
              stroke="currentColor"
              fill="none"
              stroke-width="2"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
            </svg>
            加载更多结果
          {/if}
        </button>
      {:else if searchStore.results.length > 0}
        <div class="text-center">
          <div
            class="inline-flex items-center rounded-full bg-green-50 px-5 py-2 text-sm font-medium text-green-700 ring-1 ring-green-200 dark:bg-green-900/30 dark:text-green-300 dark:ring-green-800/50"
          >
            <svg class="mr-2 h-4 w-4" viewBox="0 0 24 24" stroke="currentColor" fill="none" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
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
