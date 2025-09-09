<script lang="ts">
  import { onMount } from 'svelte';

  // 主题状态，可取 'light' 或 'dark'
  let theme = $state<'light' | 'dark'>('light');

  // 应用主题到 <html> 并持久化到 localStorage
  function applyTheme(t: 'light' | 'dark') {
    const root = document.documentElement;
    if (t === 'dark') root.classList.add('dark');
    else root.classList.remove('dark');
    try {
      localStorage.setItem('theme', t);
    } catch (_) {
      // 在某些环境（如隐私模式）无法访问 localStorage，忽略错误
    }
  }

  // 切换主题
  function toggleTheme() {
    theme = theme === 'dark' ? 'light' : 'dark';
    applyTheme(theme);
  }

  // 客户端挂载后初始化主题
  onMount(() => {
    try {
      const saved = localStorage.getItem('theme') as 'light' | 'dark' | null;
      const initial = saved ?? (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light');
      theme = initial;
      applyTheme(initial);
    } catch (_) {
      // 忽略本地存储相关错误
    }
  });
</script>

<!-- 固定位置的主题切换按钮 -->
<button
  type="button"
  aria-label={theme === 'dark' ? '切换到浅色' : '切换到深色'}
  class="fixed top-3 right-3 inline-flex h-9 w-9 items-center justify-center rounded-full bg-white/80 text-gray-900 shadow-sm backdrop-blur select-none hover:bg-white dark:bg-gray-800/80 dark:text-gray-100 dark:hover:bg-gray-800"
  onclick={toggleTheme}
>
  {#if theme === 'dark'}
    <!-- 月亮图标（当前为深色） -->
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5" class="h-5 w-5">
      <path fill="none" d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
    </svg>
  {:else}
    <!-- 太阳图标（当前为浅色） -->
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5" class="h-5 w-5">
      <circle fill="none" cx="12" cy="12" r="4" />
      <path d="M12 2v2m0 16v2M4.93 4.93l1.41 1.41m11.32 11.32l1.41 1.41M2 12h2m16 0h2M4.93 19.07l1.41-1.41m11.32-11.32l1.41-1.41" />
    </svg>
  {/if}
</button>

