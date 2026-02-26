<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { Button } from '$lib/components/ui/button';
  import { Separator } from '$lib/components/ui/separator';
  import { type ResourceItem, listResources } from '$lib/modules/explorer';
  import {
    ArrowLeft,
    RefreshCw,
    Download,
    Server,
    Cloud,
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
    Folder,
    Music,
    Film,
    Image,
    Code,
    FileArchive,
    FileBraces,
    FileText,
    Terminal
  } from 'lucide-svelte';
  import { ContextMenu } from 'bits-ui';
  import { isTextFile, isImageFile, isArchiveFile, truncateMiddle } from '$lib/modules/explorer/utils';
  import errorIcon from '$lib/assets/error.svg';
  import errorDarkIcon from '$lib/assets/error-dark.svg';

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  type IconComponent = any;

  interface SidebarItem {
    key?: string;
    label?: string;
    name: string;
    path: string;
    icon?: IconComponent;
    colorClass?: string;
  }

  // State
  let currentOrlStr = $state('');
  let items: ResourceItem[] = $state([]);
  let loading = $state(false);
  let error: string | null = $state(null);
  let urlOrl = page.url.searchParams.get('orl');
  let viewMode = $state<'table' | 'grid'>('grid');
  let showHidden = $state(false);

  // Derived filtered items
  let displayedItems = $derived.by(() => {
    if (showHidden) return items;
    return items.filter((item) => !item.name.startsWith('.'));
  });

  // Sidebar State

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

  // Derived active state from ORL
  let activeType = $derived.by(() => {
    if (currentOrlStr.startsWith('orl://local')) return 'local';
    if (currentOrlStr.startsWith('orl://s3')) return 's3';
    if (currentOrlStr.includes('@agent') || currentOrlStr.startsWith('orl://agent')) return 'agent';
    return null;
  });

  // Identify current agent name or S3 profile name
  let activeId = $derived.by(() => {
    if (activeType === 'agent') {
      // Extract from orl://id@agent...
      const match = currentOrlStr.match(/orl:\/\/([^@]+)@agent/);
      return match ? match[1] : null;
    }
    if (activeType === 's3') {
      // Extract profile from orl://profile@s3...
      const match = currentOrlStr.match(/orl:\/\/([^@]+)@s3/);
      return match ? match[1] : null;
    }
    return null;
  });

  // Initial load
  onMount(() => {
    if (urlOrl) {
      currentOrlStr = urlOrl;
      loadResources(urlOrl);
    } else {
      // Default to local root
      currentOrlStr = 'orl://local/';
      loadResources(currentOrlStr);
    }

    // Preload sidebar data
    loadSidebarData('agent');
    loadSidebarData('s3');
  });

  async function loadSidebarData(section: 's3' | 'agent') {
    sidebarLoading[section] = true;
    try {
      const rootOrl = section === 's3' ? 'orl://s3/' : 'orl://agent/';
      sidebarData[section] = await listResources(rootOrl);
    } catch (e) {
      console.error(`Failed to load sidebar ${section}`, e);
    } finally {
      sidebarLoading[section] = false;
    }
  }

  async function loadResources(orl: string): Promise<boolean> {
    loading = true;
    error = null;
    console.log('[Explorer] Loading resources for ORL:', orl);
    try {
      items = await listResources(orl);
      return true;
    } catch (e) {
      error = (e as Error).message;
      items = [];
      return false;
    } finally {
      loading = false;
    }
  }

  async function handleNavigate(newOrl: string): Promise<boolean> {
    currentOrlStr = newOrl;
    // Update URL without triggering SvelteKit navigation
    const baseUrl = window.location.origin + window.location.pathname;

    // 统一对 ORL 进行编码
    // 后端现在返回未编码的路径（如 ?entry=/home），前端负责统一编码
    // 这样可以避免双重编码问题（%2F → %252F）
    const encodedOrl = encodeURIComponent(newOrl);

    const newUrl = `${baseUrl}?orl=${encodedOrl}`;
    // 使用 goto 替代 replaceState，确保页面正确更新
    // { noScroll: true } 避免滚动影响用户体验
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    await goto(newUrl, { noScroll: true, keepFocus: true });
    return await loadResources(newOrl);
  }

  let selectedItem: ResourceItem | null = $state(null);

  function handleRowClick(item: ResourceItem) {
    selectedItem = item;
  }

  // 辅助函数：为 URL 查询参数编码 ORL
  // 后端现在返回未编码的 ORL（如 ?entry=/home），前端负责统一编码
  function encodeOrlForQueryParam(orl: string): string {
    return encodeURIComponent(orl);
  }

  function handleRowDoubleClick(item: ResourceItem) {
    console.log('[Explorer] Double-clicked item:', {
      name: item.name,
      type: item.type,
      path: item.path.substring(0, 100) + (item.path.length > 100 ? '...' : '')
    });
    console.log('[Explorer] Current ORL:', currentOrlStr);

    if (item.type === 'dir' || item.type === 'linkdir') {
      console.log('[Explorer] Processing as directory');
      console.log('[Explorer] Will navigate to:', item.path);
      handleNavigate(item.path);
    } else if (isArchiveFile(item)) {
      console.log('[Explorer] Processing as archive');
      // For archives, we navigate directly - backend auto-detects archive files
      // and lists their contents when path points to an archive file
      handleNavigate(item.path);
    } else if (isTextFile(item)) {
      console.log('[Explorer] Opening text file in /view');
      const url = `/view?sid=explorer&file=${encodeOrlForQueryParam(item.path)}`;
      window.open(url, '_blank');
    } else if (isImageFile(item)) {
      console.log('[Explorer] Opening image file in /image-view');
      const url = `/image-view?sid=explorer&file=${encodeOrlForQueryParam(item.path)}`;
      window.open(url, '_blank');
    } else {
      console.log('[Explorer] Unhandled file type:', item.type, 'for file:', item.name);
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
    if (!item.path) return;
    const url = `/api/v1/explorer/download?orl=${encodeURIComponent(item.path)}`;
    const a = document.createElement('a');
    a.href = url;
    a.download = '';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
  }

  async function goUp() {
    try {
      let urlStr = currentOrlStr;
      // eslint-disable-next-line svelte/prefer-svelte-reactivity
      const url = new URL(urlStr);

      // Check if we are inside an archive (using ?entry= query parameter)
      const entry = url.searchParams.get('entry');
      if (entry) {
        const entryParts = entry.split('/').filter((p: string) => p);
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
      const pathParts = url.pathname.split('/').filter((p: string) => p);
      if (pathParts.length > 0) {
        pathParts.pop();
        url.pathname = '/' + pathParts.join('/');

        // 清除归档相关参数，避免回退到普通目录时携带 target=archive
        url.searchParams.delete('target');
        url.searchParams.delete('entry');

        const targetOrl = url.toString();
        const success = await handleNavigate(targetOrl);

        // If navigation failed (e.g. 404/Access Denied) and we are in Agent mode,
        // it likely means we went up to a directory not in search_roots.
        // Fallback to Agent Root to show list of search roots.
        if (!success && activeType === 'agent' && activeId) {
          const rootOrl = `orl://${activeId}@agent/`;
          // Only redirect if we aren't already trying to go to root
          if (targetOrl !== rootOrl) {
            console.log('Navigation failed in Agent, falling back to root');
            handleNavigate(rootOrl);
          }
        }
      }
    } catch (e) {
      console.error('Failed to parse ORL for parent navigation', e);
      if (currentOrlStr.includes('/')) {
        const parts = currentOrlStr.split('/');
        parts.pop();
        handleNavigate(parts.join('/') || 'orl://local/');
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

  function getFileIcon(item: ResourceItem): IconComponent | null {
    if (item.mime_type) {
      if (item.mime_type.startsWith('image/')) return Image;
      if (item.mime_type.startsWith('video/')) return Film;
      if (item.mime_type.startsWith('audio/')) return Music;
      if (item.mime_type === 'application/pdf') return FileText;
      if (item.mime_type.includes('archive') || item.mime_type.includes('zip') || item.mime_type.includes('tar'))
        return FileArchive;
      if (
        item.mime_type.includes('javascript') ||
        item.mime_type.includes('typescript') ||
        item.mime_type.includes('json')
      )
        return Code;

      // Executables
      if (
        item.mime_type.includes('executable') ||
        item.mime_type.includes('mach-binary') ||
        item.mime_type.includes('elf')
      )
        return Terminal;
    }

    const lastDotIndex = item.name.lastIndexOf('.');
    if (lastDotIndex === -1) return null;

    const ext = item.name.slice(lastDotIndex + 1).toLowerCase();
    if (ext === 'json') return FileBraces;
    if (
      [
        'js',
        'ts',
        'tsx',
        'jsx',
        'py',
        'rs',
        'go',
        'java',
        'c',
        'cpp',
        'h',
        'hpp',
        'sh',
        'bash',
        'zsh',
        'yaml',
        'yml',
        'toml',
        'sql',
        'svelte'
      ].includes(ext || '')
    )
      return Code;
    if (['zip', 'rar', '7z', 'tar', 'gz', 'bz2', 'xz'].includes(ext || '')) return FileArchive;
    if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp'].includes(ext || '')) return Image;
    if (['mp4', 'mkv', 'avi', 'mov'].includes(ext || '')) return Film;
    if (['mp3', 'wav', 'ogg', 'flac'].includes(ext || '')) return Music;

    return null;
  }
</script>

<svelte:window onmousemove={handleMouseMove} onmouseup={stopResizing} />

<div class="flex h-screen gap-8 overflow-hidden px-6 py-6">
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
        <h3 class="mb-3 text-sm font-light tracking-wider text-foreground uppercase opacity-70">Explorer</h3>
        <Separator class="mb-4" />

        {#snippet renderLevel(items: SidebarItem[], depth: number)}
          <div class="space-y-0.5">
            {#each items as item (item.path)}
              {@const isActive =
                (depth === 0 && activeType === item.key) ||
                (depth === 1 && (activeId === item.name || currentOrlStr.startsWith(item.path)))}
              <button
                class="group flex w-full items-center rounded-md px-2 py-1.5 text-sm transition-colors {isActive
                  ? 'bg-primary/10 text-primary'
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
                  <div class="mr-2 h-1.5 w-1.5 flex-0 rounded-full {item.colorClass || 'bg-muted-foreground'}"></div>
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
                {@const mappedChildren = children.map(
                  (c) =>
                    ({
                      ...c,
                      colorClass: activeRoot.key === 's3' ? 'bg-blue-500' : 'bg-green-500'
                    }) as SidebarItem
                )}
                {@render renderLevel(mappedChildren, 1)}
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
            { key: 'local', label: 'Local Machine', name: 'local', path: 'orl://local/', icon: Monitor },
            { key: 'agent', label: 'Remote Agents', name: 'agent', path: 'orl://agent/', icon: Server },
            { key: 's3', label: 'S3 Storage', name: 's3', path: 'orl://s3/', icon: Cloud }
          ],
          0
        )}
      </div>
    </div>
  </aside>

  {#snippet macOSFolder(className = 'h-16 w-16', hasFiles = true, icon: IconComponent | null = null)}
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

  {#snippet macOSFile(className = 'h-16 w-16', icon: IconComponent | null = null, isText: boolean = false)}
    <div class="relative {className} flex items-center justify-center">
      <div class="relative h-full w-[76%]">
        <!-- Unified SVG for perfect alignment and realistic shadows -->
        <svg viewBox="0 0 82 100" class="h-full w-full drop-shadow-[0_1px_1.5px_rgba(0,0,0,0.12)]">
          <defs>
            <linearGradient id="flapGrad" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stop-color="#ffffff" />
              <stop offset="40%" stop-color="#f3f4f6" />
              <stop offset="100%" stop-color="#d1d5db" />
            </linearGradient>
            <linearGradient id="bodyGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color="#ffffff" />
              <stop offset="100%" stop-color="#f8fafc" />
            </linearGradient>
            <!-- Shadow cast by the fold -->
            <filter id="softShadow" x="-50%" y="-50%" width="200%" height="200%">
              <feGaussianBlur in="SourceAlpha" stdDeviation="2" />
              <feOffset dx="-2" dy="2" result="offsetblur" />
              <feComponentTransfer>
                <feFuncA type="linear" slope="0.4" />
              </feComponentTransfer>
              <feMerge>
                <feMergeNode />
              </feMerge>
            </filter>
          </defs>
          <!-- Page Body -->
          <path
            d="M0,2 Q0,0 2,0 L58,0 L82,24 L82,98 Q82,100 80,100 L2,100 Q0,100 0,98 Z"
            fill="url(#bodyGrad)"
            stroke="#cbd5e1"
            stroke-width="0.5"
            class="dark:fill-gray-100 dark:stroke-gray-500"
          />

          <!-- Crease Shadow (Cast on body) -->
          <path d="M58,0 L82,24 L58,24 Z" fill="black" filter="url(#softShadow)" />

          <!-- The Fold (Flap) -->
          <path
            d="M58,0 L58,22 Q58,24 60,24 L82,24 Z"
            fill="url(#flapGrad)"
            stroke="#cbd5e1"
            stroke-width="0.4"
            class="dark:fill-gray-200 dark:stroke-gray-400"
          />
        </svg>

        <!-- Overlays (Text/Icon) -->
        <div class="absolute inset-0">
          {#if icon}
            {@const IconComp = icon}
            <div class="absolute inset-0 flex items-center justify-center text-slate-500 opacity-55 dark:opacity-65">
              <IconComp class={className.includes('h-5') ? 'h-3.5 w-3.5' : 'h-10 w-10'} />
            </div>
          {:else if isText}
            <div class="flex h-full w-full flex-col gap-[3px] p-[18%] pt-[38%] opacity-45 dark:opacity-55">
              <div class="h-[1.5px] w-full bg-slate-500"></div>
              <div class="h-[1.5px] w-[90%] bg-slate-500"></div>
              <div class="h-[1.5px] w-full bg-slate-500"></div>
              <div class="h-[1.5px] w-[80%] bg-slate-500"></div>
              <div class="h-[1.5px] w-full bg-slate-500"></div>
              <div class="h-[1.5px] w-[95%] bg-slate-500"></div>
              <div class="h-[1.5px] w-full bg-slate-500"></div>
              <div class="h-[1.5px] w-[70%] bg-slate-500"></div>
            </div>
          {/if}
        </div>
      </div>
    </div>
  {/snippet}

  {#snippet macOSArchive(className = 'h-16 w-16', name = '')}
    {@const ext = name.split('.').pop()?.toUpperCase() || ''}
    <div class="relative {className} flex items-center justify-center">
      <div class="relative h-full w-[76%]">
        <svg viewBox="0 0 82 100" class="h-full w-full drop-shadow-[0_1px_1.5px_rgba(0,0,0,0.12)]">
          <defs>
            <linearGradient id="archiveBodyGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color="#ffffff" />
              <stop offset="100%" stop-color="#f8fafc" />
            </linearGradient>
          </defs>
          <path
            d="M0,2 Q0,0 2,0 L58,0 L82,24 L82,98 Q82,100 80,100 L2,100 Q0,100 0,98 Z"
            fill="url(#archiveBodyGrad)"
            stroke="#cbd5e1"
            stroke-width="0.5"
            class="dark:fill-gray-100 dark:stroke-gray-500"
          />
          <path
            d="M58,0 L58,22 Q58,24 60,24 L82,24 Z"
            fill="#f3f4f6"
            stroke="#cbd5e1"
            stroke-width="0.4"
            class="dark:fill-gray-200 dark:stroke-gray-400"
          />

          <!-- Zipper Track -->
          <g transform="translate(37, 24)">
            {#each Array.from({ length: 10 }).map((_, i) => i) as i (i)}
              <rect x={i % 2 === 0 ? 0 : 5} y={i * 4} width="3" height="1.6" fill="#94a3b8" rx="0.3" />
            {/each}
          </g>

          <!-- Zipper Pull (At top) -->
          <g transform="translate(35, 12)">
            <rect x="0" y="0" width="12" height="16" rx="3.5" fill="#64748b" />
            <circle cx="6" cy="11" r="2.2" fill="white" opacity="0.4" />
          </g>

          <!-- Extension Label -->
          {#if ext}
            <text
              x="41"
              y="86"
              text-anchor="middle"
              class="fill-slate-400 font-sans text-[11px] font-medium tracking-tighter uppercase"
            >
              {ext}
            </text>
          {/if}
        </svg>
      </div>
    </div>
  {/snippet}

  {#snippet itemContextMenu(item: ResourceItem)}
    {#if item && item.name}
      <ContextMenu.Content
        class="z-50 min-w-[200px] overflow-hidden rounded-md border border-black/10 bg-white/95 p-1 text-neutral-900 shadow-lg backdrop-blur-xl dark:border-white/10 dark:bg-[#1c1c1e]/95 dark:text-white/90"
      >
        {#if item.type === 'dir' || item.type === 'linkdir' || isTextFile(item) || isImageFile(item)}
          <ContextMenu.Item
            class="flex h-8 cursor-pointer items-center rounded-md px-2 py-0 text-sm transition-colors outline-none data-highlighted:bg-[#007aff] data-highlighted:text-white"
            onSelect={() => handleRowDoubleClick(item)}
          >
            <ExternalLink class="mr-3 h-3.5 w-3.5 opacity-50 dark:opacity-60" />
            <span>打开</span>
          </ContextMenu.Item>
          <ContextMenu.Separator class="my-1 h-px bg-black/5 dark:bg-white/10" />
        {/if}

        <ContextMenu.Item
          class="flex h-8 cursor-pointer items-center rounded-md px-2 py-0 text-sm transition-colors outline-none data-highlighted:bg-[#007aff] data-highlighted:text-white"
          onSelect={() => copyToClipboard(item.path)}
        >
          <Link class="mr-3 h-3.5 w-3.5 opacity-50 dark:opacity-60" />
          <span>复制 ORL 路径</span>
        </ContextMenu.Item>

        <ContextMenu.Item
          class="flex h-8 cursor-pointer items-center rounded-md px-2 py-0 text-sm transition-colors outline-none data-highlighted:bg-[#007aff] data-highlighted:text-white"
          onSelect={() => copyToClipboard(item.name)}
        >
          <Copy class="mr-3 h-3.5 w-3.5 opacity-50 dark:opacity-60" />
          <span>复制名称</span>
        </ContextMenu.Item>

        {#if item.type === 'file' || item.type === 'linkfile'}
          <ContextMenu.Separator class="my-1 h-px bg-black/5 dark:bg-white/10" />
          <ContextMenu.Item
            class="flex h-8 cursor-pointer items-center rounded-md px-2 py-0 text-sm transition-colors outline-none data-highlighted:bg-[#007aff] data-highlighted:text-white"
            onSelect={() => handleDownload(item)}
          >
            <Download class="mr-3 h-3.5 w-3.5 opacity-50 dark:opacity-60" />
            <span>下载</span>
          </ContextMenu.Item>
        {/if}

        <ContextMenu.Separator class="my-1 h-px bg-black/5 dark:bg-white/10" />

        <ContextMenu.Item
          class="flex h-8 cursor-pointer items-center rounded-md px-2 py-0 text-sm transition-colors outline-none data-highlighted:bg-[#007aff] data-highlighted:text-white"
        >
          <Info class="mr-3 h-3.5 w-3.5 opacity-50 dark:opacity-60" />
          <span>属性</span>
        </ContextMenu.Item>

        <ContextMenu.Separator class="bg-black/5 dark:bg-white/10" />

        <!-- macOS Style Tags Row -->
        <div class="flex items-center justify-between px-3 py-2">
          <div class="flex w-full justify-between px-1">
            {#each [{ color: '#ff5f57', label: '红色' }, { color: '#febc2e', label: '橙色' }, { color: '#fedd34', label: '黄色' }, { color: '#28c840', label: '绿色' }, { color: '#4a90e2', label: '蓝色' }, { color: '#a370f7', label: '紫色' }, { color: '#8e8e93', label: '灰色' }] as tag (tag.color)}
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
    <ContextMenu.Content
      class="z-50 min-w-[200px] overflow-hidden rounded-md border border-black/10 bg-white/95 p-1 text-neutral-900 shadow-lg backdrop-blur-xl dark:border-white/10 dark:bg-[#1c1c1e]/95 dark:text-white/90"
    >
      <ContextMenu.Item
        class="flex h-8 cursor-pointer items-center rounded-md px-2 py-0 text-sm transition-colors outline-none data-highlighted:bg-[#007aff] data-highlighted:text-white"
        onSelect={() => loadResources(currentOrlStr)}
      >
        <RefreshCw class="mr-3 h-3.5 w-3.5 opacity-50 dark:opacity-60" />
        <span>刷新</span>
      </ContextMenu.Item>
    </ContextMenu.Content>
  {/snippet}

  <!-- Main Content -->
  <main data-testid="explorer-container" class="flex flex-1 flex-col overflow-hidden bg-background">
    <!-- Toolbar -->
    <div class="flex items-center space-x-2 border-b border-border/40 p-4 dark:border-gray-700/50">
      <Button variant="ghost" size="icon" onclick={goUp} disabled={loading} title="后退">
        <ArrowLeft class="h-4 w-4" />
      </Button>
      <Button variant="ghost" size="icon" onclick={() => loadResources(currentOrlStr)} disabled={loading} title="刷新">
        <RefreshCw class="h-4 w-4 {loading ? 'animate-spin' : ''}" />
      </Button>
      <div
        class="flex flex-1 items-center rounded-md border border-border/40 bg-muted/50 px-3 py-1.5 focus-within:ring-1 focus-within:ring-ring dark:border-gray-700/50"
      >
        <input
          id="orl-input"
          class="w-full flex-1 border-none bg-transparent font-mono text-sm font-light outline-none"
          bind:value={currentOrlStr}
          onkeydown={(e) => e.key === 'Enter' && handleNavigate(currentOrlStr)}
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
    <div
      data-testid="explorer-content"
      class="relative flex-1 overflow-auto p-4"
      onclick={() => (selectedItem = null)}
      role="button"
      tabindex="0"
      onkeydown={() => {}}
    >
      <!-- Layer 1: Background Context Menu (Container Actions) -->
      <ContextMenu.Root>
        <ContextMenu.Trigger>
          {#snippet child({ props })}
            <div
              {...props}
              class="absolute inset-0 z-0 h-full w-full cursor-default"
              data-menu-trigger="container"
            ></div>
          {/snippet}
        </ContextMenu.Trigger>
        {@render containerContextMenu()}
      </ContextMenu.Root>

      <!-- Layer 2: Content (Foreground) -->
      <div class="pointer-events-none relative z-10 min-h-full">
        {#if error}
          <div class="pointer-events-auto mx-auto w-full max-w-5xl py-12">
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
                    <h3 class="text-2xl font-normal text-foreground">资源列举失败</h3>
                    <p class="mt-2 text-muted-foreground">在访问指定的 ORL 路径时发生了错误。</p>
                  </div>

                  <!-- Error Details Box -->
                  <div class="rounded-md border border-border bg-background text-sm">
                    <details class="group border-border last:border-0" open>
                      <summary
                        class="flex cursor-pointer items-center justify-between p-4 select-none hover:bg-muted/50"
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
                        class="flex cursor-pointer items-center justify-between p-4 select-none hover:bg-muted/50"
                      >
                        <span>排查建议</span>
                        <ChevronDown
                          class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                        />
                      </summary>
                      <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                        <ul class="ml-2 list-inside list-disc space-y-1">
                          <li>检查 ORL 语法是否正确</li>
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
                        onclick={() => loadResources(currentOrlStr)}
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
                      class="flex h-10 w-12 shrink-0 items-center justify-center px-4 text-left align-middle text-muted-foreground"
                    ></th>
                    <th class="flex h-10 flex-1 items-center px-4 text-left align-middle text-muted-foreground">Name</th
                    >
                    <th
                      class="flex h-10 w-24 shrink-0 items-center justify-end px-4 text-right align-middle text-muted-foreground"
                      >Size</th
                    >
                    <th
                      class="flex h-10 w-40 shrink-0 items-center justify-end px-4 text-right align-middle text-muted-foreground"
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
                  {#each displayedItems as item (item.path)}
                    <ContextMenu.Root>
                      <ContextMenu.Trigger
                        class="pointer-events-auto flex w-full cursor-pointer border-t border-border/40 hover:bg-muted/50 dark:border-gray-700/50"
                      >
                        {#snippet child({ props })}
                          <tr
                            {...props}
                            onclick={(e) => {
                              e.stopPropagation();
                              handleRowClick(item);
                            }}
                            oncontextmenu={(e) => {
                              handleRowClick(item);
                              // eslint-disable-next-line @typescript-eslint/no-explicit-any
                              (props as any).oncontextmenu?.(e);
                            }}
                            ondblclick={() => handleRowDoubleClick(item)}
                          >
                            <td class="flex w-14 flex-0 items-center justify-center p-2">
                              {#if item.type === 'dir'}
                                {@render macOSFolder(
                                  'h-5 w-5',
                                  !!item.has_children,
                                  (getFileIcon(item) as IconComponent) || Folder
                                )}
                              {:else if item.type === 'linkdir'}
                                {@render macOSFolder(
                                  'h-5 w-5',
                                  !!item.has_children,
                                  (getFileIcon(item) as IconComponent) || Folder
                                )}
                              {:else if item.type === 'linkfile'}
                                {@render macOSFile('h-5 w-5', Link, isTextFile(item))}
                              {:else if (getFileIcon(item) as IconComponent) === FileArchive}
                                {@render macOSArchive('h-5 w-5', item.name)}
                              {:else}
                                {@render macOSFile('h-5 w-5', getFileIcon(item), isTextFile(item))}
                              {/if}
                            </td>
                            <td class="flex flex-1 items-center truncate p-2">
                              {item.name}
                            </td>
                            <td
                              class="flex w-24 flex-0 items-center justify-end p-2 font-mono text-xs font-light text-muted-foreground"
                            >
                              {formatSize(item.size)}
                            </td>
                            <td
                              class="flex w-40 flex-0 items-center justify-end p-2 font-mono text-xs font-light text-muted-foreground"
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
                {#each displayedItems as item (item.path)}
                  <div class="flex flex-col items-center gap-0">
                    <ContextMenu.Root>
                      <ContextMenu.Trigger>
                        {#snippet child({ props })}
                          <div
                            {...props}
                            class="group/icon pointer-events-auto flex h-20 w-20 cursor-pointer items-center justify-center rounded-xl transition-all hover:bg-black/5 active:bg-blue-500/20 dark:hover:bg-white/5 {selectedItem ===
                            item
                              ? 'bg-black/10 dark:bg-white/10'
                              : ''}"
                            onclick={(e) => {
                              e.stopPropagation();
                              handleRowClick(item);
                            }}
                            oncontextmenu={(e) => {
                              handleRowClick(item);
                              // eslint-disable-next-line @typescript-eslint/no-explicit-any
                              (props as any).oncontextmenu?.(e);
                            }}
                            ondblclick={() => handleRowDoubleClick(item)}
                            role="button"
                            tabindex="0"
                          >
                            <div class="transition-transform group-hover/icon:scale-105">
                              {#if item.type === 'dir'}
                                {@render macOSFolder(
                                  'h-[70px] w-[70px]',
                                  !!item.has_children,
                                  (getFileIcon(item) as IconComponent) || Folder
                                )}
                              {:else if item.type === 'linkdir'}
                                {@render macOSFolder(
                                  'h-[70px] w-[70px]',
                                  !!item.has_children,
                                  (getFileIcon(item) as IconComponent) || Folder
                                )}
                              {:else if item.type === 'linkfile'}
                                {@render macOSFile('h-[62px] w-[62px]', Link, isTextFile(item))}
                              {:else if (getFileIcon(item) as IconComponent) === FileArchive}
                                {@render macOSArchive('h-[62px] w-[62px]', item.name)}
                              {:else}
                                {@render macOSFile('h-[62px] w-[62px]', getFileIcon(item), isTextFile(item))}
                              {/if}
                            </div>
                          </div>
                        {/snippet}
                      </ContextMenu.Trigger>
                      {@render itemContextMenu(item)}
                    </ContextMenu.Root>

                    <ContextMenu.Root>
                      <ContextMenu.Trigger>
                        {#snippet child({ props })}
                          <div
                            {...props}
                            class="flex flex-col items-center"
                            oncontextmenu={(e) => {
                              handleRowClick(item);
                              // eslint-disable-next-line @typescript-eslint/no-explicit-any
                              (props as any).oncontextmenu?.(e);
                            }}
                          >
                            <button
                              class="pointer-events-auto block max-w-[124px] rounded-[3px] px-1.5 py-0.5 text-center text-[10.5px] leading-tight font-light break-all transition-colors {selectedItem ===
                              item
                                ? 'bg-[#0060df] text-white'
                                : 'hover:bg-blue-500 hover:text-white active:bg-blue-600'}"
                              onclick={(e) => {
                                e.stopPropagation();
                                handleRowClick(item);
                              }}
                              ondblclick={() => handleRowDoubleClick(item)}
                              title={item.name}
                            >
                              {truncateMiddle(item.name, 24, 8)}
                            </button>
                            {#if (item.type === 'dir' || item.type === 'linkdir') && item.child_count != null}
                              {@const count =
                                (showHidden
                                  ? (item.child_count ?? 0)
                                  : (item.child_count ?? 0) - (item.hidden_child_count ?? 0)) ?? 0}
                              <span class="mt-0.5 text-[9.5px] text-muted-foreground/80">
                                {count === 0 ? '无项目' : `${count} 个项目`}
                              </span>
                            {:else if item.size}
                              <span class="mt-0.5 text-[9.5px] text-muted-foreground/80">
                                {formatSize(item.size)}
                              </span>
                            {/if}
                          </div>
                        {/snippet}
                      </ContextMenu.Trigger>
                      {@render itemContextMenu(item)}
                    </ContextMenu.Root>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  </main>
</div>
