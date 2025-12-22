<script lang="ts">
  import '../app.css';
  import { ModeWatcher } from 'mode-watcher';
  import favicon from '$lib/assets/favicon.svg';

  let { children } = $props();
</script>

<svelte:head>
  <link href={favicon} rel="icon" />

  <link rel="preload" href="/fonts/GoogleSansCode-Italic.woff2" as="font" type="font/woff2" crossorigin="anonymous" />
</svelte:head>

<ModeWatcher />

<header
  class="fixed top-0 right-0 left-0 z-40 flex items-center justify-between border-b bg-background/80 px-6 py-3 backdrop-blur-sm transition-all duration-200"
>
  <div class="flex items-center gap-6">
    <a href="/" class="flex items-center gap-2 text-lg font-bold">
      <div class="flex h-6 w-6 items-center justify-center rounded bg-primary/20">
        <div class="h-3 w-3 rounded-full bg-primary"></div>
      </div>
      OpsBoard
    </a>
    <nav class="flex gap-1 text-sm font-medium">
      <a href="/" class="rounded-md px-4 py-2 transition-colors hover:bg-muted/50 hover:text-primary">Search</a>
      <a
        href="/explorer"
        class="rounded-md bg-muted/20 px-4 py-2 text-primary transition-colors hover:bg-muted/50 hover:text-primary"
        >Explorer</a
      >
    </nav>
  </div>
</header>

<div class="min-h-screen bg-background pt-16 text-foreground transition-colors duration-200 ease-in-out">
  {@render children?.()}
</div>
