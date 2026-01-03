<script lang="ts">
  import '../app.css';
  import { ModeWatcher } from 'mode-watcher';
  import favicon from '$lib/assets/favicon.svg';
  import { browser } from '$app/environment';
  import { onMount } from 'svelte';

  let { children } = $props();

  // 注册 Service Worker 以缓存字体文件
  onMount(() => {
    if (browser && 'serviceWorker' in navigator) {
      navigator.serviceWorker.register('/sw.js').catch((err) => {
        console.warn('[SW] 注册失败:', err);
      });
    }
  });
</script>

<svelte:head>
  <link href={favicon} rel="icon" />
</svelte:head>

<ModeWatcher />

<div class="min-h-screen bg-background text-foreground transition-colors duration-200 ease-in-out">
  {@render children?.()}
</div>
