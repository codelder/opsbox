<script lang="ts">
  import { onMount } from 'svelte';
  import '../app.css';
  import favicon from '$lib/assets/favicon.svg';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import { env } from '$env/dynamic/public';

  let { children } = $props();

  const API_BASE = env.PUBLIC_API_BASE || '/api/v1/logsearch';

  let showSettings = $state(false);
  let endpoint = $state('');
  let bucket = $state('');
  let accessKey = $state('');
  let secretKey = $state('');
  let loadingSettings = $state(false);
  let loadError: string | null = $state(null);
  let saving = $state(false);
  let saveError: string | null = $state(null);
  let saveSuccess = $state(false);
  let loadedOnce = $state(false);

  async function fetchSettings(force = false) {
    if (loadingSettings || (loadedOnce && !force)) return;
    loadingSettings = true;
    loadError = null;
    try {
      const res = await fetch(`${API_BASE}/settings/minio`, {
        headers: { Accept: 'application/json' }
      });
      if (!res.ok) {
        throw new Error(`加载失败：${res.status}`);
      }
      const data = (await res.json()) as {
        endpoint?: string;
        bucket?: string;
        access_key?: string;
        secret_key?: string;
      };
      endpoint = data.endpoint ?? '';
      bucket = data.bucket ?? '';
      accessKey = data.access_key ?? '';
      secretKey = data.secret_key ?? '';
      loadedOnce = true;
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      loadError = err.message ?? '无法读取设置';
    } finally {
      loadingSettings = false;
    }
  }

  onMount(() => {
    fetchSettings();
  });

  function openSettings() {
    showSettings = true;
    saveSuccess = false;
    saveError = null;
    fetchSettings();
  }

  function closeSettings() {
    showSettings = false;
  }

  async function handleSave(event: Event) {
    event.preventDefault();
    if (saving) return;
    saving = true;
    saveError = null;
    saveSuccess = false;
    try {
      const res = await fetch(`${API_BASE}/settings/minio`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Accept: 'application/json'
        },
        body: JSON.stringify({
          endpoint,
          bucket,
          access_key: accessKey,
          secret_key: secretKey
        })
      });
      if (!res.ok) {
        throw new Error(`保存失败：${res.status}`);
      }
      saveSuccess = true;
      loadedOnce = true;
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      saveError = err.message ?? '保存设置失败';
    } finally {
      saving = false;
    }
  }
</script>

<svelte:head>
  <link href={favicon} rel="icon" />

  <link rel="preload" href="/fonts/GoogleSansCode.woff2" as="font" type="font/woff2" crossorigin="anonymous" />
  <link rel="preload" href="/fonts/GoogleSansCode-Italic.woff2" as="font" type="font/woff2" crossorigin="anonymous" />
</svelte:head>

<div
  class="min-h-screen bg-white text-gray-900 transition-colors duration-200 ease-in-out dark:bg-gray-900 dark:text-gray-100"
