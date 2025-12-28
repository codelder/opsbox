<script lang="ts">
  import { onMount } from 'svelte';
  import { getApiBase } from '$lib/modules/logseek/api/config';
  import { Button } from '$lib/components/ui/button';
  import LogSeekLogo from '$lib/components/LogSeekLogo.svelte';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import { ZoomIn, ZoomOut, RotateCw, Download, Maximize, RotateCcw, X } from 'lucide-svelte';

  let sid = $state('');
  let file = $state('');
  let imageUrl = $state('');
  let fileName = $state('');

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
    const params = new URL(window.location.href).searchParams;
    file = params.get('file') || '';
    sid = params.get('sid') || '';

    if (!file) {
      error = 'Invalid parameters: missing file';
      loading = false;
      return;
    }

    // Extract filename from path
    fileName = file.split('/').pop() || 'Image';

    const apiBase = getApiBase();
    if (sid) {
      const searchParams = new URLSearchParams({ sid, file });
      imageUrl = `${apiBase}/view/raw?${searchParams.toString()}`;
    } else {
      // Fallback for cases without session? Or just try directly if public
      // Likely needs session for auth
      error = 'Invalid parameters: missing sid';
      loading = false;
    }
  });

  function handleZoomIn() {
    scale = Math.min(scale + 0.5, 5);
  }

  function handleZoomOut() {
    scale = Math.max(scale - 0.5, 0.1);
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
    // Only allow left click drag
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
</script>

<svelte:window onmousemove={handleMouseMove} onmouseup={handleMouseUp} />

<div class="flex h-screen flex-col overflow-hidden bg-background text-foreground">
  <!-- Header -->
  <header
    class="flex h-14 shrink-0 items-center justify-between border-b border-border bg-background/95 px-4 backdrop-blur supports-backdrop-filter:bg-background/60"
  >
    <div class="flex items-center gap-4">
      <LogSeekLogo size="small" />
      <div class="h-4 w-px bg-border"></div>
      <span class="truncate text-sm font-medium opacity-80" title={file}>{fileName}</span>
    </div>

    <div class="flex items-center gap-2">
      <ThemeToggle />
      <!-- Typically this opens in a new tab, so closing the window works. -->
      <Button variant="ghost" size="icon" onclick={handleClose} title="Close Tab">
        <X class="h-4 w-4" />
      </Button>
    </div>
  </header>

  <!-- Toolbar -->
  <div class="z-10 flex items-center justify-center gap-2 border-b border-border bg-muted/30 p-2 shadow-sm">
    <Button variant="ghost" size="icon" onclick={handleZoomOut} title="Zoom Out">
      <ZoomOut class="h-4 w-4" />
    </Button>
    <span class="w-12 text-center font-mono text-sm text-muted-foreground">{Math.round(scale * 100)}%</span>
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
    <div class="flex h-full w-full items-center justify-center overflow-hidden">
      {#if loading && !error}
        <div class="flex flex-col items-center gap-2 text-muted-foreground">
          <div class="h-8 w-8 animate-spin rounded-full border-2 border-primary border-t-transparent"></div>
          <span class="text-sm">Loading image...</span>
        </div>
      {/if}

      {#if error}
        <div class="flex flex-col items-center gap-2 text-destructive">
          <span class="text-lg font-semibold">Unable to load image</span>
          <span class="text-sm opacity-80">{error}</span>
        </div>
      {/if}

      {#if imageUrl && !error}
        <div
          class="relative flex h-full w-full cursor-grab items-center justify-center overflow-hidden active:cursor-grabbing"
          onmousedown={handleMouseDown}
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
              error = 'Failed to load image resource';
            }}
            draggable="false"
          />
        </div>
      {/if}
    </div>
  </div>

  <!-- Footer Info -->
  <div
    class="flex h-6 items-center justify-between border-t border-border bg-muted/30 px-4 font-mono text-[10px] text-muted-foreground"
  >
    <div>{translation.x.toFixed(0)}x, {translation.y.toFixed(0)}y</div>
    <div>LOGSEEK IMAGE VIEWER</div>
  </div>
</div>
