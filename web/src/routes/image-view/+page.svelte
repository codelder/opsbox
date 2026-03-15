<script lang="ts">
  import { onMount } from 'svelte';
  import { getApiBase } from '$lib/modules/logseek/api/config';
  import { Button } from '$lib/components/ui/button';
  import LogSeekLogo from '$lib/components/LogSeekLogo.svelte';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import { listResources, type ResourceItem } from '$lib/modules/explorer';
  import { isImageFile } from '$lib/modules/explorer/utils';
  import {
    ZoomIn,
    ZoomOut,
    RotateCw,
    Download,
    Maximize,
    RotateCcw,
    X,
    ChevronLeft,
    ChevronRight
  } from 'lucide-svelte';

  let sid = $state('');
  let file = $state('');
  let imageUrl = $state('');
  let fileName = $state('');

  // Carousel state
  let peerImages: ResourceItem[] = $state([]);
  let currentIndex = $state(-1);

  // Transform state
  let scale = $state(1);
  let rotation = $state(0);
  let translation = $state({ x: 0, y: 0 });
  let isDragging = $state(false);
  let startPos = { x: 0, y: 0 };

  let imgElement: HTMLImageElement | null = $state(null);
  let loading = $state(true);
  let error = $state<string | null>(null);

  onMount(() => {
    initFromUrl();
    window.addEventListener('popstate', handlePopState);
    return () => window.removeEventListener('popstate', handlePopState);
  });

  function handlePopState() {
    initFromUrl();
  }

  async function initFromUrl() {
    const params = new URL(window.location.href).searchParams;
    const newFile = params.get('file') || '';
    const newSid = params.get('sid') || '';

    // If same file, no need to reload everything
    if (newFile === file && newSid === sid && imageUrl) return;

    file = newFile;
    sid = newSid;

    if (!file) {
      error = '参数无效：缺少 file 参数';
      loading = false;
      return;
    }

    if (!sid) {
      error = '参数无效：缺少 sid 参数';
      loading = false;
      return;
    }

    loading = true;
    error = null;
    const rawName = file.split('?')[0].split('/').pop() || 'Image';
    try {
      fileName = decodeURIComponent(rawName);
    } catch {
      fileName = rawName;
    }

    const apiBase = getApiBase();
    const searchParams = new URLSearchParams({ sid, file });
    imageUrl = `${apiBase}/view/raw?${searchParams.toString()}`;

    // Load peer images if directory changed or not loaded
    await loadPeerImages();
  }

  async function loadPeerImages() {
    try {
      // 1. Clean path and extract parent directory ODFI
      const cleanFile = file.endsWith('/') ? file.slice(0, -1) : file;
      const lastSlash = cleanFile.lastIndexOf('/');
      if (lastSlash === -1) return;

      const parentOrl = cleanFile.substring(0, lastSlash + 1);

      // Only fetch if parent directory actually changed or list is empty
      const currentParent =
        peerImages.length > 0 ? peerImages[0].path.substring(0, peerImages[0].path.lastIndexOf('/') + 1) : null;

      if (!currentParent || currentParent !== parentOrl) {
        const items = await listResources(parentOrl);
        // Sort numerically for better user experience
        peerImages = items
          .filter(isImageFile)
          .sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' }));
      }

      // 2. Robust index finding with aggressive normalization
      const targetPath = cleanFile;
      // Extract filename safely, handling potential query params in ODFI
      const targetName = cleanFile.split('?')[0].split('/').pop() || '';

      const normalize = (s: string) => {
        try {
          return decodeURIComponent(s).toLowerCase().replace(/\/$/, '').trim();
        } catch {
          return s.toLowerCase().replace(/\/$/, '').trim();
        }
      };

      const normalizedTarget = normalize(targetPath);
      const normalizedTargetName = normalize(targetName);

      let foundIndex = peerImages.findIndex((item) => {
        // a. Direct match (Fastest)
        if (item.path === targetPath) return true;
        if (item.name === targetName) return true;

        // b. Normalized path match (Handles encoding/trailing slash/case differences)
        if (normalize(item.path) === normalizedTarget) return true;

        // c. Normalized name match (Most robust for UI consistency)
        if (normalize(item.name) === normalizedTargetName) return true;

        return false;
      });

      // d. Last resort: match by splitting path and comparing tails
      if (foundIndex === -1 && targetName) {
        foundIndex = peerImages.findIndex((item) => {
          const itemName = item.path.split('?')[0].split('/').pop();
          return itemName === targetName || normalize(itemName || '') === normalizedTargetName;
        });
      }

      currentIndex = foundIndex;
      if (foundIndex === -1) {
        console.warn(`[Image Carousel] Failed to find current image in peer list.`, {
          targetPath,
          targetName,
          firstInList: peerImages[0]?.path
        });
      }
    } catch (e) {
      console.warn('Failed to load peer images for carousel', e);
    }
  }

  function navigateTo(index: number) {
    if (index < 0 || index >= peerImages.length) return;
    const nextItem = peerImages[index];
    const nextFile = nextItem.path;

    if (nextFile === file) return;

    // Update URL without reload
    const url = new URL(window.location.href);
    url.searchParams.set('file', nextFile);
    window.history.pushState({}, '', url.toString());

    handleReset();
    // Re-run init logic for the new file
    initFromUrl();
  }

  function handlePrev() {
    if (currentIndex > 0) navigateTo(currentIndex - 1);
  }

  function handleNext() {
    if (currentIndex < peerImages.length - 1) navigateTo(currentIndex + 1);
  }

  function handleZoomIn() {
    // Finer zoom: increase by 20%
    scale = Math.min(scale * 1.2, 20);
  }

  function handleZoomOut() {
    // Finer zoom: decrease by 20%
    scale = Math.max(scale / 1.2, 0.05);
  }

  function handleWheel(e: WheelEvent) {
    e.preventDefault();
    const delta = -e.deltaY;
    const factor = delta > 0 ? 1.1 : 1 / 1.1;

    const newScale = Math.min(Math.max(scale * factor, 0.05), 20);

    // TODO: If we want to zoom at cursor position, we need to adjust translation
    // For now, just zoom center
    scale = newScale;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'ArrowLeft') handlePrev();
    if (e.key === 'ArrowRight') handleNext();
  }

  function handleReset() {
    scale = 1;
    rotation = 0;
    translation = { x: 0, y: 0 };
  }

  function handleRotateCw() {
    rotation = (rotation + 90) % 360;
  }

  function handleRotateCcw() {
    rotation = (rotation - 90) % 360;
  }

  function handleMouseDown(e: MouseEvent) {
    if (e.button !== 0) return;
    isDragging = true;
    startPos = { x: e.clientX - translation.x, y: e.clientY - translation.y };
    e.preventDefault();
  }

  function handleMouseMove(e: MouseEvent) {
    if (!isDragging) return;
    translation = {
      x: e.clientX - startPos.x,
      y: e.clientY - startPos.y
    };
  }

  function handleMouseUp() {
    isDragging = false;
  }

  function downloadImage() {
    const link = document.createElement('a');
    link.href = imageUrl;
    link.download = fileName;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
  }

  function handleClose() {
    window.close();
  }

  function getThumbnailUrl(path: string) {
    const apiBase = getApiBase();
    const searchParams = new URLSearchParams({ sid, file: path });
    return `${apiBase}/view/raw?${searchParams.toString()}`;
  }

  let thumbnailsContainer: HTMLDivElement | null = $state(null);

  $effect(() => {
    if (thumbnailsContainer && currentIndex !== -1) {
      const container = thumbnailsContainer as HTMLDivElement;
      const activeThumb = container.children[currentIndex] as HTMLElement;
      if (activeThumb) {
        activeThumb.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'center' });
      }
    }
  });

  function scrollThumbnails(direction: 'left' | 'right') {
    if (!thumbnailsContainer) return;
    const scrollAmount = thumbnailsContainer.clientWidth * 0.8;
    thumbnailsContainer.scrollBy({
      left: direction === 'left' ? -scrollAmount : scrollAmount,
      behavior: 'smooth'
    });
  }
