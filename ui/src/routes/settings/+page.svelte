<script lang="ts">
  /**
   * MinIO 设置页面（重构版）
   * 使用 LogSeek 模块的 composables 和 API 客户端
   */
  import { invalidate } from '$app/navigation';
  import { useSettings } from '$lib/modules/logseek';

  // 使用 composable 管理状态和方法
  const settings = useSettings();

  // 初始化设置加载
  let settingsInit = $state(false);
  $effect(() => {
    if (settingsInit) return;
    settingsInit = true;
    settings.loadSettings();
  });

  // 保存设置并跳转
  async function handleSave(event: Event) {
    event.preventDefault();
    await settings.save();
    if (settings.saveSuccess) {
      await invalidate('/api/v1/logseek/settings/minio');
      window.location.href = '/';
    }
  }

  function handleCancel() {
    if (history.length > 1) {
      history.back();
    } else {
      window.location.href = '/';
    }
  }
</script>

<svelte:head>
  <title>MinIO 设置 · Opsboard</title>
</svelte:head>

<div class="mx-auto flex max-w-5xl flex-col gap-6 px-6 pb-16 text-slate-900 dark:text-slate-100">
  <header class="pt-6">
    <p class="text-xs font-semibold tracking-[0.2em] text-slate-500 uppercase dark:text-slate-400">Storage</p>
    <h1 class="mt-2 text-2xl font-semibold text-slate-900 dark:text-slate-50">MinIO 设置</h1>
    <p class="mt-2 text-sm text-slate-500 dark:text-slate-400">配置日志检索所需的对象存储连接和凭证。</p>
  </header>

  <nav
    class="flex items-center gap-6 border-b border-slate-200 pb-3 text-sm font-medium text-slate-500 dark:border-slate-800 dark:text-slate-400"
  >
    <span class="rounded-full bg-white px-3 py-1 text-slate-900 shadow-sm dark:bg-slate-900 dark:text-slate-100"
      >存储设置</span
    >
    <span class="px-3 py-1">告警</span>
    <span class="px-3 py-1">通知</span>
    <span class="px-3 py-1">团队</span>
  </nav>

  <form class="space-y-6" onsubmit={handleSave}>
    {#if settings.loadingSettings && !settings.loadedOnce}
      <div
        class="rounded-xl border border-dashed border-slate-200 bg-white/40 px-4 py-3 text-sm text-slate-500 dark:border-slate-800 dark:bg-slate-900/40 dark:text-slate-400"
      >
        正在加载设置…
      </div>
    {/if}

    {#if settings.loadError}
      <div
        class="flex items-start gap-3 rounded-xl border border-amber-300 bg-amber-50 px-4 py-3 text-sm text-amber-700 shadow-sm dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          stroke="currentColor"
          stroke-width="1.5"
          class="mt-0.5 h-5 w-5"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M12 9v4m0 4h.01m-.01-14a9 9 0 1 1 0 18 9 9 0 0 1 0-18z"
          />
        </svg>
        <span>{settings.loadError}</span>
      </div>
    {/if}

    {#if settings.saveError}
      <div
        class="flex items-start gap-3 rounded-xl border border-red-300 bg-red-50 px-4 py-3 text-sm text-red-700 shadow-sm dark:border-red-800 dark:bg-red-950 dark:text-red-200"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          stroke="currentColor"
          stroke-width="1.5"
          class="mt-0.5 h-5 w-5"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M12 9v4m0 4h.01m-.01-14a9 9 0 1 1 0 18 9 9 0 0 1 0-18z"
          />
        </svg>
        <span>{settings.saveError}</span>
      </div>
    {/if}

    {#if settings.connectionError && settings.connectionError !== settings.saveError}
      <div
        class="flex items-start gap-3 rounded-xl border border-red-300 bg-red-50 px-4 py-3 text-sm text-red-700 shadow-sm dark:border-red-800 dark:bg-red-950 dark:text-red-200"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          stroke="currentColor"
          stroke-width="1.5"
          class="mt-0.5 h-5 w-5"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M12 9v4m0 4h.01m-.01-14a9 9 0 1 1 0 18 9 9 0 0 1 0-18z"
          />
        </svg>
        <span>{settings.connectionError}</span>
      </div>
    {/if}

    {#if settings.saveSuccess}
      <div
        class="flex items-start gap-3 rounded-xl border border-emerald-300 bg-emerald-50 px-4 py-3 text-sm text-emerald-700 shadow-sm dark:border-emerald-800 dark:bg-emerald-950 dark:text-emerald-200"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          stroke="currentColor"
          stroke-width="1.5"
          class="mt-0.5 h-5 w-5"
        >
          <path stroke-linecap="round" stroke-linejoin="round" d="m5 13 4 4L19 7" />
        </svg>
        <span>设置已保存</span>
      </div>
    {/if}

    <section
      class="rounded-3xl border border-slate-200 bg-white shadow-lg shadow-slate-200/40 dark:border-slate-800 dark:bg-slate-900 dark:shadow-black/30"
    >
      <div class="flex flex-col gap-8 p-6 lg:flex-row lg:p-8">
        <div class="space-y-3 lg:w-64">
          <h2 class="text-lg font-semibold text-slate-900 dark:text-slate-100">连接信息</h2>
          <p class="text-sm leading-relaxed text-slate-500 dark:text-slate-400">
            指定 MinIO 服务的基础地址和默认存储桶，用于日志索引与检索。
          </p>
        </div>

        <div class="flex-1 space-y-8">
          <label
            class="flex flex-col gap-3 rounded-2xl border border-transparent bg-white/60 p-4 text-sm text-slate-500 shadow-sm shadow-slate-200/40 transition hover:border-slate-200 hover:shadow-slate-200 dark:bg-slate-900/60 dark:text-slate-400 dark:hover:border-slate-700"
          >
            <span>
              <span class="text-xs font-semibold tracking-[0.2em] text-indigo-500 uppercase dark:text-indigo-400 block">
                Endpoint
              </span>
              <span class="text-sm leading-relaxed text-slate-600 dark:text-slate-300 block">
                填写 MinIO 服务的完整访问地址，通常包含协议与端口。
              </span>
            </span>
            <input
              class="w-full rounded-xl border border-slate-200 bg-white px-3 py-3 text-sm text-slate-900 shadow-inner shadow-slate-200 focus:border-indigo-500 focus:ring-4 focus:ring-indigo-100 focus:outline-none dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:shadow-none dark:focus:border-indigo-400 dark:focus:ring-indigo-500/30"
              placeholder="http://host:9000"
            bind:value={settings.endpoint}
            disabled={settings.loadingSettings && !settings.loadedOnce}
            />
          </label>

          <label
            class="flex flex-col gap-3 rounded-2xl border border-transparent bg-white/60 p-4 text-sm text-slate-500 shadow-sm shadow-slate-200/40 transition hover:border-slate-200 hover:shadow-slate-200 dark:bg-slate-900/60 dark:text-slate-400 dark:hover:border-slate-700"
          >
            <span>
              <span class="text-xs font-semibold tracking-[0.2em] text-indigo-500 uppercase dark:text-indigo-400 block">
                Bucket
              </span>
              <span class="text-sm leading-relaxed text-slate-600 dark:text-slate-300 block">
                设置默认的对象存储桶名称，用于读写日志归档。
              </span>
            </span>
            <input
              class="w-full rounded-xl border border-slate-200 bg-white px-3 py-3 text-sm text-slate-900 shadow-inner shadow-slate-200 focus:border-indigo-500 focus:ring-4 focus:ring-indigo-100 focus:outline-none dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:shadow-none dark:focus:border-indigo-400 dark:focus:ring-indigo-500/30"
              placeholder="bucket"
            bind:value={settings.bucket}
            disabled={settings.loadingSettings && !settings.loadedOnce}
            />
          </label>

          <label
            class="flex flex-col gap-3 rounded-2xl border border-transparent bg-white/60 p-4 text-sm text-slate-500 shadow-sm shadow-slate-200/40 transition hover:border-slate-200 hover:shadow-slate-200 dark:bg-slate-900/60 dark:text-slate-400 dark:hover:border-slate-700"
          >
            <span>
              <span class="text-xs font-semibold tracking-[0.2em] text-indigo-500 uppercase dark:text-indigo-400 block">
                Access Key
              </span>
              <span class="text-sm leading-relaxed text-slate-600 dark:text-slate-300 block">
                使用具有访问权限的 Access Key，可在 MinIO 控制台创建或轮换。
              </span>
            </span>
            <input
              class="w-full rounded-xl border border-slate-200 bg-white px-3 py-3 text-sm text-slate-900 shadow-inner shadow-slate-200 focus:border-indigo-500 focus:ring-4 focus:ring-indigo-100 focus:outline-none dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:shadow-none dark:focus:border-indigo-400 dark:focus:ring-indigo-500/30"
              placeholder="access key"
            bind:value={settings.accessKey}
            autocomplete="off"
            disabled={settings.loadingSettings && !settings.loadedOnce}
            />
          </label>

          <label
            class="flex flex-col gap-3 rounded-2xl border border-transparent bg-white/60 p-4 text-sm text-slate-500 shadow-sm shadow-slate-200/40 transition hover:border-slate-200 hover:shadow-slate-200 dark:bg-slate-900/60 dark:text-slate-400 dark:hover:border-slate-700"
          >
            <span>
              <span class="text-xs font-semibold tracking-[0.2em] text-indigo-500 uppercase dark:text-indigo-400 block">
                Secret Key
              </span>
              <span class="text-sm leading-relaxed text-slate-600 dark:text-slate-300 block">
                输入与 Access Key 对应的密钥，建议存储于安全的凭证管理器。
              </span>
            </span>
            <input
              class="w-full rounded-xl border border-slate-200 bg-white px-3 py-3 text-sm text-slate-900 shadow-inner shadow-slate-200 focus:border-indigo-500 focus:ring-4 focus:ring-indigo-100 focus:outline-none dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:shadow-none dark:focus:border-indigo-400 dark:focus:ring-indigo-500/30"
              placeholder="secret key"
              type="password"
            bind:value={settings.secretKey}
            autocomplete="off"
            disabled={settings.loadingSettings && !settings.loadedOnce}
            />
          </label>

          <div
            class="rounded-2xl border border-slate-100 bg-slate-50 px-4 py-3 text-sm text-slate-500 dark:border-slate-800 dark:bg-slate-900/80 dark:text-slate-400"
          >
            这些凭证会用于访问 MinIO 对象存储，请确保拥有读写权限。
          </div>
        </div>
      </div>

      <div
        class="flex flex-wrap items-center justify-end gap-3 border-t border-slate-200 bg-slate-100/70 px-6 py-5 dark:border-slate-800 dark:bg-slate-900/60"
      >
        <button
          type="button"
          class="inline-flex items-center rounded-xl border border-transparent px-4 py-2 text-sm font-medium text-slate-500 transition hover:text-slate-700 focus:ring-2 focus:ring-slate-300 focus:outline-none dark:text-slate-300 dark:hover:text-slate-100"
          onclick={handleCancel}
        >
          取消
        </button>

        <button
          type="submit"
          class="inline-flex items-center justify-center rounded-xl bg-indigo-600 px-5 py-2 text-sm font-semibold text-white shadow-sm transition hover:bg-indigo-500 focus:ring-4 focus:ring-indigo-200 focus:outline-none disabled:cursor-not-allowed disabled:bg-indigo-300 dark:bg-indigo-500 dark:hover:bg-indigo-400 dark:focus:ring-indigo-500/40"
          disabled={settings.saving ||
            (settings.loadingSettings && !settings.loadedOnce) ||
            !settings.endpoint.trim() ||
            !settings.bucket.trim() ||
            !settings.accessKey.trim() ||
            !settings.secretKey.trim()}
        >
          {#if settings.saving}
            保存中…
          {:else}
            保存设置
          {/if}
        </button>
      </div>
    </section>
  </form>
</div>
