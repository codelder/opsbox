<script lang="ts">
  /**
   * Server 日志设置组件
   * 管理 Server 的日志级别和保留策略
   */
  import Alert from '$lib/components/Alert.svelte';
  import {
    fetchServerLogConfig,
    updateServerLogLevel,
    updateServerLogRetention,
    type LogConfigResponse
  } from '$lib/modules/agent/api';

  let config = $state<LogConfigResponse | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);

  // 初始化加载
  let inited = $state(false);
  $effect(() => {
    if (inited) return;
    inited = true;
    loadConfig();
  });

  async function loadConfig() {
    loading = true;
    error = null;
    try {
      config = await fetchServerLogConfig();
    } catch (e) {
      error = e instanceof Error ? e.message : '加载配置失败';
    } finally {
      loading = false;
    }
  }

  async function handleSave() {
    if (!config) return;
    loading = true;
    error = null;
    success = null;

    try {
      // 更新日志级别
      await updateServerLogLevel(config.level);

      // 更新保留数量
      await updateServerLogRetention(config.retention_count);

      success = '配置已保存';
      setTimeout(() => {
        success = null;
      }, 3000);
    } catch (e) {
      error = e instanceof Error ? e.message : '保存失败';
    } finally {
      loading = false;
    }
  }
</script>

<div class="space-y-6">
  {#if error}
    <Alert type="error" message={error} onClose={() => (error = null)} />
  {/if}

  {#if success}
    <Alert type="success" message={success} onClose={() => (success = null)} />
  {/if}

  <section class="rounded-3xl border border-[var(--border)] bg-[var(--surface)] p-6 shadow-lg shadow-black/5">
    <div class="mb-6">
      <h2 class="text-lg font-semibold text-[var(--text)]">Server 日志设置</h2>
      <p class="mt-1 text-sm text-[var(--muted)]">配置 Server 的日志级别和保留策略</p>
    </div>

    {#if loading && !config}
      <div class="py-10 text-center text-sm text-[var(--muted)]">加载中…</div>
    {:else if config}
      <div class="space-y-4">
        <!-- 日志级别 -->
        <label
          class="flex flex-col gap-3 rounded-2xl border border-transparent bg-white/60 p-4 text-sm text-slate-500 shadow-sm shadow-slate-200/40 transition hover:border-slate-200 hover:shadow-slate-200 dark:bg-slate-900/60 dark:text-slate-400 dark:hover:border-slate-700"
        >
          <span>
            <span
              class="block text-xs font-semibold tracking-[0.2em] text-indigo-500 uppercase dark:text-indigo-400"
            >
              日志级别
            </span>
            <span class="block text-sm leading-relaxed text-slate-600 dark:text-slate-300">
              控制日志输出的详细程度
            </span>
          </span>
          <select
            class="w-full rounded-xl border border-slate-200 bg-white px-3 py-3 text-sm text-slate-900 shadow-inner shadow-slate-200 focus:border-indigo-500 focus:ring-4 focus:ring-indigo-100 focus:outline-none dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:shadow-none dark:focus:border-indigo-400 dark:focus:ring-indigo-500/30"
            bind:value={config.level}
          >
            <option value="error">ERROR - 仅错误</option>
            <option value="warn">WARN - 警告及以上</option>
            <option value="info">INFO - 信息及以上（推荐）</option>
            <option value="debug">DEBUG - 调试及以上</option>
            <option value="trace">TRACE - 全部日志</option>
          </select>
        </label>

        <!-- 日志保留 -->
        <label
          class="flex flex-col gap-3 rounded-2xl border border-transparent bg-white/60 p-4 text-sm text-slate-500 shadow-sm shadow-slate-200/40 transition hover:border-slate-200 hover:shadow-slate-200 dark:bg-slate-900/60 dark:text-slate-400 dark:hover:border-slate-700"
        >
          <span>
            <span
              class="block text-xs font-semibold tracking-[0.2em] text-indigo-500 uppercase dark:text-indigo-400"
            >
              日志保留
            </span>
            <span class="block text-sm leading-relaxed text-slate-600 dark:text-slate-300">
              保留最近 N 天的日志文件
            </span>
          </span>
          <div class="flex items-center gap-2">
            <input
              type="number"
              class="w-full rounded-xl border border-slate-200 bg-white px-3 py-3 text-sm text-slate-900 shadow-inner shadow-slate-200 focus:border-indigo-500 focus:ring-4 focus:ring-indigo-100 focus:outline-none dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:shadow-none dark:focus:border-indigo-400 dark:focus:ring-indigo-500/30"
              bind:value={config.retention_count}
              min="1"
              max="365"
            />
            <span class="text-sm text-[var(--text)]">天</span>
          </div>
        </label>

        <!-- 日志路径（只读） -->
        <label
          class="flex flex-col gap-3 rounded-2xl border border-transparent bg-white/60 p-4 text-sm text-slate-500 shadow-sm shadow-slate-200/40 transition dark:bg-slate-900/60 dark:text-slate-400"
        >
          <span>
            <span
              class="block text-xs font-semibold tracking-[0.2em] text-indigo-500 uppercase dark:text-indigo-400"
            >
              日志路径
            </span>
            <span class="block text-sm leading-relaxed text-slate-600 dark:text-slate-300">
              日志文件存储位置（启动时指定）
            </span>
          </span>
          <input
            type="text"
            class="w-full rounded-xl border border-slate-200 bg-slate-100 px-3 py-3 text-sm text-slate-900 shadow-inner shadow-slate-200 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-400"
            value={config.log_dir}
            disabled
          />
        </label>

        <!-- 操作按钮 -->
        <div class="flex items-center gap-3 pt-2">
          <button
            class="rounded-xl bg-[var(--primary)] px-6 py-2.5 text-sm font-semibold text-[var(--primary-foreground)] shadow-sm transition hover:opacity-90 focus:ring-4 focus:ring-[var(--ring)] focus:outline-none disabled:opacity-50"
            onclick={handleSave}
            disabled={loading}
          >
            {loading ? '保存中…' : '保存'}
          </button>
          <button
            class="rounded-xl bg-[var(--surface-2)] px-6 py-2.5 text-sm font-medium text-[var(--text)] transition hover:bg-[var(--surface)]"
            onclick={loadConfig}
            disabled={loading}
          >
            重置
          </button>
        </div>
      </div>
    {/if}
  </section>

  <!-- 提示信息 -->
  <section
    class="rounded-xl border border-blue-300 bg-blue-50 px-4 py-3 text-sm text-blue-700 dark:border-blue-800 dark:bg-blue-950 dark:text-blue-200"
  >
    <div class="flex items-start gap-3">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        stroke="currentColor"
        stroke-width="1.5"
        class="mt-0.5 h-5 w-5 shrink-0 text-blue-600 dark:text-blue-400"
        fill="none"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
        />
      </svg>
      <div class="min-w-0 flex-1">
        <p class="mb-2 font-semibold">提示</p>
        <ul class="list-disc space-y-1 pl-5 text-sm">
          <li>修改日志级别会立即生效，无需重启服务</li>
          <li>修改日志保留数量会在下次日志滚动时生效</li>
          <li>DEBUG 和 TRACE 级别会产生大量日志，建议仅在排查问题时使用</li>
        </ul>
      </div>
    </div>
  </section>
</div>
