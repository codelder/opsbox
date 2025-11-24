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

  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';
  import { Search, X, Loader2, Check, ArrowDown } from 'lucide-svelte';
  import { Badge } from '$lib/components/ui/badge';

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
<div class="min-h-screen bg-background text-foreground">
  <div class="mx-auto max-w-[1560px] px-4 py-8">
    <!-- 顶部区域重新设计 -->
    <div class="mb-10">
      <!-- Logo 与描述区域 -->
      <div class="mb-6 text-center">
        <div id="logo-label" class="inline-block">
          <LogSeekLogo size="medium" asLabel htmlFor="search" hoverable />
        </div>
        <p class="mt-2 text-sm text-muted-foreground">快速搜索和浏览日志文件</p>
      </div>

      <!-- 搜索框区域 -->
      <form class="mx-auto max-w-4xl" onsubmit={handleSubmit}>
        <div class="group relative flex items-center">
          <div class="pointer-events-none absolute left-4 z-10 text-muted-foreground">
            <Search class="h-5 w-5" />
          </div>
          <Input
            id="search"
            class="h-14 rounded-2xl border-input bg-background pr-12 pl-12 text-base text-foreground shadow-lg transition-all hover:shadow-xl focus-visible:ring-primary/50"
            disabled={searchStore.loading}
            bind:value={q}
            placeholder="输入查询串或自然语言搜索…"
            autocomplete="off"
          />
          {#if searchStore.loading}
            <div class="absolute right-4 z-10">
              <Loader2 class="h-5 w-5 animate-spin text-primary" />
            </div>
          {:else if q}
            <Button
              variant="ghost"
              size="icon"
              class="absolute right-2 z-10 h-10 w-10 text-muted-foreground hover:text-foreground"
              onclick={() => {
                q = '';
                searchStore.cleanup();
              }}
              aria-label="清除搜索内容"
              type="button"
            >
              <X class="h-5 w-5" />
            </Button>
          {/if}
        </div>
      </form>
    </div>

    <!-- 结果区域 -->
    <div class="space-y-6">
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
          <div class="rounded border bg-card p-3 text-card-foreground">
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
        <Button
          size="lg"
          class="rounded-xl px-8 shadow-lg transition-all hover:shadow-xl"
          onclick={() => searchStore.loadMore()}
          disabled={searchStore.loading}
        >
          {#if searchStore.loading}
            <Loader2 class="mr-2 h-4 w-4 animate-spin" />
            {searchStore.results.length === 0 ? '搜索中…' : '加载中…'}
          {:else}
            <ArrowDown class="mr-2 h-4 w-4" />
            加载更多结果
          {/if}
        </Button>
      {:else if searchStore.results.length > 0}
        <div class="text-center">
          <Badge
            variant="outline"
            class="bg-green-50 px-4 py-1.5 text-sm text-green-700 dark:bg-green-900/30 dark:text-green-300"
          >
            <Check class="mr-2 h-4 w-4" />
            已显示所有搜索结果
          </Badge>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  /* 动态添加的临时高亮效果类，用于搜索结果卡片收起时的视觉反馈 - Used dynamically in toggleFileShowAll() */
  :global(.highlight-card) {
    border: 2px solid var(--primary);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--primary), transparent 80%);
    animation: highlight-pulse 2s ease-in-out;
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