>
  <button
    type="button"
    class="fixed top-3 left-3 z-50 inline-flex h-9 w-9 items-center justify-center rounded-full bg-white/80 text-gray-900 shadow-sm backdrop-blur hover:bg-white focus:outline-none focus:ring-2 focus:ring-blue-500 dark:bg-gray-800/80 dark:text-gray-100 dark:hover:bg-gray-800"
    aria-label="打开设置"
    on:click={openSettings}
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
  </button>

  <ThemeToggle />

  <div
    class={`fixed inset-0 z-40 bg-black/40 transition-opacity duration-200 ease-in-out ${
      showSettings ? 'opacity-100 pointer-events-auto' : 'pointer-events-none opacity-0'
    }`}
    on:click={closeSettings}
  />

  <aside
    class={`fixed inset-y-0 left-0 z-50 w-full max-w-md transform bg-white shadow-xl transition-transform duration-300 ease-in-out dark:bg-gray-900 ${
      showSettings ? 'translate-x-0' : '-translate-x-full'
    }`}
    on:click={(event) => event.stopPropagation()}
  >
    <form class="flex h-full flex-col" on:submit={handleSave}>
      <div class="flex items-center justify-between border-b border-gray-200 px-5 py-4 dark:border-gray-700">
        <div>
          <h2 class="text-base font-semibold text-gray-800 dark:text-gray-100">MinIO 设置</h2>
          <p class="text-xs text-gray-500 dark:text-gray-400">修改后将用于 S3 检索</p>
        </div>
        <button
          type="button"
          class="inline-flex h-8 w-8 items-center justify-center rounded-full text-gray-500 hover:bg-gray-100 hover:text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:text-gray-400 dark:hover:bg-gray-800"
          aria-label="关闭"
          on:click={closeSettings}
        >
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="h-4 w-4">
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18 18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <div class="flex-1 space-y-4 overflow-y-auto px-5 py-4">
        {#if loadError}
          <div class="rounded border border-amber-300 bg-amber-50 px-3 py-2 text-xs text-amber-700 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-300">
            {loadError}
          </div>
        {/if}
        {#if saveError}
          <div class="rounded border border-red-300 bg-red-50 px-3 py-2 text-xs text-red-700 dark:border-red-800 dark:bg-red-950 dark:text-red-300">
            {saveError}
          </div>
        {/if}
        {#if saveSuccess}
          <div class="rounded border border-green-300 bg-green-50 px-3 py-2 text-xs text-green-700 dark:border-green-800 dark:bg-green-950 dark:text-green-300">
            设置已保存
          </div>
        {/if}

        <label class="block text-sm font-medium text-gray-700 dark:text-gray-200">
          Endpoint
          <input
            class="mt-1 w-full rounded border border-gray-300 bg-white px-3 py-2 text-sm text-gray-800 shadow-sm focus:border-blue-500 focus:outline-none focus:ring-2 focus:ring-blue-200 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-100"
            placeholder="http://host:9000"
            bind:value={endpoint}
          />
        </label>

        <label class="block text-sm font-medium text-gray-700 dark:text-gray-200">
          Bucket
          <input
            class="mt-1 w-full rounded border border-gray-300 bg-white px-3 py-2 text-sm text-gray-800 shadow-sm focus:border-blue-500 focus:outline-none focus:ring-2 focus:ring-blue-200 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-100"
            placeholder="bucket"
            bind:value={bucket}
          />
        </label>

        <label class="block text-sm font-medium text-gray-700 dark:text-gray-200">
          Access Key
          <input
            class="mt-1 w-full rounded border border-gray-300 bg-white px-3 py-2 text-sm text-gray-800 shadow-sm focus:border-blue-500 focus:outline-none focus:ring-2 focus:ring-blue-200 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-100"
            placeholder="access key"
            bind:value={accessKey}
            autocomplete="off"
          />
        </label>

        <label class="block text-sm font-medium text-gray-700 dark:text-gray-200">
          Secret Key
          <input
            class="mt-1 w-full rounded border border-gray-300 bg-white px-3 py-2 text-sm text-gray-800 shadow-sm focus:border-blue-500 focus:outline-none focus:ring-2 focus:ring-blue-200 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-100"
            placeholder="secret key"
            type="password"
            bind:value={secretKey}
            autocomplete="off"
          />
        </label>
      </div>

      <div class="border-t border-gray-200 px-5 py-4 dark:border-gray-700">
        <button
          type="submit"
          class="inline-flex w-full items-center justify-center rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white transition hover:bg-blue-700 disabled:cursor-not-allowed disabled:bg-blue-300"
          disabled={
            saving ||
            !endpoint.trim() ||
            !bucket.trim() ||
            !accessKey.trim() ||
            !secretKey.trim()
          }
        >
          {#if saving}
            保存中…
          {:else}
            保存
          {/if}
        </button>
      </div>
    </form>
  </aside>

  {@render children?.()}
</div>
