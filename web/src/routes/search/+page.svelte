<script lang="ts">
  /**
   * 搜索页面（重构版）
   * 仿 GitHub 代码搜索布局
   */
  import { SvelteSet } from 'svelte/reactivity';
  import { useSearch } from '$lib/modules/logseek';
  import LogSeekLogo from '$lib/components/LogSeekLogo.svelte';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import SearchResultCard from './SearchResultCard.svelte';
  import SearchEmptyState from './SearchEmptyState.svelte';

  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';
  import {
    Search,
    X,
    Loader2,
    HardDrive,
    Cloud,
    Server,
    Archive,
    FileText,
    Folder,
    ChevronRight,
    ChevronDown
  } from 'lucide-svelte';
  import { Badge } from '$lib/components/ui/badge';
  import { Separator } from '$lib/components/ui/separator';

  // 使用 composable 管理搜索状态
  const searchStore = useSearch();

  // 本地输入框状态
  let q = $state('');

  // 当前选中的筛选器
  let selectedSource = $state<string | null>(null); // 'S3' | 'Agent' | 'Local'
  let selectedSubItem = $state<string | null>(null); // 二级菜单的key（归档名/目录/agentId）

  // 展开状态
  const expandedSources = new SvelteSet<string>();

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

  // ============ 路径解析逻辑 ============

  // 解析路径，提取数据源类型和二级分类key
  interface ParsedPath {
    sourceType: 'S3' | 'Agent' | 'Local';
    subKey: string; // 二级菜单的key
    subLabel: string; // 二级菜单的显示名称
    subType: 'archive' | 'dir' | 'agent'; // 二级类型
  }

  function parsePath(path: string): ParsedPath {
    // S3 归档文件: tar.gz+s3://profile:bucket/path/archive.tar.gz:internal/path
    if (path.includes('+s3://') && path.includes('.tar.gz')) {
      const match = path.match(/\+s3:\/\/[^/]+\/(.+?\.tar\.gz)/);
      const archiveName = match ? match[1].split('/').pop() || path : path;
      return {
        sourceType: 'S3',
        subKey: archiveName,
        subLabel: archiveName,
        subType: 'archive'
      };
    }

    // 普通 S3 文件: s3://profile:bucket/path/file
    if (path.startsWith('s3://')) {
      // 提取 bucket 作为二级分类
      const match = path.match(/^s3:\/\/([^/]+)/);
      const bucket = match ? match[1] : 'default';
      return {
        sourceType: 'S3',
        subKey: bucket,
        subLabel: bucket,
        subType: 'dir'
      };
    }

    // Agent 文件: agent://agent-id/path/file
    if (path.startsWith('agent://')) {
      const match = path.match(/^agent:\/\/([^/]+)/);
      const agentId = match ? match[1] : 'unknown';
      return {
        sourceType: 'Agent',
        subKey: agentId,
        subLabel: agentId,
        subType: 'agent'
      };
    }

    // 本地目录文件: dir+file:///base/path:relative/file 或 file:///path/file
    if (path.startsWith('dir+file://')) {
      const match = path.match(/^dir\+file:\/\/([^:]+)/);
      const dirPath = match ? match[1] : '/';
      return {
        sourceType: 'Local',
        subKey: dirPath,
        subLabel: dirPath,
        subType: 'dir'
      };
    }

    // 普通本地文件: file:///path/file
    if (path.startsWith('file://')) {
      // 提取目录路径作为二级分类
      const filePath = path.replace('file://', '');
      const parts = filePath.split('/');
      parts.pop(); // 移除文件名
      const dirPath = parts.join('/') || '/';
      return {
        sourceType: 'Local',
        subKey: dirPath,
        subLabel: dirPath,
        subType: 'dir'
      };
    }

    // 默认当作本地文件
    return {
      sourceType: 'Local',
      subKey: '/',
      subLabel: '/',
      subType: 'dir'
    };
  }

  function getSourceIcon(type: string) {
    switch (type) {
      case 'S3':
        return Cloud;
      case 'Agent':
        return Server;
      case 'Local':
        return HardDrive;
      default:
        return FileText;
    }
  }

  function getSubTypeIcon(subType: string) {
    switch (subType) {
      case 'archive':
        return Archive;
      case 'agent':
        return Server;
      case 'dir':
        return Folder;
      default:
        return FileText;
    }
  }

  // ============ 统计逻辑 ============

  // 树形统计结构
  interface SourceNode {
    type: 'S3' | 'Agent' | 'Local';
    count: number;
    children: Map<string, { label: string; count: number; subType: string }>;
  }

  let sourceTree = $derived.by(() => {
    const tree: Record<string, SourceNode> = {
      S3: { type: 'S3', count: 0, children: new Map() },
      Agent: { type: 'Agent', count: 0, children: new Map() },
      Local: { type: 'Local', count: 0, children: new Map() }
    };

    for (const res of searchStore.results) {
      const parsed = parsePath(res.path);
      const node = tree[parsed.sourceType];
      node.count += 1;

      const existing = node.children.get(parsed.subKey);
      if (existing) {
        existing.count += 1;
      } else {
        node.children.set(parsed.subKey, {
          label: parsed.subLabel,
          count: 1,
          subType: parsed.subType
        });
      }
    }

    return tree;
  });

  let totalCount = $derived(searchStore.results.length);

  // 筛选后的结果
  let filteredResults = $derived.by(() => {
    return searchStore.results.filter((res) => {
      const parsed = parsePath(res.path);
      if (selectedSource && parsed.sourceType !== selectedSource) return false;
      if (selectedSubItem && parsed.subKey !== selectedSubItem) return false;
      return true;
    });
  });

  let filteredCount = $derived(filteredResults.length);

  // ============ 交互逻辑 ============

  // 切换一级菜单展开/收起
  function toggleSourceExpand(source: string) {
    if (expandedSources.has(source)) {
      expandedSources.delete(source);
    } else {
      expandedSources.add(source);
    }
  }

  // 选择一级菜单（筛选）
  function selectSource(source: string) {
    if (selectedSource === source && !selectedSubItem) {
      // 再次点击取消选中
      selectedSource = null;
    } else {
      selectedSource = source;
      selectedSubItem = null;
      // 自动展开
      expandedSources.add(source);
    }
  }

  // 选择二级菜单（筛选）
  function selectSubItem(source: string, subKey: string) {
    if (selectedSource === source && selectedSubItem === subKey) {
      // 再次点击取消选中
      selectedSubItem = null;
    } else {
      selectedSource = source;
      selectedSubItem = subKey;
    }
  }

  // 清除所有筛选
  function clearFilters() {
    selectedSource = null;
    selectedSubItem = null;
  }

  // 格式化数字显示
  function formatCount(count: number): string {
    if (count >= 1000000) return `${(count / 1000000).toFixed(1)}M`;
    if (count >= 1000) return `${(count / 1000).toFixed(1)}k`;
    return count.toString();
  }

  // 截断长路径显示
  function truncatePath(path: string, maxLen: number = 30): string {
    if (path.length <= maxLen) return path;
    return '...' + path.slice(-maxLen + 3);
  }
