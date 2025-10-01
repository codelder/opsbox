<script lang="ts">
  /**
   * MinIO 设置页面（重构版）
   * 使用 LogSeek 模块的 composables 和 API 客户端
   */
  import { invalidate } from '$app/navigation';
  import { useSettings } from '$lib/modules/logseek';
  import Alert from '$lib/components/Alert.svelte';
  import SettingsInput from './SettingsInput.svelte';

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
      // 强制完整页面刷新，以重新检查配置状态
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
      <Alert type="warning" message={settings.loadError} />
    {/if}

    {#if settings.saveError}
      <Alert type="error" message={settings.saveError} />
    {/if}

    {#if settings.connectionError && settings.connectionError !== settings.saveError}
      <Alert type="error" message={settings.connectionError} />
    {/if}

    {#if settings.saveSuccess}
      <Alert type="success" message="设置已保存" />
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
          <SettingsInput
            label="Endpoint"
            description="填写 MinIO 服务的完整访问地址，通常包含协议与端口。"
            placeholder="http://host:9000"
            bind:value={settings.endpoint}
            disabled={settings.loadingSettings && !settings.loadedOnce}
          />

          <SettingsInput
            label="Bucket"
            description="设置默认的对象存储桶名称，用于读写日志归档。"
            placeholder="bucket"
            bind:value={settings.bucket}
            disabled={settings.loadingSettings && !settings.loadedOnce}
          />

          <SettingsInput
            label="Access Key"
            description="使用具有访问权限的 Access Key，可在 MinIO 控制台创建或轮换。"
            placeholder="access key"
            bind:value={settings.accessKey}
            autocomplete="off"
            disabled={settings.loadingSettings && !settings.loadedOnce}
          />

          <SettingsInput
            label="Secret Key"
            description="输入与 Access Key 对应的密钥，建议存储于安全的凭证管理器。"
            placeholder="secret key"
            type="password"
            bind:value={settings.secretKey}
            autocomplete="off"
            disabled={settings.loadingSettings && !settings.loadedOnce}
          />

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
