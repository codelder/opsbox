<script lang="ts">
  import { onMount } from 'svelte';
  import { goto, invalidate } from '$app/navigation';
  import { env } from '$env/dynamic/public';

  const API_BASE = env.PUBLIC_API_BASE || '/api/v1/logsearch';

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
  let connectionError: string | null = $state(null);

  async function fetchSettings(force = false) {
    if (loadingSettings || (loadedOnce && !force)) return;
    loadingSettings = true;
    loadError = null;
    connectionError = null;
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
        connection_error?: string | null;
      };
      endpoint = data.endpoint ?? '';
      bucket = data.bucket ?? '';
      accessKey = data.access_key ?? '';
      secretKey = data.secret_key ?? '';
      connectionError = data.connection_error ?? null;
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

  async function handleSave(event: Event) {
    event.preventDefault();
    if (saving) return;
    saving = true;
    saveError = null;
    saveSuccess = false;
    connectionError = null;
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
        const defaultMessage = `保存失败：${res.status}`;
        let message = defaultMessage;
        try {
          const problem = await res.json();
          message = problem?.detail || problem?.title || defaultMessage;
        } catch (_jsonErr) {
          // ignore json parse error, keep default message
        }
        saveError = message;
        connectionError = message;
        return;
      }
      await fetchSettings(true);
      await invalidate('/api/v1/logsearch/settings/minio');
      connectionError = null;
      saveSuccess = true;
      await goto('/', { invalidateAll: true });
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      saveError = err.message ?? '保存设置失败';
    } finally {
      saving = false;
    }
  }

  function handleCancel() {
    if (history.length > 1) {
      history.back();
    } else {
      goto('/');
    }
  }
</script>

<svelte:head>
  <title>MinIO 设置 · Opsboard</title>
</svelte:head>