</script>

<svelte:window onmousemove={handleMouseMove} onmouseup={handleMouseUp} onkeydown={handleKeyDown} />

<div class="flex h-screen flex-col overflow-hidden bg-background text-foreground">
  <!-- Header -->
  <header
    class="flex h-14 shrink-0 items-center justify-between border-b border-border bg-background/95 px-4 backdrop-blur supports-backdrop-filter:bg-background/60"
  >
    <div class="flex items-center gap-4">
      <LogSeekLogo size="small" />
      <div class="h-4 w-px bg-border"></div>
      <span class="truncate text-sm font-medium opacity-80" title={file}>{fileName}</span>
      {#if peerImages.length > 0}
        <span class="rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] whitespace-nowrap opacity-60">
          {currentIndex + 1} / {peerImages.length}
        </span>
      {/if}
    </div>

    <div class="flex items-center gap-2">
      <ThemeToggle />
      <Button variant="ghost" size="icon" onclick={handleClose} title="Close Tab">
        <X class="h-4 w-4" />
      </Button>
    </div>
  </header>

  <!-- Toolbar -->
  <div class="z-10 flex items-center justify-center gap-2 border-b border-border bg-muted/30 p-2 shadow-sm">
    <div class="flex items-center gap-1">
      <Button
        variant="ghost"
        size="icon"
        onclick={handlePrev}
        disabled={currentIndex <= 0}
        title="Previous (Left Arrow)"
      >
        <ChevronLeft class="h-4 w-4" />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        onclick={handleNext}
        disabled={currentIndex >= peerImages.length - 1}
        title="Next (Right Arrow)"
      >
        <ChevronRight class="h-4 w-4" />
      </Button>
    </div>

    <div class="mx-2 h-4 w-px bg-border"></div>

    <Button variant="ghost" size="icon" onclick={handleZoomOut} title="Zoom Out">
      <ZoomOut class="h-4 w-4" />
    </Button>
    <span class="w-16 text-center font-mono text-sm text-muted-foreground">{Math.round(scale * 100)}%</span>
    <Button variant="ghost" size="icon" onclick={handleZoomIn} title="Zoom In">
      <ZoomIn class="h-4 w-4" />
    </Button>

    <div class="mx-2 h-4 w-px bg-border"></div>

    <Button variant="ghost" size="icon" onclick={handleRotateCcw} title="Rotate Left">
      <RotateCcw class="h-4 w-4" />
    </Button>
    <Button variant="ghost" size="icon" onclick={handleRotateCw} title="Rotate Right">
      <RotateCw class="h-4 w-4" />
    </Button>

    <div class="mx-2 h-4 w-px bg-border"></div>

    <Button variant="ghost" size="icon" onclick={handleReset} title="Reset View">
      <Maximize class="h-4 w-4" />
    </Button>

    <div class="mx-2 h-4 w-px bg-border"></div>

    <Button variant="outline" size="sm" onclick={downloadImage} class="gap-2">
      <Download class="h-3.5 w-3.5" />
      <span class="hidden sm:inline">Download</span>
    </Button>
  </div>

  <!-- Main View -->
  <div class="relative flex-1 overflow-hidden bg-muted/50">
    <!-- Prev button overlay -->
    <div class="absolute inset-y-0 left-0 z-30 flex w-24 items-center justify-center transition-opacity">
      <Button
        variant="secondary"
        size="icon"
        class="h-16 w-16 rounded-full bg-background/10 text-foreground opacity-30 shadow-2xl backdrop-blur-md transition-all hover:scale-110 hover:bg-background/80 hover:opacity-100 active:scale-95 disabled:hidden"
        onclick={handlePrev}
        disabled={currentIndex <= 0}
      >
        <ChevronLeft class="h-10 w-10" />
      </Button>
    </div>

    <!-- Next button overlay -->
    <div class="absolute inset-y-0 right-0 z-30 flex w-24 items-center justify-center transition-opacity">
      <Button
        variant="secondary"
        size="icon"
        class="h-16 w-16 rounded-full bg-background/10 text-foreground opacity-30 shadow-2xl backdrop-blur-md transition-all hover:scale-110 hover:bg-background/80 hover:opacity-100 active:scale-95 disabled:hidden"
        onclick={handleNext}
        disabled={currentIndex >= peerImages.length - 1}
      >
        <ChevronRight class="h-10 w-10" />
      </Button>
    </div>

    <div class="flex h-full w-full items-center justify-center overflow-hidden">
      {#if loading && !error}
        <div class="flex flex-col items-center gap-2 text-muted-foreground">
          <div class="h-8 w-8 animate-spin rounded-full border-2 border-primary border-t-transparent"></div>
          <span class="text-sm">Loading image...</span>
        </div>
      {/if}

      {#if error}
        <div class="flex flex-col items-center gap-2 p-4 text-center text-destructive">
          <span class="text-lg font-semibold">无法加载图片</span>
          <span class="text-sm opacity-80">{error}</span>
        </div>
      {/if}

      {#if imageUrl && !error}
        <div
          class="relative flex h-full w-full cursor-grab items-center justify-center overflow-hidden active:cursor-grabbing"
          onmousedown={handleMouseDown}
          onwheel={handleWheel}
          role="presentation"
        >
          <!-- Checkerboard background for transparency -->
          <div
            class="pointer-events-none absolute inset-0 z-0 opacity-10"
            style="background-image: radial-gradient(#808080 1px, transparent 1px); background-size: 16px 16px;"
          ></div>

          <img
            bind:this={imgElement}
            src={imageUrl}
            alt={fileName}
            class="z-10 max-h-[90%] max-w-[90%] rounded-sm shadow-2xl transition-transform duration-75 ease-out select-none"
            style:transform={`translate(${translation.x}px, ${translation.y}px) rotate(${rotation}deg) scale(${scale})`}
            onload={() => (loading = false)}
            onerror={() => {
              loading = false;
              error = '加载图片资源失败';
            }}
            draggable="false"
          />
        </div>
      {/if}
    </div>
  </div>

  <!-- Thumbnails Carousel -->
  {#if peerImages.length > 1}
    <div class="group relative z-20 border-t border-border bg-background/50 backdrop-blur-sm">
      <!-- Left scroll button -->
      <button
        class="absolute inset-y-0 left-0 z-30 flex w-12 items-center justify-center bg-linear-to-r from-background/80 to-transparent text-foreground opacity-0 transition-opacity hover:opacity-100"
        onclick={() => scrollThumbnails('left')}
        title="Scroll Left"
      >
        <ChevronLeft class="h-8 w-8" />
      </button>

      <div
        bind:this={thumbnailsContainer}
        class="no-scrollbar flex h-32 items-center gap-4 overflow-x-auto overflow-y-hidden px-4 py-3"
        style="scrollbar-width: none; -ms-overflow-style: none;"
      >
        {#each peerImages as item, i (item.path)}
          <button
            class="relative h-24 w-40 shrink-0 overflow-hidden rounded-lg border-2 transition-all hover:scale-105 active:scale-95 {i ===
            currentIndex
              ? 'scale-105 border-primary ring-2 ring-primary/20'
              : 'border-transparent opacity-60 hover:opacity-100'}"
            onclick={() => navigateTo(i)}
            title={item.name}
          >
            <img src={getThumbnailUrl(item.path)} alt={item.name} class="h-full w-full object-cover" loading="lazy" />
            {#if i === currentIndex}
              <div class="absolute inset-0 bg-primary/10"></div>
            {/if}
          </button>
        {/each}
      </div>

      <!-- Right scroll button -->
      <button
        class="absolute inset-y-0 right-0 z-30 flex w-12 items-center justify-center bg-linear-to-l from-background/80 to-transparent text-foreground opacity-0 transition-opacity hover:opacity-100"
        onclick={() => scrollThumbnails('right')}
        title="Scroll Right"
      >
        <ChevronRight class="h-8 w-8" />
      </button>
    </div>
  {/if}

  <style>
    .no-scrollbar::-webkit-scrollbar {
      display: none;
    }
  </style>

  <!-- Footer Info -->
  <div
    class="flex h-6 items-center justify-between border-t border-border bg-muted/30 px-4 font-mono text-[10px] text-muted-foreground uppercase"
  >
    <div class="flex gap-4">
      <div>Pos: {translation.x.toFixed(0)}x, {translation.y.toFixed(0)}y</div>
      <div>Zoom: {(scale * 100).toFixed(0)}%</div>
      {#if imgElement}
        <div>Size: {imgElement.naturalWidth} x {imgElement.naturalHeight}</div>
      {/if}
    </div>
    <div class="hidden sm:block">LOGSEEK IMAGE VIEWER</div>
  </div>
</div>
