<script lang="ts">
  /**
   * 搜索页面（重构版）
   * 仿 GitHub 代码搜索布局
   */
  import { SvelteSet } from 'svelte/reactivity';
  import { useSearch } from '$lib/modules/logseek';
  import LogSeekLogo from '$lib/components/LogSeekLogo.svelte';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import SearchResultCard from './SearchResultCard.svelte';
  import SearchEmptyState from './SearchEmptyState.svelte';

  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';
  import {
    Search,
    LoaderCircle,
    X,
    Cloud,
    Server,
    HardDrive,
    Archive,
    Folder,
    Database,
    CircleCheckBig
  } from 'lucide-svelte';
  import { parseFileUrl } from '$lib/modules/logseek/utils/fileUrl';
  import { Badge } from '$lib/components/ui/badge';
  import { Separator } from '$lib/components/ui/separator';

  // 使用 composable 管理搜索状态
  const searchStore = useSearch();

  // 本地输入框状态
  let q = $state('');

  // 当前选中的筛选器
  // 当前选中的路径（从根节点开始的路径数组）
  // Level 0: Endpoint Type (S3, Agent, Local)
  // Level 1: Endpoint ID (Profile:Bucket, AgentID, Hostname)
  // Level 2+: Path segments (Dir, Archive, Inner Path)
  let selectedPath = $state<string[]>([]);

  // 树节点结构
  interface TreeNode {
    key: string;
    label: string;
    count: number;
    fullPath: string[];
    children: Map<string, TreeNode>;
    type: 'endpoint_type' | 'endpoint_id' | 'dir' | 'archive' | 'file';
    icon?: any;
    url: string; // Added URL field
  }

  // 构建资源树
  let sourceTree = $derived.by(() => {
    const tree: Record<string, TreeNode> = {
      S3: {
        key: 'S3',
        label: 'S3 云存储',
        count: 0,
        fullPath: ['S3'],
        children: new Map(),
        type: 'endpoint_type',
        icon: Cloud,
        url: 'ls://s3'
      },
      Agent: {
        key: 'Agent',
        label: '远程代理',
        count: 0,
        fullPath: ['Agent'],
        children: new Map(),
        type: 'endpoint_type',
        icon: Server,
        url: 'ls://agent'
      },
      Local: {
        key: 'Local',
        label: '本地文件',
        count: 0,
        fullPath: ['Local'],
        children: new Map(),
        type: 'endpoint_type',
        icon: HardDrive,
        url: 'ls://local'
      }
    };

    for (const res of searchStore.results) {
      const parsed = parseFileUrl(res.path);
      if (!parsed) continue;

      // 1. Endpoint Type
      let typeKey = 'Local';
      if (parsed.endpointType === 's3') typeKey = 'S3';
      else if (parsed.endpointType === 'agent') typeKey = 'Agent';

      const typeNode = tree[typeKey];
      typeNode.count++;

      // 2. Endpoint ID
      let idNode = typeNode.children.get(parsed.endpointId);
      if (!idNode) {
        idNode = {
          key: parsed.endpointId,
          label: parsed.endpointId,
          count: 0,
          fullPath: [typeKey, parsed.endpointId],
          children: new Map(),
          type: 'endpoint_id',
          icon: typeKey === 'S3' ? Database : typeKey === 'Agent' ? Server : HardDrive,
          url: `ls://${parsed.endpointType}/${parsed.endpointId}`
        };
        typeNode.children.set(parsed.endpointId, idNode);
      }
      idNode.count++;

      // 3. Path Segments
      const pathSegments = parsed.path.split('/').filter((p: string) => p);
      let currentParent = idNode;
      let currentPathStr = '';

      for (let i = 0; i < pathSegments.length; i++) {
        const segment = pathSegments[i];
        const isLastSegment = i === pathSegments.length - 1;
        const isArchiveFile = parsed.targetType === 'archive' && isLastSegment;

        currentPathStr += (currentPathStr ? '/' : '') + segment;

        if (isLastSegment && !isArchiveFile) {
          continue;
        }

        let child = currentParent.children.get(segment);
        if (!child) {
          const nodeTargetType = isArchiveFile ? 'archive' : 'dir';
          const nodeUrl = `ls://${parsed.endpointType}/${parsed.endpointId}/${nodeTargetType}/${currentPathStr}`;

          child = {
            key: segment,
            label: segment,
            count: 0,
            fullPath: [...currentParent.fullPath, segment],
            children: new Map(),
            type: isArchiveFile ? 'archive' : 'dir',
            icon: isArchiveFile ? Archive : Folder,
            url: nodeUrl
          };
          currentParent.children.set(segment, child);
        }
        child.count++;
        currentParent = child;
      }

      // 4. Archive Entry Path
      if (parsed.targetType === 'archive' && parsed.entryPath) {
        const entrySegments = parsed.entryPath.split('/').filter((p: string) => p);
        let currentEntryPathStr = '';

        for (let i = 0; i < entrySegments.length - 1; i++) {
          const segment = entrySegments[i];
          currentEntryPathStr += (currentEntryPathStr ? '/' : '') + segment;

          let child = currentParent.children.get(segment);
          if (!child) {
            const nodeUrl = `ls://${parsed.endpointType}/${parsed.endpointId}/archive/${parsed.path}?entry=${currentEntryPathStr}`;

            child = {
              key: segment,
              label: segment,
              count: 0,
              fullPath: [...currentParent.fullPath, segment],
              children: new Map(),
              type: 'dir',
              icon: Folder,
              url: nodeUrl
            };
            currentParent.children.set(segment, child);
          }
          child.count++;
          currentParent = child;
        }
      }
    }

    return tree;
  });

  // 压缩树节点（Skip Single Child）
  // 规则：
  // 1. Endpoint Type (Level 0) 不压缩，始终显示
  // 2. Endpoint ID (Level 1) 如果只有一个，则跳过（直接显示下一级）
  // 3. Dir/Archive (Level 2+) 如果只有一个子节点且数量相同，合并显示
  function getRenderTree(root: Record<string, TreeNode>): TreeNode[] {
    // 直接返回根节点，压缩逻辑移至渲染层 (renderStackedLevel)
    return Object.values(root);
  }

  let renderTree = $derived(getRenderTree(sourceTree));

  // 筛选逻辑
  let filteredResults = $derived.by(() => {
    if (selectedPath.length === 0) return searchStore.results;

    return searchStore.results.filter((res) => {
      const parsed = parseFileUrl(res.path);
      if (!parsed) return false;

      // 1. Check Endpoint Type
      let typeKey = 'Local';
      if (parsed.endpointType === 's3') typeKey = 'S3';
      else if (parsed.endpointType === 'agent') typeKey = 'Agent';

      if (selectedPath.length > 0 && selectedPath[0] !== typeKey) return false;
      if (selectedPath.length === 1) return true;

      // 2. Check Endpoint ID
      // 注意：如果 Endpoint ID 被跳过了（因为只有一个），selectedPath 中可能不包含它？
      // 不，selectedPath 存储的是 UI 上点击的路径。
      // 如果 UI 上跳过了 Endpoint ID，那么 selectedPath[1] 直接就是 Dir/Archive。
      // 我们需要根据 sourceTree 的结构来还原匹配逻辑。
      // 更好的方法是：selectedPath 存储的是 TreeNode 的 fullPath。
      // 无论 UI 怎么压缩，fullPath 都是完整的真实路径。

      // 让我们重新定义 selectedPath：它存储的是用户点击的那个节点的 fullPath。
      // 当用户点击一个“压缩节点”时，我们使用该节点的 fullPath。
      // 压缩节点的 fullPath 指向的是最深层的那个节点。

      // 验证 fullPath 匹配
      // fullPath: ['S3', 'prod:bucket', 'dir1', 'dir2']
      // res parts: type, id, path segments...

      const resPathParts = [typeKey, parsed.endpointId];
      const pathSegments = parsed.path.split('/').filter((p: string) => p);

      resPathParts.push(...pathSegments);

      if (parsed.targetType === 'archive' && parsed.entryPath) {
        resPathParts.push(...parsed.entryPath.split('/').filter((p: string) => p));
      }

      // 检查 resPathParts 是否以 selectedPath 开头
      if (resPathParts.length < selectedPath.length) return false;

      for (let i = 0; i < selectedPath.length; i++) {
        // 归档文件特殊处理：如果是 archive 类型，不需要完全匹配，只要前缀匹配即可
        // 比如 selectedPath 到了 archive.tar.gz，那么 archive.tar.gz/inner 也是匹配的
        if (resPathParts[i] !== selectedPath[i]) return false;
      }

      return true;
    });
  });

  let filteredCount = $derived(filteredResults.length);

  // ============ 辅助函数 ============

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

  // 清除所有筛选状态
  function clearFilters() {
    selectedPath = [];
  }

  // 开始新搜索（清除筛选状态并执行搜索）
  function startSearch(query: string) {
    clearFilters();
    searchStore.search(query);
  }

  // 从地址栏读取 ?q=，并在客户端启动搜索
  let searchInit = $state(false);
  $effect(() => {
    if (searchInit) return;
    searchInit = true;
    const params = new URL(window.location.href).searchParams;
    const initial = (params.get('q') || '').trim();
    q = initial;
    if (initial) startSearch(initial);
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
    startSearch(next);
  }

  // ============ 侧边栏交互逻辑 ============
  let totalCount = $derived(searchStore.results.length);

  // 交互逻辑
  function toggleSelection(node: TreeNode) {
    // 检查是否已经选中（是当前选中路径的前缀）
    const isSelected = isPathSelected(node.fullPath);
    const isExactMatch =
      selectedPath.length === node.fullPath.length && selectedPath.every((p, i) => p === node.fullPath[i]);

    if (isExactMatch) {
      // 如果完全匹配，说明是取消选中 -> 选中父节点
      // 如果是根节点，清空
      if (node.fullPath.length <= 1) {
        selectedPath = [];
      } else {
        // 这里的逻辑有点复杂，因为 UI 树和 逻辑树 不一致。
        // 简单处理：点击已选中的 -> 取消选中（回到空）或者回到上一级？
        // 通常侧边栏行为：点击高亮，再次点击取消。
        selectedPath = [];
      }
    } else {
      // 选中新节点
      selectedPath = node.fullPath;
    }
  }

  function isPathSelected(path: string[]) {
    if (selectedPath.length < path.length) return false;
    for (let i = 0; i < path.length; i++) {
      if (selectedPath[i] !== path[i]) return false;
    }
    return true;
  }

  // 侧边栏宽度调整
  let sidebarWidth = $state(280);
  let isResizing = $state(false);

  function startResizing(e: MouseEvent) {
    isResizing = true;
    e.preventDefault();
    document.body.style.cursor = 'col-resize';
  }

  function handleMouseMove(e: MouseEvent) {
    if (!isResizing) return;
    const newWidth = Math.max(200, Math.min(e.clientX - 24, 600));
    sidebarWidth = newWidth;
  }

  function stopResizing() {
    if (isResizing) {
      isResizing = false;
      document.body.style.cursor = '';
    }
  }

  // 动态计算路径截断长度
  let pathTruncateLength = $derived(Math.max(10, Math.floor((sidebarWidth - 110) / 7.5)));
</script>

<svelte:window onmousemove={handleMouseMove} onmouseup={stopResizing} />

<div class="bg-background text-foreground min-h-screen">
  <!-- 顶部导航栏 -->
  <header
    class="border-border bg-background/95 supports-backdrop-filter:bg-background/60 sticky top-0 z-50 w-full border-b backdrop-blur"
  >
    <div class="flex h-16 w-full items-center gap-4 px-6">
      <!-- Logo -->
      <a href="/" class="flex items-center gap-2 transition-opacity hover:opacity-80">
        <LogSeekLogo size="small" />
      </a>

      <!-- 搜索框 -->
      <form class="ml-4 flex-1" onsubmit={handleSubmit}>
        <div class="group relative flex items-center">
          <div class="text-muted-foreground pointer-events-none absolute left-3 z-10">
            <Search class="h-4 w-4" />
          </div>
          <Input
            id="search"
            class="border-input bg-muted/50 text-foreground hover:bg-muted focus-visible:bg-background focus-visible:ring-primary h-9 rounded-md pl-9 pr-9 text-sm shadow-none transition-all focus-visible:ring-1"
            disabled={searchStore.loading}
            bind:value={q}
            placeholder="搜索..."
            autocomplete="off"
          />
          {#if searchStore.loading}
            <div class="absolute right-3 z-10">
              <LoaderCircle class="text-primary h-3.5 w-3.5 animate-spin" />
            </div>
          {:else if q}
            <Button
              variant="ghost"
              size="icon"
              class="text-muted-foreground hover:text-foreground absolute right-1 z-10 h-7 w-7"
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
        <Settings />
        <ThemeToggle />
      </div>
    </div>
  </header>

  <div class="min-h-[calc(100vh-4rem)] w-full px-6 py-6">
    <div
      class="grid grid-cols-1 gap-8 md:grid-cols-[var(--sidebar-width)_1fr] md:items-start"
      style="--sidebar-width: {sidebarWidth}px"
    >
      <!-- 左侧边栏：统计与筛选 -->
      <aside class="group/sidebar border-border relative hidden h-full border-r pr-6 md:block">
        <!-- 拖动把手 -->
        <button
          type="button"
          class="hover:bg-primary/20 absolute -right-1 top-0 z-10 h-full w-2 cursor-col-resize border-0 bg-transparent p-0 transition-colors"
          onmousedown={startResizing}
          aria-label="调整侧边栏宽度"
        ></button>
        <div class="sticky top-24 -mr-2 max-h-[calc(100vh-8rem)] space-y-6 overflow-y-auto pr-2">
          <div>
            <h3 class="text-foreground mb-3 text-sm font-semibold">筛选</h3>
            <Separator class="mb-4" />

            {#snippet renderStackedLevel(nodes: TreeNode[], depth: number)}
              <!--
                如果当前层级只有一个节点，且不是根层级（depth > 0），
                则跳过该层级，直接渲染子节点。
                这满足了"如果只剩一个筛选项则跳过"的需求。
              -->
              {#if nodes.length === 0}
                <!-- 空节点，不渲染任何内容 -->
              {:else if depth > 0 && nodes.length === 1 && nodes[0].children.size > 0}
                {@render renderStackedLevel(
                  Array.from(nodes[0].children.values()).sort((a, b) => b.count - a.count),
                  depth + 1
                )}
              {:else}
                <div class="space-y-0.5">
                  {#each nodes as node (node.key)}
                    {@const isPathActive = isPathSelected(node.fullPath)}
                    {@const isExactActive =
                      selectedPath.length === node.fullPath.length &&
                      selectedPath.every((p, i) => p === node.fullPath[i])}

                    <!--
                      Level 0 (Endpoint Type) 始终显示。
                      其他层级只有 count > 0 才显示。
                    -->
                    {#if depth === 0 || node.count > 0}
                      <button
                        class="group flex w-full items-center justify-between rounded-md px-2 py-1.5 text-sm transition-colors {isPathActive
                          ? 'bg-primary/10 text-primary font-medium'
                          : 'text-foreground hover:bg-muted/50'}"
                        onclick={() => toggleSelection(node)}
                        title={node.url}
                      >
                        <div class="flex items-center gap-2 overflow-hidden">
                          {#if node.icon}
                            <node.icon
                              class="h-4 w-4 shrink-0 {isPathActive
                                ? 'text-primary'
                                : 'text-muted-foreground group-hover:text-foreground'}"
                            />
                          {/if}
                          <span class="truncate">{truncatePath(node.label, pathTruncateLength)}</span>
                        </div>
                        <Badge
                          variant={isExactActive ? 'default' : 'secondary'}
                          class="shrink-0 rounded-full px-2 py-0.5 text-xs font-medium">{formatCount(node.count)}</Badge
                        >
                      </button>
                    {/if}
                  {/each}
                </div>

                <!-- 查找当前层级中被选中的节点（作为路径一部分的节点），渲染其子节点 -->
                {@const activeNode = nodes.find((n) => isPathSelected(n.fullPath))}
                {#if activeNode && activeNode.children.size > 0}
                  <Separator class="my-4" />
                  {@render renderStackedLevel(
                    Array.from(activeNode.children.values()).sort((a, b) => b.count - a.count),
                    depth + 1
                  )}
                {/if}
              {/if}
            {/snippet}

            {@render renderStackedLevel(renderTree, 0)}
          </div>
        </div>
      </aside>

      <!-- 右侧结果区域 -->
      <main class="min-w-0">
        <!-- 结果统计头 -->
        <div class="mb-4 flex items-center justify-between">
          <h2 class="text-lg font-semibold">
            {#if filteredCount > 0}
              {filteredCount} 个结果
              {#if selectedPath.length > 0}
                <span class="text-muted-foreground ml-2 text-sm font-normal"> (已筛选) </span>
              {/if}
            {:else if !searchStore.loading && q}
              0 个结果
            {:else}
              搜索结果
            {/if}
          </h2>
        </div>

        <div class="space-y-4">
          {#each filteredResults as item, i (item.path + '-' + i)}
            {#if item && item.path && item.chunks}
              <SearchResultCard {item} index={i} sid={searchStore.sid} />
            {:else}
              <!-- 兼容其他对象：兜底显示 -->
              <div class="bg-card text-card-foreground rounded border p-3">
                <pre class="whitespace-pre-wrap break-all text-sm leading-relaxed">{JSON.stringify(item, null, 2)}</pre>
              </div>
            {/if}
          {/each}

          <!-- 空状态和错误状态 -->
          {#if searchStore.error && !searchStore.loading}
            <SearchEmptyState
              type="error"
              errorMessage={searchStore.error}
              onRetry={() => {
                if (q) startSearch(q);
              }}
            />
          {:else if !searchStore.loading && filteredResults.length === 0 && q && !searchStore.hasMore && !searchStore.error}
            <SearchEmptyState type="no-results" />
          {:else if !searchStore.loading && !searchStore.error && filteredResults.length === 0 && !q}
            <SearchEmptyState type="initial" />
          {/if}
        </div>

        <!-- 分页控制按钮 -->
        {#if q}
          <div class="mt-8 flex items-center justify-center">
            {#if searchStore.hasMore}
              <Button
                class="w-full max-w-xs shadow-sm"
                onclick={() => searchStore.loadMore()}
                disabled={searchStore.loading}
              >
                {#if searchStore.loading}
                  <LoaderCircle class="mr-2 h-4 w-4 animate-spin" />
                  {searchStore.results.length === 0 ? '搜索中...' : '加载更多...'}
                {:else}
                  加载更多
                {/if}
              </Button>
            {:else if filteredResults.length > 0}
              <div class="flex items-center gap-2 text-sm text-green-600 dark:text-green-400">
                <CircleCheckBig class="h-4 w-4" />
                <span>已加载全部结果</span>
              </div>
            {/if}
          </div>
        {/if}
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
