<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Separator } from '$lib/components/ui/separator';
  import { type ResourceItem, type Odfi, listResources } from '$lib/modules/explorer';
  import {
    Folder,
    File,
    ArrowLeft,
    RefreshCw,
    Home,
    Download,
    Server,
    HardDrive,
    Cloud,
    ChevronRight,
    ChevronDown,
    Monitor,
    LayoutList,
    LayoutGrid,
    Eye,
    EyeOff,
    Link,
    ExternalLink,
    Copy,
    Info,
    Music,
    Film,
    Image as ImageIcon
  } from 'lucide-svelte';
  import * as ContextMenu from '$lib/components/ui/context-menu';
  import errorIcon from '$lib/assets/error.svg';
  import errorDarkIcon from '$lib/assets/error-dark.svg';

  // State
  let currentOdfiStr = $state('');
  let items: ResourceItem[] = $state([]);
  let loading = $state(false);
  let error: string | null = $state(null);
  let urlOdfi = page.url.searchParams.get('odfi');
  let viewMode = $state<'table' | 'grid'>('grid');
  let showHidden = $state(false);

  // Derived filtered items
  let displayedItems = $derived.by(() => {
    if (showHidden) return items;
    return items.filter((item) => !item.name.startsWith('.'));
  });

  // Sidebar State
  let expandedSections = $state({
    s3: false,
    agent: false
  });
  let sidebarData = $state({
    s3: [] as ResourceItem[],
    agent: [] as ResourceItem[]
  });
  let sidebarLoading = $state({
    s3: false,
    agent: false
  });

  // Sidebar Resizing
  let sidebarWidth = $state(256);
  let isResizing = $state(false);

  function startResizing(e: MouseEvent) {
    isResizing = true;
    e.preventDefault();
    document.body.style.cursor = 'col-resize';
  }

  function handleMouseMove(e: MouseEvent) {
    if (!isResizing) return;
    const newWidth = Math.max(200, Math.min(e.clientX, 600));
    sidebarWidth = newWidth;
  }

  function stopResizing() {
    if (isResizing) {
      isResizing = false;
      document.body.style.cursor = '';
    }
  }

  // Derived active state from ODFI
  let activeType = $derived.by(() => {
    if (currentOdfiStr.startsWith('odfi://local')) return 'local';
    if (currentOdfiStr.startsWith('odfi://s3')) return 's3';
    if (currentOdfiStr.includes('@agent') || currentOdfiStr.startsWith('odfi://agent')) return 'agent';
    return null;
  });

  // Identify current agent name or S3 profile name
  let activeId = $derived.by(() => {
    if (activeType === 'agent') {
      // Extract from odfi://id@agent...
      const match = currentOdfiStr.match(/odfi:\/\/([^@]+)@agent/);
      return match ? match[1] : null;
    }
    if (activeType === 's3') {
      // Extract profile from odfi://profile@s3...
      const match = currentOdfiStr.match(/odfi:\/\/([^@]+)@s3/);
      return match ? match[1] : null;
    }
    return null;
  });

  // Initial load
  onMount(() => {
    if (urlOdfi) {
      currentOdfiStr = urlOdfi;
      loadResources(urlOdfi);
    } else {
      // Default to local root
      currentOdfiStr = 'odfi://local/';
      loadResources(currentOdfiStr);
    }

    // Preload sidebar data
    loadSidebarData('agent');
    loadSidebarData('s3');
  });

  async function loadSidebarData(section: 's3' | 'agent') {
    sidebarLoading[section] = true;
    try {
      const rootOdfi = section === 's3' ? 'odfi://s3/' : 'odfi://agent/';
      sidebarData[section] = await listResources(rootOdfi);
    } catch (e) {
      console.error(`Failed to load sidebar ${section}`, e);
    } finally {
      sidebarLoading[section] = false;
    }
  }

  async function loadResources(odfi: string) {
    loading = true;
    error = null;
    try {
      items = await listResources(odfi);
    } catch (e: any) {
      error = e.message;
      items = [];
    } finally {
      loading = false;
    }
  }

  async function toggleSection(section: 's3' | 'agent') {
    expandedSections[section] = !expandedSections[section];

    // Load if expanding and empty
    if (expandedSections[section] && sidebarData[section].length === 0) {
      sidebarLoading[section] = true;
      try {
        const rootOdfi = section === 's3' ? 'odfi://s3/' : 'odfi://agent/';
        sidebarData[section] = await listResources(rootOdfi);
      } catch (e) {
        console.error(`Failed to load sidebar ${section}`, e);
      } finally {
        sidebarLoading[section] = false;
      }
    }
  }

  function handleNavigate(newOdfi: string) {
    currentOdfiStr = newOdfi;
    // Update URL without reload
    const url = new URL(window.location.href);
    url.searchParams.set('odfi', newOdfi);
    // Remove title query if present to clean up
    goto(url.toString(), { keepFocus: true, noScroll: true });
    loadResources(newOdfi);
  }

  function handleRowClick(item: ResourceItem) {
    if (item.type === 'dir' || item.type === 'linkdir') {
      handleNavigate(item.path);
    } else {
      // Default action for files could be preview or download
      console.log('File clicked:', item.path);
    }
  }

  async function copyToClipboard(text: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch (err) {
      console.error('Failed to copy: ', err);
    }
  }

  function handleDownload(item: ResourceItem) {
    // TODO: Implement download API
    console.log('Downloading:', item.path);
  }

  function goUp() {
    try {
      let urlStr = currentOdfiStr;
      const url = new URL(urlStr);

      // Check if we are inside an archive
      const entry = url.searchParams.get('entry');
      if (entry) {
        const entryParts = entry.split('/').filter((p) => p);
        if (entryParts.length > 1) {
          entryParts.pop();
          url.searchParams.set('entry', entryParts.join('/'));
        } else {
          url.searchParams.delete('entry');
        }
        handleNavigate(url.toString());
        return;
      }

      // Normal path navigation
      const pathParts = url.pathname.split('/').filter((p) => p);
      if (pathParts.length > 0) {
        pathParts.pop();
        url.pathname = '/' + pathParts.join('/');
        handleNavigate(url.toString());
      }
    } catch (e) {
      console.error('Failed to parse ODFI for parent navigation', e);
      if (currentOdfiStr.includes('/')) {
        const parts = currentOdfiStr.split('/');
        parts.pop();
        handleNavigate(parts.join('/') || 'odfi://local/');
      }
    }
  }

  function formatSize(bytes: number | null | undefined): string {
    if (bytes === null || bytes === undefined) return '';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let i = 0;
    while (bytes >= 1024 && i < units.length - 1) {
      bytes /= 1024;
      i++;
    }
    return `${bytes.toFixed(1)} ${units[i]}`;
  }

  function truncateMiddle(str: string, maxVisualWidth: number = 40, tailChars: number = 7): string {
    let visualWidth = 0;
    for (let i = 0; i < str.length; i++) {
      visualWidth += str.charCodeAt(i) > 255 ? 2 : 1;
    }

    if (visualWidth <= maxVisualWidth) return str;

    const ellipsis = '...';
    const tailStr = str.slice(-tailChars);

    let tailWidth = 0;
    for (let i = 0; i < tailStr.length; i++) {
      tailWidth += tailStr.charCodeAt(i) > 255 ? 2 : 1;
    }

    const availableHeadWidth = maxVisualWidth - tailWidth - 3;
    if (availableHeadWidth <= 0) return '...' + tailStr;

    let headStr = '';
    let headWidth = 0;
    for (let i = 0; i < str.length - tailChars; i++) {
      const charWidth = str.charCodeAt(i) > 255 ? 2 : 1;
      if (headWidth + charWidth > availableHeadWidth) break;
      headStr += str[i];
      headWidth += charWidth;
    }

    return headStr + ellipsis + tailStr;
  }
