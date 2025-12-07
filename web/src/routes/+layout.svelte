<script lang="ts">
  import '../app.css';
  import { ModeWatcher } from 'mode-watcher';
  import favicon from '$lib/assets/favicon.svg';

  let { children } = $props();
</script>

<svelte:head>
  <link href={favicon} rel="icon" />

  <link rel="preload" href="/fonts/GoogleSansCode.woff2" as="font" type="font/woff2" crossorigin="anonymous" />
  <link rel="preload" href="/fonts/GoogleSansCode-Italic.woff2" as="font" type="font/woff2" crossorigin="anonymous" />
</svelte:head>

<ModeWatcher />

<div class="min-h-screen bg-base text-content transition-colors duration-200 ease-in-out">
  {@render children?.()}
</div>
