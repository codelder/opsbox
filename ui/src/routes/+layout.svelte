<script lang="ts">
  import '../app.css';
  import favicon from '$lib/assets/favicon.svg';

  let { children } = $props();
</script>

<svelte:head>
  <link href={favicon} rel="icon" />

  <link rel="preload" href="/fonts/GoogleSansCode.woff2" as="font" type="font/woff2" crossorigin="anonymous" />
  <link rel="preload" href="/fonts/GoogleSansCode-Italic.woff2" as="font" type="font/woff2" crossorigin="anonymous" />
</svelte:head>

{@render children?.()}
