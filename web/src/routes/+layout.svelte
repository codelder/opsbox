<script lang="ts">
  import '../app.css';
  import { ModeWatcher } from 'mode-watcher';
  import favicon from '$lib/assets/favicon.svg';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import Settings from '$lib/components/Settings.svelte';

  let { children } = $props();
</script>

<svelte:head>
  <link href={favicon} rel="icon" />

  <link rel="preload" href="/fonts/GoogleSansCode.woff2" as="font" type="font/woff2" crossorigin="anonymous" />
  <link rel="preload" href="/fonts/GoogleSansCode-Italic.woff2" as="font" type="font/woff2" crossorigin="anonymous" />
</svelte:head>

<ModeWatcher />

<div class="min-h-screen bg-base text-content transition-colors duration-200 ease-in-out">
  <div class="fixed top-3 left-3 z-50"><Settings /></div>
  <div class="fixed top-3 right-3 z-50"><ThemeToggle /></div>
  <main>
    {@render children?.()}
  </main>
</div>
