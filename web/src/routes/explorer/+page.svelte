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

  function getFolderIcon(name: string) {
    const n = name.toLowerCase();
    if (n.includes('download')) return Download;
    if (n.includes('picture') || n.includes('image')) return ImageIcon;
    if (n.includes('desktop')) return Monitor;
    if (n.includes('music')) return Music;
    if (n.includes('movie') || n.includes('video')) return Film;
    if (n.includes('cloud') || name.startsWith('odfi://s3')) return Cloud;
    if (n.includes('agent')) return Server;
    return null;
  }
</script>

<svelte:window onmousemove={handleMouseMove} onmouseup={stopResizing} />

<div class="flex h-[calc(100vh-4rem)] gap-8 overflow-hidden px-6 py-6">
  <!-- Sidebar -->
  <aside
    class="group/sidebar border-border relative hidden h-full border-r pr-6 md:block"
    style="width: {sidebarWidth}px"
  >
    <!-- 拖动把手 -->
    <button
      type="button"
      class="hover:bg-primary/20 absolute -right-1 top-0 z-10 h-full w-2 cursor-col-resize border-0 bg-transparent p-0 transition-colors"
      onmousedown={startResizing}
      aria-label="调整侧边栏宽度"
    ></button>

    <div class="sticky top-0 max-h-full space-y-6 overflow-y-auto pr-2">
      <div>
        <h3 class="text-foreground mb-3 text-sm font-semibold">Explorer</h3>
        <Separator class="mb-4" />

        {#snippet renderLevel(items: any[], depth: number)}
          <div class="space-y-0.5">
            {#each items as item}
              {@const isActive =
                (depth === 0 && activeType === item.key) ||
                (depth === 1 && (activeId === item.name || currentOdfiStr.startsWith(item.path)))}
              <button
                class="group flex w-full items-center rounded-md px-2 py-1.5 text-sm transition-colors {isActive
                  ? 'bg-primary/10 text-primary font-medium'
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
                <div class="text-muted-foreground mt-4 animate-pulse px-2 py-1 text-xs">Loading...</div>
              {:else}
                <div class="text-muted-foreground mt-4 px-2 py-1 text-xs">
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

  {#snippet macOSFolder(className = 'h-20 w-20', hasFiles = true, icon: any = null)}
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
            <feOffset dx="0" dy="-0.5" result="offsetblur" />
            <feFlood flood-color="black" flood-opacity="0.2" />
            <feComposite in2="offsetblur" operator="in" />
            <feMerge>
              <feMergeNode />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
        </defs>
        <!-- Folder Body with tab - Shoulder at y=17 -->
        <path
          d="M10,12 L35,12 L42,17 L90,17 C94,17 95,18 95,22 L95,83 C95,87 94,88 90,88 L10,88 C6,88 5,87 5,83 L5,17 C5,13 6,12 10,12 Z"
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
        <!-- Folder Front - Lowered to y=25 to reveal more back shoulder context -->
        <path
          d="M5,25 L95,25 L95,83 C95,87 94,88 90,88 L10,88 C6,88 5,87 5,83 L5,25 Z"
          fill="url(#folderFrontGrad)"
          filter="url(#frontShadow)"
        />
        <!-- Bottom/Side Stroke for Front -->
        <path
          d="M95,25 L95,83 C95,87 94,88 90,88 L10,88 C6,88 5,87 5,83 L5,25"
          fill="none"
          stroke="#2d8cdb"
          stroke-width="0.5"
        />
      </svg>
      {#if icon}
        {@const IconComp = icon}
        <div class="absolute left-0 right-0 top-[62%] flex -translate-y-1/2 items-center justify-center">
          <!-- Subtle bottom highlight -->
          <div
            class="absolute inset-0 flex translate-y-[0.5px] items-center justify-center opacity-20 mix-blend-overlay"
          >
            <IconComp class={className.includes('h-6') ? 'h-3 w-3' : 'h-8 w-8'} strokeWidth={2} color="white" />
          </div>
          <!-- Soft recessed inner shadow -->
          <div class="opacity-30 mix-blend-multiply dark:mix-blend-overlay">
            <IconComp class={className.includes('h-6') ? 'h-3 w-3' : 'h-8 w-8'} strokeWidth={2} />
          </div>
        </div>
      {/if}
    </div>
  {/snippet}

  {#snippet macOSFile(className = 'h-20 w-20', icon: any = null)}
    <div class="relative {className} flex items-center justify-center">
      <div
        class="relative h-full w-[80%] rounded-[4px] border border-gray-200 bg-white shadow-[0_1px_3px_rgba(0,0,0,0.1),0_1px_2px_rgba(0,0,0,0.06)] dark:border-gray-700 dark:bg-gray-100"
      >
        <div class="absolute right-0 top-0 h-[20%] w-[30%]">
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
      <ContextMenu.Content class="w-56">
        <ContextMenu.Item onclick={() => handleRowClick(item)}>
          <ExternalLink class="mr-2 h-4 w-4" />
          <span>打开</span>
        </ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item onclick={() => copyToClipboard(item.path)}>
          <Link class="mr-2 h-4 w-4" />
          <span>复制 ODFI 路径</span>
        </ContextMenu.Item>
        <ContextMenu.Item onclick={() => copyToClipboard(item.name)}>
          <Copy class="mr-2 h-4 w-4" />
          <span>复制名称</span>
        </ContextMenu.Item>
        <ContextMenu.Separator />
        {#if item.type === 'file' || item.type === 'linkfile'}
          <ContextMenu.Item onclick={() => handleDownload(item)}>
            <Download class="mr-2 h-4 w-4" />
            <span>下载</span>
          </ContextMenu.Item>
        {/if}
        <ContextMenu.Separator />
        <ContextMenu.Item>
          <Info class="mr-2 h-4 w-4" />
          <span>属性</span>
        </ContextMenu.Item>
      </ContextMenu.Content>
    {/if}
  {/snippet}

  {#snippet containerContextMenu()}
    <ContextMenu.Content class="w-56">
      <ContextMenu.Item onclick={() => loadResources(currentOdfiStr)}>
        <RefreshCw class="mr-2 h-4 w-4" />
        <span>刷新</span>
      </ContextMenu.Item>
    </ContextMenu.Content>
  {/snippet}

  <!-- Main Content -->
  <div class="bg-background flex flex-1 flex-col overflow-hidden">
    <!-- Toolbar -->
    <div class="border-border/40 flex items-center space-x-2 border-b p-4 dark:border-gray-700/50">
      <Button variant="ghost" size="icon" onclick={goUp} disabled={loading}>
        <ArrowLeft class="h-4 w-4" />
      </Button>
      <Button variant="ghost" size="icon" onclick={() => loadResources(currentOdfiStr)} disabled={loading}>
        <RefreshCw class="h-4 w-4 {loading ? 'animate-spin' : ''}" />
      </Button>
      <div
        class="border-border/40 bg-muted/50 focus-within:ring-ring flex flex-1 items-center rounded-md border px-3 py-1.5 focus-within:ring-1 dark:border-gray-700/50"
      >
        <input
          class="w-full flex-1 border-none bg-transparent font-mono text-sm outline-none"
          bind:value={currentOdfiStr}
          onkeydown={(e) => e.key === 'Enter' && handleNavigate(currentOdfiStr)}
        />
      </div>

      <div class="border-border/40 flex items-center rounded-md border p-0.5 dark:border-gray-700/50">
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
                <div class="border-border bg-card rounded-lg border p-10 md:p-14">
                  <div class="flex flex-col items-center gap-10 md:flex-row md:items-start md:gap-14">
                    <!-- Illustration -->
                    <div class="shrink-0">
                      <img src={errorIcon} alt="Error" class="w-48 md:w-60 dark:hidden" />
                      <img src={errorDarkIcon} alt="Error" class="hidden w-48 md:w-60 dark:block" />
                    </div>

                    <!-- Content -->
                    <div class="w-full flex-1 space-y-6 text-left">
                      <div>
                        <h3 class="text-foreground text-2xl font-semibold">资源列举失败</h3>
                        <p class="text-muted-foreground mt-2">在访问指定的 ODFI 路径时发生了错误。</p>
                      </div>

                      <!-- Error Details Box -->
                      <div class="border-border bg-background rounded-md border text-sm">
                        <details class="border-border group last:border-0" open>
                          <summary
                            class="hover:bg-muted/50 flex cursor-pointer select-none items-center justify-between p-4 font-medium"
                          >
                            <span>错误详情</span>
                            <ChevronDown
                              class="text-muted-foreground h-4 w-4 transition-transform duration-200 group-open:rotate-180"
                            />
                          </summary>
                          <div class="text-muted-foreground px-4 pb-4 pt-0">
                            <p class="bg-muted break-all rounded p-3 font-mono text-xs leading-relaxed">
                              {error}
                            </p>
                          </div>
                        </details>

                        <details class="border-border group border-t last:border-0">
                          <summary
                            class="hover:bg-muted/50 flex cursor-pointer select-none items-center justify-between p-4 font-medium"
                          >
                            <span>排查建议</span>
                            <ChevronDown
                              class="text-muted-foreground h-4 w-4 transition-transform duration-200 group-open:rotate-180"
                            />
                          </summary>
                          <div class="text-muted-foreground space-y-2 px-4 pb-4 pt-0">
                            <ul class="ml-2 list-inside list-disc space-y-1">
                              <li>检查 ODFI 语法是否正确</li>
                              <li>确保远程代理 (Agent) 处于在线状态</li>
                              <li>如果是 S3，请检查子账户是否有对应 Bucket 的权限 (ListBuckets/ListObjects)</li>
                              <li>检查网络连接是否正常</li>
                            </ul>
                          </div>
                        </details>

                        <!-- Retry action -->
                        <div class="border-border border-t p-4">
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
              <div class="border-border/40 rounded-md border dark:border-gray-700/50">
                {#if viewMode === 'table'}
                  <table class="w-full text-sm">
                    <thead class="bg-muted/40 block w-full">
                      <tr class="flex w-full">
                        <th
                          class="text-muted-foreground flex h-10 w-12 shrink-0 items-center justify-center px-4 text-left align-middle font-medium"
                        ></th>
                        <th
                          class="text-muted-foreground flex h-10 flex-1 items-center px-4 text-left align-middle font-medium"
                          >Name</th
                        >
                        <th
                          class="text-muted-foreground flex h-10 w-24 shrink-0 items-center justify-end px-4 text-right align-middle font-medium"
                          >Size</th
                        >
                        <th
                          class="text-muted-foreground flex h-10 w-40 shrink-0 items-center justify-end px-4 text-right align-middle font-medium"
                          >Modified</th
                        >
                      </tr>
                    </thead>
                    <tbody class="block max-h-[calc(100vh-16rem)] w-full overflow-y-auto">
                      {#if displayedItems.length === 0 && !loading}
                        <tr class="border-border/40 flex w-full border-t dark:border-gray-700/50">
                          <td class="text-muted-foreground w-full p-8 text-center"> This directory is empty. </td>
                        </tr>
                      {/if}
                      {#each displayedItems as item}
                        <ContextMenu.Root>
                          <ContextMenu.Trigger
                            class="border-border/40 hover:bg-muted/50 flex w-full cursor-pointer border-t dark:border-gray-700/50"
                          >
                            {#snippet child({ props })}
                              <tr {...props} onclick={() => handleRowClick(item)}>
                                <td class="flex w-12 flex-shrink-0 items-center justify-center p-2">
                                  {#if item.type === 'dir'}
                                    {@render macOSFolder('h-6 w-6', !!item.has_children, getFolderIcon(item.name))}
                                  {:else if item.type === 'linkdir'}
                                    {@render macOSFolder('h-6 w-6', !!item.has_children, Link)}
                                  {:else if item.type === 'linkfile'}
                                    {@render macOSFile('h-6 w-6', Link)}
                                  {:else}
                                    {@render macOSFile('h-6 w-6')}
                                  {/if}
                                </td>
                                <td class="flex flex-1 items-center truncate p-2 font-medium">
                                  {item.name}
                                </td>
                                <td
                                  class="text-muted-foreground flex w-24 flex-shrink-0 items-center justify-end p-2 font-mono text-xs"
                                >
                                  {formatSize(item.size)}
                                </td>
                                <td
                                  class="text-muted-foreground flex w-40 flex-shrink-0 items-center justify-end p-2 font-mono text-xs"
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
                      <div class="text-muted-foreground col-span-full p-8 text-center">This directory is empty.</div>
                    {/if}
                    {#each displayedItems as item}
                      <ContextMenu.Root>
                        <ContextMenu.Trigger>
                          {#snippet child({ props })}
                            <button
                              {...props}
                              class="hover:bg-muted/30 group flex flex-col items-center gap-2 rounded-lg border border-transparent p-2 transition-all hover:shadow-sm"
                              onclick={() => handleRowClick(item)}
                            >
                              <div
                                class="flex h-24 w-24 items-center justify-center transition-transform group-hover:scale-105"
                              >
                                {#if item.type === 'dir'}
                                  {@render macOSFolder('h-20 w-20', !!item.has_children, getFolderIcon(item.name))}
                                {:else if item.type === 'linkdir'}
                                  {@render macOSFolder('h-20 w-20', !!item.has_children, Link)}
                                {:else if item.type === 'linkfile'}
                                  {@render macOSFile('h-20 w-20', Link)}
                                {:else}
                                  {@render macOSFile('h-20 w-20')}
                                {/if}
                              </div>
                              <span class="w-full truncate text-center text-xs font-medium" title={item.name}>
                                {item.name}
                              </span>
                              {#if item.size}
                                <span class="text-muted-foreground text-[10px]">
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