</script>

<div class="min-h-screen bg-background text-foreground">
  <!-- 顶部导航栏 -->
  <header
    class="sticky top-0 z-50 w-full border-b border-border bg-background/95 backdrop-blur supports-backdrop-filter:bg-background/60"
  >
    <div class="flex h-16 w-full items-center gap-4 px-6">
      <!-- Logo -->
      <a href="/" class="flex items-center gap-2 transition-opacity hover:opacity-80">
        <LogSeekLogo size="small" />
      </a>

      <!-- 搜索框 -->
      <form class="ml-4 flex-1" onsubmit={handleSubmit}>
        <div class="group relative flex items-center">
          <div class="pointer-events-none absolute left-3 z-10 text-muted-foreground">
            <Search class="h-4 w-4" />
          </div>
          <Input
            id="search"
            class="h-9 rounded-md border-input bg-muted/50 pr-9 pl-9 text-sm text-foreground shadow-none transition-all hover:bg-muted focus-visible:bg-background focus-visible:ring-1 focus-visible:ring-primary"
            disabled={searchStore.loading}
            bind:value={q}
            placeholder="搜索..."
            autocomplete="off"
          />
          {#if searchStore.loading}
            <div class="absolute right-3 z-10">
              <Loader2 class="h-3.5 w-3.5 animate-spin text-primary" />
            </div>
          {:else if q}
            <Button
              variant="ghost"
              size="icon"
              class="absolute right-1 z-10 h-7 w-7 text-muted-foreground hover:text-foreground"
              onclick={() => {
                q = '';
                searchStore.cleanup();
              }}
              aria-label="清除搜索内容"
              type="button"
            >
              <X class="h-3.5 w-3.5" />
            </Button>
          {/if}
        </div>
      </form>

      <!-- 右侧操作区 -->
      <div class="ml-auto flex items-center gap-2">
        <a
          href="/settings"
          aria-label="打开设置"
          class="inline-flex h-9 w-9 items-center justify-center rounded-full bg-white/80 text-gray-900 shadow-sm backdrop-blur select-none hover:bg-white focus:ring-2 focus:ring-blue-500 focus:outline-none dark:bg-gray-800/80 dark:text-gray-100 dark:hover:bg-gray-800"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            class="h-5 w-5"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 0 0 2.573 1.066c1.543-.89 3.31.876 2.42 2.42a1.724 1.724 0 0 0 1.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 0 0-1.066 2.573c.89 1.543-.876 3.31-2.42 2.42a1.724 1.724 0 0 0-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 0 0-2.573-1.066c-1.543.89-3.31-.876-2.42-2.42a1.724 1.724 0 0 0-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 0 0 1.066-2.573c-.89-1.543.876-3.31 2.42-2.42.996.575 2.245.021 2.572-1.065z"
            />
            <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0z" />
          </svg>
        </a>
        <ThemeToggle />
      </div>
    </div>
  </header>

  <div class="w-full px-6 py-6">
    <div class="grid grid-cols-1 gap-8 md:grid-cols-[280px_1fr]">
      <!-- 左侧边栏：统计与筛选 -->
      <aside class="hidden md:block">
        <div class="sticky top-24 space-y-6">
          <div>
            <h3 class="mb-3 text-sm font-semibold text-foreground">Filter by</h3>
            <Separator class="mb-4" />

            <div class="space-y-1">
              <!-- S3 -->
              <div>
                <div class="flex items-center">
                  <!-- 展开/收起按钮 -->
                  <button
                    class="flex h-6 w-6 items-center justify-center rounded text-muted-foreground hover:bg-muted/50 hover:text-foreground {sourceTree
                      .S3.count === 0
                      ? 'invisible'
                      : ''}"
                    onclick={() => toggleSourceExpand('S3')}
                  >
                    {#if expandedSources.has('S3')}
                      <ChevronDown class="h-3.5 w-3.5" />
                    {:else}
                      <ChevronRight class="h-3.5 w-3.5" />
                    {/if}
                  </button>
                  <!-- 一级菜单项 -->
                  <button
                    class="group flex flex-1 items-center justify-between rounded-md px-2 py-1.5 text-sm transition-colors {selectedSource ===
                      'S3' && !selectedSubItem
                      ? 'bg-primary/10 font-medium text-primary'
                      : 'text-foreground hover:bg-muted/50'}"
                    onclick={() => selectSource('S3')}
                  >
                    <div class="flex items-center gap-2">
                      <Cloud
                        class="h-4 w-4 {selectedSource === 'S3'
                          ? 'text-primary'
                          : 'text-muted-foreground group-hover:text-foreground'}"
                      />
                      <span>S3</span>
                    </div>
                    <Badge
                      variant={selectedSource === 'S3' && !selectedSubItem ? 'default' : 'secondary'}
                      class="rounded-full px-2 py-0.5 text-xs font-medium">{formatCount(sourceTree.S3.count)}</Badge
                    >
                  </button>
                </div>
                <!-- 二级菜单 -->
                {#if expandedSources.has('S3') && sourceTree.S3.children.size > 0}
                  <div class="mt-1 ml-6 space-y-0.5 border-l border-border pl-2">
                    {#each Array.from(sourceTree.S3.children.entries()).sort((a, b) => b[1].count - a[1].count) as [subKey, subInfo]}
                      {@const SubIcon = getSubTypeIcon(subInfo.subType)}
                      {@const isSubSelected = selectedSource === 'S3' && selectedSubItem === subKey}
                      <button
                        class="group flex w-full items-center justify-between rounded-md px-2 py-1 text-sm transition-colors {isSubSelected
                          ? 'bg-primary/10 font-medium text-primary'
                          : 'text-muted-foreground hover:bg-muted/30 hover:text-foreground'}"
                        onclick={() => selectSubItem('S3', subKey)}
                        title={subInfo.label}
                      >
                        <div class="flex items-center gap-2 overflow-hidden">
                          <SubIcon
                            class="h-3.5 w-3.5 shrink-0 {isSubSelected ? 'text-primary' : 'text-muted-foreground'}"
                          />
                          <span class="truncate text-xs">{truncatePath(subInfo.label, 25)}</span>
                        </div>
                        <span
                          class="ml-2 shrink-0 text-xs {isSubSelected
                            ? 'font-medium text-primary'
                            : 'text-muted-foreground'}">{formatCount(subInfo.count)}</span
                        >
                      </button>
                    {/each}
                  </div>
                {/if}
              </div>

              <!-- Agent -->
              <div>
                <div class="flex items-center">
                  <button
                    class="flex h-6 w-6 items-center justify-center rounded text-muted-foreground hover:bg-muted/50 hover:text-foreground {sourceTree
                      .Agent.count === 0
                      ? 'invisible'
                      : ''}"
                    onclick={() => toggleSourceExpand('Agent')}
                  >
                    {#if expandedSources.has('Agent')}
                      <ChevronDown class="h-3.5 w-3.5" />
                    {:else}
                      <ChevronRight class="h-3.5 w-3.5" />
                    {/if}
                  </button>
                  <button
                    class="group flex flex-1 items-center justify-between rounded-md px-2 py-1.5 text-sm transition-colors {selectedSource ===
                      'Agent' && !selectedSubItem
                      ? 'bg-primary/10 font-medium text-primary'
                      : 'text-foreground hover:bg-muted/50'}"
                    onclick={() => selectSource('Agent')}
                  >
                    <div class="flex items-center gap-2">
                      <Server
                        class="h-4 w-4 {selectedSource === 'Agent'
                          ? 'text-primary'
                          : 'text-muted-foreground group-hover:text-foreground'}"
                      />
                      <span>Agent</span>
                    </div>
                    <Badge
                      variant={selectedSource === 'Agent' && !selectedSubItem ? 'default' : 'secondary'}
                      class="rounded-full px-2 py-0.5 text-xs font-medium">{formatCount(sourceTree.Agent.count)}</Badge
                    >
                  </button>
                </div>
                {#if expandedSources.has('Agent') && sourceTree.Agent.children.size > 0}
                  <div class="mt-1 ml-6 space-y-0.5 border-l border-border pl-2">
                    {#each Array.from(sourceTree.Agent.children.entries()).sort((a, b) => b[1].count - a[1].count) as [subKey, subInfo]}
                      {@const SubIcon = getSubTypeIcon(subInfo.subType)}
                      {@const isSubSelected = selectedSource === 'Agent' && selectedSubItem === subKey}
                      <button
                        class="group flex w-full items-center justify-between rounded-md px-2 py-1 text-sm transition-colors {isSubSelected
                          ? 'bg-primary/10 font-medium text-primary'
                          : 'text-muted-foreground hover:bg-muted/30 hover:text-foreground'}"
                        onclick={() => selectSubItem('Agent', subKey)}
                        title={subInfo.label}
                      >
                        <div class="flex items-center gap-2 overflow-hidden">
                          <SubIcon
                            class="h-3.5 w-3.5 shrink-0 {isSubSelected ? 'text-primary' : 'text-muted-foreground'}"
                          />
                          <span class="truncate text-xs">{truncatePath(subInfo.label, 25)}</span>
                        </div>
                        <span
                          class="ml-2 shrink-0 text-xs {isSubSelected
                            ? 'font-medium text-primary'
                            : 'text-muted-foreground'}">{formatCount(subInfo.count)}</span
                        >
                      </button>
                    {/each}
                  </div>
                {/if}
              </div>

              <!-- Local -->
              <div>
                <div class="flex items-center">
                  <button
                    class="flex h-6 w-6 items-center justify-center rounded text-muted-foreground hover:bg-muted/50 hover:text-foreground {sourceTree
                      .Local.count === 0
                      ? 'invisible'
                      : ''}"
                    onclick={() => toggleSourceExpand('Local')}
                  >
                    {#if expandedSources.has('Local')}
                      <ChevronDown class="h-3.5 w-3.5" />
                    {:else}
                      <ChevronRight class="h-3.5 w-3.5" />
                    {/if}
                  </button>
                  <button
                    class="group flex flex-1 items-center justify-between rounded-md px-2 py-1.5 text-sm transition-colors {selectedSource ===
                      'Local' && !selectedSubItem
                      ? 'bg-primary/10 font-medium text-primary'
                      : 'text-foreground hover:bg-muted/50'}"
                    onclick={() => selectSource('Local')}
                  >
                    <div class="flex items-center gap-2">
                      <HardDrive
                        class="h-4 w-4 {selectedSource === 'Local'
                          ? 'text-primary'
                          : 'text-muted-foreground group-hover:text-foreground'}"
                      />
                      <span>Local</span>
                    </div>
                    <Badge
                      variant={selectedSource === 'Local' && !selectedSubItem ? 'default' : 'secondary'}
                      class="rounded-full px-2 py-0.5 text-xs font-medium">{formatCount(sourceTree.Local.count)}</Badge
                    >
                  </button>
                </div>
                {#if expandedSources.has('Local') && sourceTree.Local.children.size > 0}
                  <div class="mt-1 ml-6 space-y-0.5 border-l border-border pl-2">
                    {#each Array.from(sourceTree.Local.children.entries()).sort((a, b) => b[1].count - a[1].count) as [subKey, subInfo]}
                      {@const SubIcon = getSubTypeIcon(subInfo.subType)}
                      {@const isSubSelected = selectedSource === 'Local' && selectedSubItem === subKey}
                      <button
                        class="group flex w-full items-center justify-between rounded-md px-2 py-1 text-sm transition-colors {isSubSelected
                          ? 'bg-primary/10 font-medium text-primary'
                          : 'text-muted-foreground hover:bg-muted/30 hover:text-foreground'}"
                        onclick={() => selectSubItem('Local', subKey)}
                        title={subInfo.label}
                      >
                        <div class="flex items-center gap-2 overflow-hidden">
                          <SubIcon
                            class="h-3.5 w-3.5 shrink-0 {isSubSelected ? 'text-primary' : 'text-muted-foreground'}"
                          />
                          <span class="truncate text-xs">{truncatePath(subInfo.label, 25)}</span>
                        </div>
                        <span
                          class="ml-2 shrink-0 text-xs {isSubSelected
                            ? 'font-medium text-primary'
                            : 'text-muted-foreground'}">{formatCount(subInfo.count)}</span
                        >
                      </button>
                    {/each}
                  </div>
                {/if}
              </div>
            </div>

            <!-- 清除筛选按钮 -->
            {#if selectedSource || selectedSubItem}
              <button
                class="mt-4 flex w-full items-center justify-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
                onclick={clearFilters}
              >
                <X class="h-3 w-3" />
                <span>Clear filters</span>
              </button>
            {/if}
          </div>
        </div>
      </aside>

      <!-- 右侧结果区域 -->
      <main class="min-w-0">
        <!-- 结果统计头 -->
        <div class="mb-4 flex items-center justify-between">
          <h2 class="text-lg font-semibold">
            {#if filteredCount > 0}
              {filteredCount} results
              {#if selectedSource || selectedSubItem}
                <span class="ml-2 text-sm font-normal text-muted-foreground">
                  (filtered from {totalCount})
                </span>
              {/if}
            {:else if !searchStore.loading && q}
              0 results
            {:else}
              Search results
            {/if}
          </h2>
          <!-- 排序下拉框 (Mock) -->
          <div class="text-sm text-muted-foreground">
            Sort: <span class="font-medium text-foreground">Best match</span>
          </div>
        </div>

        <div class="space-y-4">
          {#each filteredResults as item, i (item.path + '-' + i)}
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
          {:else if !searchStore.loading && filteredResults.length === 0 && q && !searchStore.hasMore && !searchStore.error}
            <SearchEmptyState type="no-results" />
          {:else if !searchStore.loading && !searchStore.error && filteredResults.length === 0 && !q}
            <SearchEmptyState type="initial" />
          {/if}
        </div>

        <!-- 分页控制按钮 -->
        <div class="mt-8 flex items-center justify-center">
          {#if searchStore.hasMore}
            <Button
              variant="outline"
              class="w-full max-w-xs shadow-sm"
              onclick={() => searchStore.loadMore()}
              disabled={searchStore.loading}
            >
              {#if searchStore.loading}
                <Loader2 class="mr-2 h-4 w-4 animate-spin" />
                {searchStore.results.length === 0 ? 'Searching...' : 'Loading more...'}
              {:else}
                Load more
              {/if}
            </Button>
          {:else if filteredResults.length > 0}
            <p class="text-sm text-muted-foreground">All results loaded</p>
          {/if}
        </div>
      </main>
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