</script>

<svelte:window onmousemove={handleMouseMove} onmouseup={stopResizing} />

<div class="flex h-[calc(100vh-4rem)] gap-8 overflow-hidden px-6 py-6">
  <!-- Sidebar -->
  <aside
    class="group/sidebar relative hidden h-full border-r border-border pr-6 md:block"
    style="width: {sidebarWidth}px"
  >
    <!-- 拖动把手 -->
    <button
      type="button"
      class="absolute top-0 -right-1 z-10 h-full w-2 cursor-col-resize border-0 bg-transparent p-0 transition-colors hover:bg-primary/20"
      onmousedown={startResizing}
      aria-label="调整侧边栏宽度"
    ></button>

    <div class="sticky top-0 max-h-full space-y-6 overflow-y-auto pr-2">
      <div>
        <h3 class="mb-3 text-sm font-semibold text-foreground">Explorer</h3>
        <Separator class="mb-4" />

        {#snippet renderLevel(items: any[], depth: number)}
          <div class="space-y-0.5">
            {#each items as item}
              {@const isActive =
                (depth === 0 && activeType === item.key) ||
                (depth === 1 && (activeId === item.name || currentOdfiStr.startsWith(item.path)))}
              <button
                class="group flex w-full items-center rounded-md px-2 py-1.5 text-sm transition-colors {isActive
                  ? 'bg-primary/10 font-medium text-primary'
                  : 'text-foreground hover:bg-muted/50'}"
                onclick={() => handleNavigate(item.path)}
              >
                {#if item.icon}
                  <item.icon
                    class="mr-2 h-4 w-4 {isActive
                      ? 'text-primary'
                      : 'text-muted-foreground group-hover:text-foreground'}"
                  />
                {:else}
                  <div
                    class="mr-2 h-1.5 w-1.5 flex-shrink-0 rounded-full {item.colorClass || 'bg-muted-foreground'}"
                  ></div>
                {/if}
                <span class="truncate">{item.label || item.name}</span>
              </button>
            {/each}
          </div>

          <!-- Drill down -->
          {#if depth === 0}
            {@const activeRoot = items.find((i) => i.key === activeType)}
            {#if activeRoot && activeRoot.key !== 'local'}
              {@const children = sidebarData[activeRoot.key as 's3' | 'agent']}
              {#if children && children.length > 0}
                <Separator class="my-3" />
                {@render renderLevel(
                  children.map((c) => ({
                    ...c,
                    colorClass: activeRoot.key === 's3' ? 'bg-blue-500' : 'bg-green-500'
                  })),
                  1
                )}
              {:else if sidebarLoading[activeRoot.key as 's3' | 'agent']}
                <div class="mt-4 animate-pulse px-2 py-1 text-xs text-muted-foreground">Loading...</div>
              {:else}
                <div class="mt-4 px-2 py-1 text-xs text-muted-foreground">
                  {#if activeRoot.key === 's3'}No profiles found{:else}No online agents{/if}
                </div>
              {/if}
            {/if}
          {/if}
        {/snippet}

        {@render renderLevel(
          [
            { key: 'local', label: 'Local Machine', path: 'odfi://local/', icon: Monitor },
            { key: 'agent', label: 'Remote Agents', path: 'odfi://agent/', icon: Server },
            { key: 's3', label: 'S3 Storage', path: 'odfi://s3/', icon: Cloud }
          ],
          0
        )}
      </div>
    </div>
  </aside>

  {#snippet macOSFolder(className = 'h-16 w-16', hasFiles = true, icon: any = null)}
    <div class="relative {className} flex items-center justify-center">
      <svg viewBox="0 0 100 88" class="h-full w-full drop-shadow-sm">
        <defs>
          <!-- Back part gradient (darker/deeper) -->
          <linearGradient id="folderBackGrad" x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" style="stop-color:#5ea6d8;stop-opacity:1" />
            <stop offset="100%" style="stop-color:#3896d8;stop-opacity:1" />
          </linearGradient>
          <!-- Front part gradient (brighter/vibrant) -->
          <linearGradient id="folderFrontGrad" x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" style="stop-color:#8bd5ff;stop-opacity:1" />
            <stop offset="100%" style="stop-color:#38b6ff;stop-opacity:1" />
          </linearGradient>
          <!-- Filter for the front cover's shadow on the body -->
          <filter id="frontShadow" x="-20%" y="-20%" width="140%" height="140%">
            <feGaussianBlur in="SourceAlpha" stdDeviation="1.2" />
            <feOffset dx="0" dy="-1" result="offsetblur" />
            <feFlood flood-color="black" flood-opacity="0.3" />
            <feComposite in2="offsetblur" operator="in" />
            <feMerge>
              <feMergeNode />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
        </defs>
        <!-- Folder Body with tab - Shoulder at y=17 -->
        <path
          d="M10,12 L35,12 L42,17 L90,17 C94,17 95,18 95,22 L95,80 C95,84 94,85 90,85 L10,85 C6,85 5,84 5,80 L5,17 C5,13 6,12 10,12 Z"
          fill="url(#folderBackGrad)"
          stroke="#2d9cdb"
          stroke-width="0.3"
        />
        <!-- White Bar (Paper) inside - Peek height of 4 units (from y=21 to 25) -->
        {#if hasFiles}
          <path
            d="M12,21 L88,21 C90,21 91,21.5 91,22 L91,26 L9,26 L9,22 C9,21.5 10,21 12,21 Z"
            fill="white"
            class="dark:fill-gray-100"
          />
        {/if}
        <!-- Folder Front - Rounded top corners (radius 6) -->
        <path
          d="M5,31 Q5,25 11,25 L89,25 Q95,25 95,31 L95,80 C95,84 94,85 90,85 L10,85 C6,85 5,84 5,80 L5,31 Z"
          fill="url(#folderFrontGrad)"
          filter="url(#frontShadow)"
        />
        <!-- Top Bevel Highlight -->
        <path
          d="M5,31 Q5,25 11,25 L89,25 Q95,25 95,31"
          fill="none"
          stroke="white"
          stroke-opacity="0.4"
          stroke-width="0.5"
        />
        <!-- Bottom/Side Stroke for Front -->
        <path
          d="M95,31 L95,80 C95,84 94,85 90,85 L10,85 C6,85 5,84 5,80 L5,31"
          fill="none"
          stroke="#2d8cdb"
          stroke-width="0.5"
        />
      </svg>
      {#if icon}
        {@const IconComp = icon}
        <div class="absolute top-[62%] right-0 left-0 flex -translate-y-1/2 items-center justify-center">
          <!-- Subtle bottom highlight -->
          <div
            class="absolute inset-0 flex translate-y-[0.5px] items-center justify-center opacity-20 mix-blend-overlay"
          >
            <IconComp class={className.includes('h-5') ? 'h-2.5 w-2.5' : 'h-6 w-6'} strokeWidth={2} color="white" />
          </div>
          <!-- Soft recessed inner shadow -->
          <div class="opacity-30 mix-blend-multiply dark:mix-blend-overlay">
            <IconComp class={className.includes('h-5') ? 'h-2.5 w-2.5' : 'h-6 w-6'} strokeWidth={2} />
          </div>
        </div>
      {/if}
    </div>
  {/snippet}

  {#snippet macOSFile(className = 'h-16 w-16', icon: any = null)}
    <div class="relative {className} flex items-center justify-center">
      <div
        class="relative h-full w-[80%] rounded-[4px] border border-gray-200 bg-white shadow-[0_1px_3px_rgba(0,0,0,0.1),0_1px_2px_rgba(0,0,0,0.06)] dark:border-gray-700 dark:bg-gray-100"
      >
        <div class="absolute top-0 right-0 h-[20%] w-[30%]">
          <svg viewBox="0 0 30 20" class="h-full w-full">
            <path
              d="M0,0 L0,20 L30,20 Z"
              fill="white"
              stroke="#e5e7eb"
              stroke-width="0.5"
              class="dark:fill-gray-100 dark:stroke-gray-300"
            />
          </svg>
        </div>
        {#if icon}
          {@const IconComp = icon}
          <div class="absolute inset-0 flex items-center justify-center opacity-20 dark:opacity-40">
            <IconComp class={className.includes('h-6') ? 'h-3 w-3' : 'h-10 w-10'} />
          </div>
        {/if}
      </div>
    </div>
  {/snippet}

  {#snippet itemContextMenu(item: ResourceItem)}
    {#if item && item.name}
      <ContextMenu.Content class="w-64 text-[13px]">
        <ContextMenu.Item class="h-8 py-0 focus:bg-[#007aff] focus:text-white" onclick={() => handleRowClick(item)}>
          <ExternalLink class="mr-3 h-3.5 w-3.5 opacity-50 dark:opacity-60" />
          <span>打开</span>
        </ContextMenu.Item>

        <ContextMenu.Separator class="bg-black/5 dark:bg-white/10" />

        <ContextMenu.Item
          class="h-8 py-0 focus:bg-[#007aff] focus:text-white"
          onclick={() => copyToClipboard(item.path)}
        >
          <Link class="mr-3 h-3.5 w-3.5 opacity-50 dark:opacity-60" />
          <span>复制 ODFI 路径</span>
        </ContextMenu.Item>

        <ContextMenu.Item
          class="h-8 py-0 focus:bg-[#007aff] focus:text-white"
          onclick={() => copyToClipboard(item.name)}
        >
          <Copy class="mr-3 h-3.5 w-3.5 opacity-50 dark:opacity-60" />
          <span>复制名称</span>
        </ContextMenu.Item>

        {#if item.type === 'file' || item.type === 'linkfile'}
          <ContextMenu.Item class="h-8 py-0 focus:bg-[#007aff] focus:text-white" onclick={() => handleDownload(item)}>
            <Download class="mr-3 h-3.5 w-3.5 opacity-50 dark:opacity-60" />
            <span>下载</span>
          </ContextMenu.Item>
        {/if}

        <ContextMenu.Separator class="bg-black/5 dark:bg-white/10" />

        <ContextMenu.Item class="h-8 py-0 focus:bg-[#007aff] focus:text-white">
          <Info class="mr-3 h-3.5 w-3.5 opacity-50 dark:opacity-60" />
          <span>属性</span>
        </ContextMenu.Item>

        <ContextMenu.Separator class="bg-black/5 dark:bg-white/10" />

        <!-- macOS Style Tags Row -->
        <div class="flex items-center justify-between px-3 py-2">
          <div class="flex w-full justify-between px-1">
            {#each [{ color: '#ff5f57', label: '红色' }, { color: '#febc2e', label: '橙色' }, { color: '#fedd34', label: '黄色' }, { color: '#28c840', label: '绿色' }, { color: '#4a90e2', label: '蓝色' }, { color: '#a370f7', label: '紫色' }, { color: '#8e8e93', label: '灰色' }] as tag}
              <button
                class="h-[18px] w-[18px] rounded-full ring-1 ring-black/5 transition-transform hover:scale-110 active:scale-90 dark:ring-white/10"
                style="background-color: {tag.color}"
                title={tag.label}
              ></button>
            {/each}
          </div>
        </div>
      </ContextMenu.Content>
    {/if}
  {/snippet}

  {#snippet containerContextMenu()}
    <ContextMenu.Content class="w-48 text-[13px]">
      <ContextMenu.Item
        class="h-8 py-0 focus:bg-[#007aff] focus:text-white"
        onclick={() => loadResources(currentOdfiStr)}
      >
        <RefreshCw class="mr-3 h-3.5 w-3.5 opacity-50 dark:opacity-60" />
        <span>刷新</span>
      </ContextMenu.Item>
    </ContextMenu.Content>
  {/snippet}

  <!-- Main Content -->
  <div class="flex flex-1 flex-col overflow-hidden bg-background">
    <!-- Toolbar -->
    <div class="flex items-center space-x-2 border-b border-border/40 p-4 dark:border-gray-700/50">
      <Button variant="ghost" size="icon" onclick={goUp} disabled={loading}>
        <ArrowLeft class="h-4 w-4" />
      </Button>
      <Button variant="ghost" size="icon" onclick={() => loadResources(currentOdfiStr)} disabled={loading}>
        <RefreshCw class="h-4 w-4 {loading ? 'animate-spin' : ''}" />
      </Button>
      <div
        class="flex flex-1 items-center rounded-md border border-border/40 bg-muted/50 px-3 py-1.5 focus-within:ring-1 focus-within:ring-ring dark:border-gray-700/50"
      >
        <input
          class="w-full flex-1 border-none bg-transparent font-mono text-sm outline-none"
          bind:value={currentOdfiStr}
          onkeydown={(e) => e.key === 'Enter' && handleNavigate(currentOdfiStr)}
        />
      </div>

      <div class="flex items-center rounded-md border border-border/40 p-0.5 dark:border-gray-700/50">
        <Button
          variant="ghost"
          size="icon"
          class="h-8 w-8 {showHidden ? 'text-primary' : 'text-muted-foreground'}"
          onclick={() => (showHidden = !showHidden)}
          title={showHidden ? 'Hide hidden files' : 'Show hidden files'}
        >
          {#if showHidden}
            <Eye class="h-4 w-4" />
          {:else}
            <EyeOff class="h-4 w-4" />
          {/if}
        </Button>
        <Separator orientation="vertical" class="mx-0.5 h-4" />
        <Button
          variant={viewMode === 'table' ? 'secondary' : 'ghost'}
          size="icon"
          class="h-8 w-8"
          onclick={() => (viewMode = 'table')}
        >
          <LayoutList class="h-4 w-4" />
        </Button>
        <Button
          variant={viewMode === 'grid' ? 'secondary' : 'ghost'}
          size="icon"
          class="h-8 w-8"
          onclick={() => (viewMode = 'grid')}
        >
          <LayoutGrid class="h-4 w-4" />
        </Button>
      </div>
    </div>

    <!-- Content Area -->
    <ContextMenu.Root>
      <ContextMenu.Trigger>
        {#snippet child({ props })}
          <div {...props} class="flex-1 overflow-auto p-4">
            {#if error}
              <div class="mx-auto w-full max-w-5xl py-12">
                <div class="rounded-lg border border-border bg-card p-10 md:p-14">
                  <div class="flex flex-col items-center gap-10 md:flex-row md:items-start md:gap-14">
                    <!-- Illustration -->
                    <div class="shrink-0">
                      <img src={errorIcon} alt="Error" class="w-48 md:w-60 dark:hidden" />
                      <img src={errorDarkIcon} alt="Error" class="hidden w-48 md:w-60 dark:block" />
                    </div>

                    <!-- Content -->
                    <div class="w-full flex-1 space-y-6 text-left">
                      <div>
                        <h3 class="text-2xl font-semibold text-foreground">资源列举失败</h3>
                        <p class="mt-2 text-muted-foreground">在访问指定的 ODFI 路径时发生了错误。</p>
                      </div>

                      <!-- Error Details Box -->
                      <div class="rounded-md border border-border bg-background text-sm">
                        <details class="group border-border last:border-0" open>
                          <summary
                            class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
                          >
                            <span>错误详情</span>
                            <ChevronDown
                              class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                            />
                          </summary>
                          <div class="px-4 pt-0 pb-4 text-muted-foreground">
                            <p class="rounded bg-muted p-3 font-mono text-xs leading-relaxed break-all">
                              {error}
                            </p>
                          </div>
                        </details>

                        <details class="group border-t border-border last:border-0">
                          <summary
                            class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
                          >
                            <span>排查建议</span>
                            <ChevronDown
                              class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                            />
                          </summary>
                          <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                            <ul class="ml-2 list-inside list-disc space-y-1">
                              <li>检查 ODFI 语法是否正确</li>
                              <li>确保远程代理 (Agent) 处于在线状态</li>
                              <li>如果是 S3，请检查子账户是否有对应 Bucket 的权限 (ListBuckets/ListObjects)</li>
                              <li>检查网络连接是否正常</li>
                            </ul>
                          </div>
                        </details>

                        <!-- Retry action -->
                        <div class="border-t border-border p-4">
                          <Button
                            variant="default"
                            size="sm"
                            onclick={() => loadResources(currentOdfiStr)}
                            disabled={loading}
                          >
                            <RefreshCw class="mr-2 h-4 w-4 {loading ? 'animate-spin' : ''}" />
                            重试
                          </Button>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            {:else}
              <div class="rounded-md border border-border/40 dark:border-gray-700/50">
                {#if viewMode === 'table'}
                  <table class="w-full text-sm">
                    <thead class="block w-full bg-muted/40">
                      <tr class="flex w-full">
                        <th
                          class="flex h-10 w-12 shrink-0 items-center justify-center px-4 text-left align-middle font-medium text-muted-foreground"
                        ></th>
                        <th
                          class="flex h-10 flex-1 items-center px-4 text-left align-middle font-medium text-muted-foreground"
                          >Name</th
                        >
                        <th
                          class="flex h-10 w-24 shrink-0 items-center justify-end px-4 text-right align-middle font-medium text-muted-foreground"
                          >Size</th
                        >
                        <th
                          class="flex h-10 w-40 shrink-0 items-center justify-end px-4 text-right align-middle font-medium text-muted-foreground"
                          >Modified</th
                        >
                      </tr>
                    </thead>
                    <tbody class="block max-h-[calc(100vh-16rem)] w-full overflow-y-auto">
                      {#if displayedItems.length === 0 && !loading}
                        <tr class="flex w-full border-t border-border/40 dark:border-gray-700/50">
                          <td class="w-full p-8 text-center text-muted-foreground"> This directory is empty. </td>
                        </tr>
                      {/if}
                      {#each displayedItems as item}
                        <ContextMenu.Root>
                          <ContextMenu.Trigger
                            class="flex w-full cursor-pointer border-t border-border/40 hover:bg-muted/50 dark:border-gray-700/50"
                          >
                            {#snippet child({ props })}
                              <tr {...props} onclick={() => handleRowClick(item)}>
                                <td class="flex w-12 flex-shrink-0 items-center justify-center p-2">
                                  {#if item.type === 'dir'}
                                    {@render macOSFolder('h-5 w-5', !!item.has_children)}
                                  {:else if item.type === 'linkdir'}
                                    {@render macOSFolder('h-5 w-5', !!item.has_children, Link)}
                                  {:else if item.type === 'linkfile'}
                                    {@render macOSFile('h-5 w-5', Link)}
                                  {:else}
                                    {@render macOSFile('h-5 w-5')}
                                  {/if}
                                </td>
                                <td class="flex flex-1 items-center truncate p-2 font-medium">
                                  {item.name}
                                </td>
                                <td
                                  class="flex w-24 flex-shrink-0 items-center justify-end p-2 font-mono text-xs text-muted-foreground"
                                >
                                  {formatSize(item.size)}
                                </td>
                                <td
                                  class="flex w-40 flex-shrink-0 items-center justify-end p-2 font-mono text-xs text-muted-foreground"
                                >
                                  {#if item.modified}
                                    {new Date(item.modified * 1000).toLocaleString()}
                                  {/if}
                                </td>
                              </tr>
                            {/snippet}
                          </ContextMenu.Trigger>
                          {@render itemContextMenu(item)}
                        </ContextMenu.Root>
                      {/each}
                    </tbody>
                  </table>
                {:else}
                  <!-- Grid View (Auto-fill) -->
                  <div class="grid gap-2 p-2" style="grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));">
                    {#if displayedItems.length === 0 && !loading}
                      <div class="col-span-full p-8 text-center text-muted-foreground">This directory is empty.</div>
                    {/if}
                    {#each displayedItems as item}
                      <ContextMenu.Root>
                        <ContextMenu.Trigger>
                          {#snippet child({ props })}
                            <button
                              {...props}
                              class="group flex flex-col items-center gap-1 rounded-lg border border-transparent p-2 transition-all hover:bg-muted/30 hover:shadow-sm"
                              onclick={() => handleRowClick(item)}
                            >
                              <div
                                class="flex h-[72px] w-20 items-end justify-center transition-transform group-hover:scale-105"
                              >
                                {#if item.type === 'dir'}
                                  {@render macOSFolder('h-16 w-16', !!item.has_children)}
                                {:else if item.type === 'linkdir'}
                                  {@render macOSFolder('h-16 w-16', !!item.has_children, Link)}
                                {:else if item.type === 'linkfile'}
                                  {@render macOSFile('h-16 w-16', Link)}
                                {:else}
                                  {@render macOSFile('h-16 w-16')}
                                {/if}
                              </div>
                              <span
                                class="line-clamp-2 min-h-[2.2em] w-full text-center text-[10.5px] leading-[1.1] font-medium [overflow-wrap:anywhere] [word-break:normal]"
                                title={item.name}
                              >
                                {truncateMiddle(item.name, 28, 9).replace(/(.)([_-])/g, '$1\u200B$2')}
                              </span>
                              {#if item.type === 'dir' || item.type === 'linkdir'}
                                {@const count =
                                  (showHidden
                                    ? item.child_count
                                    : (item.child_count ?? 0) - (item.hidden_child_count ?? 0)) ?? 0}
                                <span class="text-[10px] font-medium text-blue-500/80">
                                  {count === 0 ? '无项目' : `${count} 个项目`}
                                </span>
                              {:else if item.size}
                                <span class="text-[10px] font-medium text-blue-500/80">
                                  {formatSize(item.size)}
                                </span>
                              {/if}
                            </button>
                          {/snippet}
                        </ContextMenu.Trigger>
                        {@render itemContextMenu(item)}
                      </ContextMenu.Root>
                    {/each}
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        {/snippet}
      </ContextMenu.Trigger>
      {@render containerContextMenu()}
    </ContextMenu.Root>
  </div>
</div>