<div class="mx-auto flex max-w-5xl flex-col gap-6 px-6 pb-16 text-slate-900 dark:text-slate-100">
  <header class="pt-6">
    <p class="text-xs font-semibold uppercase tracking-[0.2em] text-slate-500 dark:text-slate-400">Storage</p>
    <h1 class="mt-2 text-2xl font-semibold text-slate-900 dark:text-slate-50">MinIO 设置</h1>
    <p class="mt-2 text-sm text-slate-500 dark:text-slate-400">
      配置日志检索所需的对象存储连接和凭证。
    </p>
  </header>

  <nav class="flex items-center gap-6 border-b border-slate-200 pb-3 text-sm font-medium text-slate-500 dark:border-slate-800 dark:text-slate-400">
    <span class="rounded-full bg-white px-3 py-1 text-slate-900 shadow-sm dark:bg-slate-900 dark:text-slate-100">存储设置</span>
    <span class="px-3 py-1">告警</span>
    <span class="px-3 py-1">通知</span>
    <span class="px-3 py-1">团队</span>
  </nav>

  <form class="space-y-6" onsubmit={handleSave}>
    {#if loadingSettings && !loadedOnce}
      <div class="rounded-xl border border-dashed border-slate-200 bg-white/40 px-4 py-3 text-sm text-slate-500 dark:border-slate-800 dark:bg-slate-900/40 dark:text-slate-400">
        正在加载设置…
      </div>
    {/if}

    {#if loadError}
      <div class="flex items-start gap-3 rounded-xl border border-amber-300 bg-amber-50 px-4 py-3 text-sm text-amber-700 shadow-sm dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="mt-0.5 h-5 w-5">
          <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v4m0 4h.01m-.01-14a9 9 0 1 1 0 18 9 9 0 0 1 0-18z" />
        </svg>
        <span>{loadError}</span>
      </div>
    {/if}

    {#if saveError}
      <div class="flex items-start gap-3 rounded-xl border border-red-300 bg-red-50 px-4 py-3 text-sm text-red-700 shadow-sm dark:border-red-800 dark:bg-red-950 dark:text-red-200">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="mt-0.5 h-5 w-5">
          <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v4m0 4h.01m-.01-14a9 9 0 1 1 0 18 9 9 0 0 1 0-18z" />
        </svg>
        <span>{saveError}</span>
      </div>
    {/if}

    {#if connectionError && connectionError !== saveError}
      <div class="flex items-start gap-3 rounded-xl border border-red-300 bg-red-50 px-4 py-3 text-sm text-red-700 shadow-sm dark:border-red-800 dark:bg-red-950 dark:text-red-200">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="mt-0.5 h-5 w-5">
          <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v4m0 4h.01m-.01-14a9 9 0 1 1 0 18 9 9 0 0 1 0-18z" />
        </svg>
        <span>{connectionError}</span>
      </div>
    {/if}

    {#if saveSuccess}
      <div class="flex items-start gap-3 rounded-xl border border-emerald-300 bg-emerald-50 px-4 py-3 text-sm text-emerald-700 shadow-sm dark:border-emerald-800 dark:bg-emerald-950 dark:text-emerald-200">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="mt-0.5 h-5 w-5">
          <path stroke-linecap="round" stroke-linejoin="round" d="m5 13 4 4L19 7" />
        </svg>
        <span>设置已保存</span>
      </div>
    {/if}

    <section class="rounded-3xl border border-slate-200 bg-white shadow-lg shadow-slate-200/40 dark:border-slate-800 dark:bg-slate-900 dark:shadow-black/30">
      <div class="flex flex-col gap-8 p-6 lg:flex-row lg:p-8">
        <div class="space-y-3 lg:w-64">
          <h2 class="text-lg font-semibold text-slate-900 dark:text-slate-100">连接信息</h2>
          <p class="text-sm leading-relaxed text-slate-500 dark:text-slate-400">
            指定 MinIO 服务的基础地址和默认存储桶，用于日志索引与检索。
          </p>
        </div>

        <div class="flex-1 space-y-8">
          <label class="flex flex-col gap-3 rounded-2xl border border-transparent bg-white/60 p-4 text-sm text-slate-500 shadow-sm shadow-slate-200/40 transition hover:border-slate-200 hover:shadow-slate-200 dark:bg-slate-900/60 dark:text-slate-400 dark:hover:border-slate-700">
            <div>
              <p class="text-xs font-semibold uppercase tracking-[0.2em] text-indigo-500 dark:text-indigo-400">Endpoint</p>
              <p class="text-sm leading-relaxed text-slate-600 dark:text-slate-300">填写 MinIO 服务的完整访问地址，通常包含协议与端口。</p>
            </div>
            <input
              class="w-full rounded-xl border border-slate-200 bg-white px-3 py-3 text-sm text-slate-900 shadow-inner shadow-slate-200 focus:border-indigo-500 focus:outline-none focus:ring-4 focus:ring-indigo-100 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:shadow-none dark:focus:border-indigo-400 dark:focus:ring-indigo-500/30"
              placeholder="http://host:9000"
              bind:value={endpoint}
              disabled={loadingSettings && !loadedOnce}
            />
          </label>

          <label class="flex flex-col gap-3 rounded-2xl border border-transparent bg-white/60 p-4 text-sm text-slate-500 shadow-sm shadow-slate-200/40 transition hover:border-slate-200 hover:shadow-slate-200 dark:bg-slate-900/60 dark:text-slate-400 dark:hover:border-slate-700">
            <div>
              <p class="text-xs font-semibold uppercase tracking-[0.2em] text-indigo-500 dark:text-indigo-400">Bucket</p>
              <p class="text-sm leading-relaxed text-slate-600 dark:text-slate-300">设置默认的对象存储桶名称，用于读写日志归档。</p>
            </div>
            <input
              class="w-full rounded-xl border border-slate-200 bg-white px-3 py-3 text-sm text-slate-900 shadow-inner shadow-slate-200 focus:border-indigo-500 focus:outline-none focus:ring-4 focus:ring-indigo-100 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:shadow-none dark:focus:border-indigo-400 dark:focus:ring-indigo-500/30"
              placeholder="bucket"
              bind:value={bucket}
              disabled={loadingSettings && !loadedOnce}
            />
          </label>

          <label class="flex flex-col gap-3 rounded-2xl border border-transparent bg-white/60 p-4 text-sm text-slate-500 shadow-sm shadow-slate-200/40 transition hover:border-slate-200 hover:shadow-slate-200 dark:bg-slate-900/60 dark:text-slate-400 dark:hover:border-slate-700">
            <div>
              <p class="text-xs font-semibold uppercase tracking-[0.2em] text-indigo-500 dark:text-indigo-400">Access Key</p>
              <p class="text-sm leading-relaxed text-slate-600 dark:text-slate-300">使用具有访问权限的 Access Key，可在 MinIO 控制台创建或轮换。</p>
            </div>
            <input
              class="w-full rounded-xl border border-slate-200 bg-white px-3 py-3 text-sm text-slate-900 shadow-inner shadow-slate-200 focus:border-indigo-500 focus:outline-none focus:ring-4 focus:ring-indigo-100 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:shadow-none dark:focus:border-indigo-400 dark:focus:ring-indigo-500/30"
              placeholder="access key"
              bind:value={accessKey}
              autocomplete="off"
              disabled={loadingSettings && !loadedOnce}
            />
          </label>

          <label class="flex flex-col gap-3 rounded-2xl border border-transparent bg-white/60 p-4 text-sm text-slate-500 shadow-sm shadow-slate-200/40 transition hover:border-slate-200 hover:shadow-slate-200 dark:bg-slate-900/60 dark:text-slate-400 dark:hover:border-slate-700">
            <div>
              <p class="text-xs font-semibold uppercase tracking-[0.2em] text-indigo-500 dark:text-indigo-400">Secret Key</p>
              <p class="text-sm leading-relaxed text-slate-600 dark:text-slate-300">输入与 Access Key 对应的密钥，建议存储于安全的凭证管理器。</p>
            </div>
            <input
              class="w-full rounded-xl border border-slate-200 bg-white px-3 py-3 text-sm text-slate-900 shadow-inner shadow-slate-200 focus:border-indigo-500 focus:outline-none focus:ring-4 focus:ring-indigo-100 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:shadow-none dark:focus:border-indigo-400 dark:focus:ring-indigo-500/30"
              placeholder="secret key"
              type="password"
              bind:value={secretKey}
              autocomplete="off"
              disabled={loadingSettings && !loadedOnce}
            />
          </label>

          <div class="rounded-2xl border border-slate-100 bg-slate-50 px-4 py-3 text-sm text-slate-500 dark:border-slate-800 dark:bg-slate-900/80 dark:text-slate-400">
            这些凭证会用于访问 MinIO 对象存储，请确保拥有读写权限。
          </div>
        </div>
      </div>

      <div class="flex flex-wrap items-center justify-end gap-3 border-t border-slate-200 bg-slate-100/70 px-6 py-5 dark:border-slate-800 dark:bg-slate-900/60">
        <button
          type="button"
          class="inline-flex items-center rounded-xl border border-transparent px-4 py-2 text-sm font-medium text-slate-500 transition hover:text-slate-700 focus:outline-none focus:ring-2 focus:ring-slate-300 dark:text-slate-300 dark:hover:text-slate-100"
          onclick={handleCancel}
        >
          取消
        </button>

        <button
          type="submit"
          class="inline-flex items-center justify-center rounded-xl bg-indigo-600 px-5 py-2 text-sm font-semibold text-white shadow-sm transition hover:bg-indigo-500 focus:outline-none focus:ring-4 focus:ring-indigo-200 disabled:cursor-not-allowed disabled:bg-indigo-300 dark:bg-indigo-500 dark:hover:bg-indigo-400 dark:focus:ring-indigo-500/40"
          disabled={
            saving ||
            loadingSettings && !loadedOnce ||
            !endpoint.trim() ||
            !bucket.trim() ||
            !accessKey.trim() ||
            !secretKey.trim()
          }
        >
          {#if saving}
            保存中…
          {:else}
            保存设置
          {/if}
        </button>
      </div>
    </section>
  </form>
</div>
