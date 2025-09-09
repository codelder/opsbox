<script lang="ts">
  import '../app.css';
  import favicon from '$lib/assets/favicon.svg';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';

  let { children } = $props();
</script>

<svelte:head>
  <link href={favicon} rel="icon" />

  <link rel="preload" href="/fonts/GoogleSansCode.woff2" as="font" type="font/woff2" crossorigin="anonymous" />
  <link rel="preload" href="/fonts/GoogleSansCode-Italic.woff2" as="font" type="font/woff2" crossorigin="anonymous" />
</svelte:head>

<div
  class="min-h-screen bg-white text-gray-900 transition-colors duration-200 ease-in-out dark:bg-gray-900 dark:text-gray-100"
>
  <ThemeToggle />

  {@render children?.()}
</div>
