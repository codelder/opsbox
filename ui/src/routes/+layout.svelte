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
  <a
    href="/settings"
    class="fixed top-3 left-3 z-50 inline-flex h-9 w-9 items-center justify-center rounded-full bg-white/80 text-gray-900 shadow-sm backdrop-blur hover:bg-white focus:outline-none focus:ring-2 focus:ring-blue-500 dark:bg-gray-800/80 dark:text-gray-100 dark:hover:bg-gray-800"
    aria-label="打开设置"
  >
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      class="h-5 w-5"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 0 0 2.573 1.066c1.543-.89 3.31.876 2.42 2.42a1.724 1.724 0 0 0 1.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 0 0-1.066 2.573c.89 1.543-.876 3.31-2.42 2.42a1.724 1.724 0 0 0-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 0 0-2.573-1.066c-1.543.89-3.31-.876-2.42-2.42a1.724 1.724 0 0 0-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 0 0 1.066-2.573c-.89-1.543.876-3.31 2.42-2.42.996.575 2.245.021 2.572-1.065z"
      />
      <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0z" />
    </svg>
  </a>

  <ThemeToggle />
  <main class="pt-16">
    {@render children?.()}
  </main>
</div>
