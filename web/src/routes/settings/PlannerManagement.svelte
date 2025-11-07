<script lang="ts">
  import Alert from '$lib/components/Alert.svelte';
  import {
    listPlanners,
    getPlanner,
    savePlanner,
    deletePlanner,
    testPlanner,
    type PlannerMeta
  } from '$lib/modules/logseek';

  let loading = $state(false);
  let error = $state<string | null>(null);
  let items = $state<PlannerMeta[]>([]);

  let editing = $state(false);
  let editingApp = $state<string | null>(null);
  let app = $state('');
  let script = $state('');
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let saveSuccess = $state(false);

  // 测试相关状态
  let testQ = $state('');
  let testing = $state(false);
  let testError = $state<string | null>(null);
  import type { Source } from '$lib/modules/logseek';
  let testResult: { cleaned_query: string; sources: Source[] } | null = $state(null);

  // 帮助文档
  let showHelp = $state(false);
  let helpLoading = $state(false);
  let helpError = $state<string | null>(null);
  let helpHtml = $state('');

  async function loadHelp() {
    if (showHelp) {
      showHelp = false;
      return;
    }
    helpLoading = true;
    helpError = null;
    helpHtml = '';
    try {
      const API_BASE = (await import('$lib/modules/logseek/api/config')).getApiBase();
      const res = await fetch(`${API_BASE}/settings/planners/readme`, { headers: { Accept: 'text/html' } });
      if (!res.ok) throw new Error(`加载失败：HTTP ${res.status}`);
      helpHtml = await res.text();
      showHelp = true;
    } catch (e: unknown) {
      const err = e as { message?: string };
      helpError = err?.message || '加载失败';
      showHelp = true;
    } finally {
      helpLoading = false;
    }
  }

  async function refresh() {
    loading = true;
    error = null;
    saveSuccess = false;
    try {
      items = await listPlanners();
    } catch (e: unknown) {
      const err = e as { message?: string };
      error = err?.message || '加载失败';
    }
    loading = false;
  }
  $effect(() => {
    if (!loading && items.length === 0 && !editing) refresh();
  });

  async function startNew() {
    editing = true;
    editingApp = null;
    app = '';
    script = '';
    saveError = null;
    saveSuccess = false;
  }
  async function startEdit(name: string) {
    editing = true;
    editingApp = name;
    saveError = null;
    saveSuccess = false;
    try {
      const r = await getPlanner(name);
      app = r.app;
      script = r.script;
    } catch (e: unknown) {
      const err = e as { message?: string };
      saveError = err?.message || '加载脚本失败';
    }
  }
  function cancelEdit() {
    editing = false;
    editingApp = null;
    app = '';
    script = '';
    testQ = '';
    testError = null;
    testResult = null;
  }

  async function runTest() {
    testError = null;
    testResult = null;
    if (!app.trim()) {
      testError = '请先填写业务标识(app)';
      return;
    }
    if (!testQ.trim()) {
      testError = '请输入完整查询 q';
      return;
    }
    testing = true;
    try {
      const r = await testPlanner({ app: app.trim(), q: testQ });
      testResult = r as { cleaned_query: string; sources: Source[] };
    } catch (e: unknown) {
      const err = e as { message?: string };
      testError = err?.message || '测试失败';
    } finally {
      testing = false;
    }
  }

  async function submit(e: Event) {
    e.preventDefault();
    if (!app.trim()) {
      saveError = 'app 不能为空';
      return;
    }
    saving = true;
    saveError = null;
    saveSuccess = false;
    try {
      await savePlanner({ app: app.trim(), script });
      saveSuccess = true;
      await refresh();
      cancelEdit();
    } catch (e: unknown) {
      const err = e as { message?: string };
      saveError = err?.message || '保存失败';
    } finally {
      saving = false;
    }
  }

  async function remove(name: string) {
    if (!confirm(`确认删除脚本 "${name}"？`)) return;
    try {
      await deletePlanner(name);
      await refresh();
    } catch (e: unknown) {
      const err = e as { message?: string };
      alert(err?.message || '删除失败');
    }
  }
</script>

<div class="space-y-6">
  {#if error}
    <Alert type="error" message={error} />
  {/if}
  {#if saveError}
    <Alert type="error" message={saveError} />
  {/if}
  {#if saveSuccess}
    <Alert type="success" message="脚本已保存" />
  {/if}

  {#if !editing}
    <section class="rounded-3xl border border-slate-200 bg-white shadow-lg dark:border-slate-800 dark:bg-slate-900">
      <div class="flex items-center justify-between border-b border-slate-200 p-6 dark:border-slate-800">
        <div>
          <h2 class="text-lg font-semibold text-slate-900 dark:text-slate-100">规划脚本</h2>
          <p class="mt-1 text-sm text-slate-500 dark:text-slate-400">为不同业务(app:xxx)配置 Starlark 脚本</p>
        </div>
        <button
          class="rounded-xl bg-indigo-600 px-4 py-2 text-sm font-semibold text-white dark:bg-indigo-500"
          onclick={startNew}>新建脚本</button
        >
      </div>
      <div class="p-6">
        {#if loading}
          <div class="text-center text-sm text-slate-500 dark:text-slate-400">加载中…</div>
        {:else if items.length === 0}
          <div
            class="rounded-xl border border-dashed border-slate-200 bg-slate-50 px-4 py-8 text-center dark:border-slate-700 dark:bg-slate-900/50"
          >
            暂无脚本，点击“新建脚本”添加
          </div>
        {:else}
          <div class="space-y-3">
            {#each items as it (it.app)}
              <div
                class="flex items-center justify-between rounded-xl border border-slate-200 bg-slate-50 p-4 dark:border-slate-700 dark:bg-slate-800/50"
              >
                <div class="flex-1">
                  <h3 class="font-semibold text-slate-900 dark:text-slate-100">{it.app}</h3>
                  <p class="mt-1 text-xs text-slate-500 dark:text-slate-400">
                    更新于 {new Date(it.updated_at * 1000).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' })}
                  </p>
                </div>
                <div class="flex items-center gap-2">
                  <button
                    class="rounded-lg px-3 py-1.5 text-sm text-slate-600 hover:bg-slate-200 dark:text-slate-300 dark:hover:bg-slate-700"
                    onclick={() => startEdit(it.app)}>编辑</button
                  >
                  <button
                    class="rounded-lg px-3 py-1.5 text-sm text-red-600 hover:bg-red-100 dark:text-red-400 dark:hover:bg-red-900/30"
                    onclick={() => remove(it.app)}>删除</button
                  >
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </section>
  {:else}
    <section class="rounded-3xl border border-slate-200 bg-white shadow-lg dark:border-slate-800 dark:bg-slate-900">
      <div class="border-b border-slate-200 p-6 dark:border-slate-800">
        <h2 class="text-lg font-semibold text-slate-900 dark:text-slate-100">
          {editingApp ? `编辑脚本：${editingApp}` : '新建脚本'}
        </h2>
      </div>
      <form class="space-y-4 p-6" onsubmit={submit}>
        <div>
          <label for="planner-app" class="block text-sm font-medium text-slate-700 dark:text-slate-300"
            >业务标识（app）</label
          >
          <input
            id="planner-app"
            class="mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-600 dark:bg-slate-800 dark:text-white"
            bind:value={app}
            placeholder="例如：bbip"
            disabled={!!editingApp || saving}
            required
          />
          <p class="mt-1 text-xs text-slate-500 dark:text-slate-400">查询中使用 app:&#123;app&#125; 选择脚本</p>
        </div>
        <div>
          <label for="planner-script" class="block text-sm font-medium text-slate-700 dark:text-slate-300"
            >Starlark 脚本</label
          >
          <textarea
            id="planner-script"
            class="mt-1 block h-80 w-full rounded-lg border border-slate-300 bg-white px-3 py-2 font-mono text-sm dark:border-slate-600 dark:bg-slate-800 dark:text-white"
            bind:value={script}
            spellcheck={false}
          ></textarea>
        </div>
        <div class="flex justify-between gap-3 border-t border-slate-200 pt-4 dark:border-slate-800">
          <div class="flex items-center gap-2">
            <input
              class="w-96 rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-600 dark:bg-slate-800 dark:text-white"
              placeholder="输入完整查询 q（可含 app:/dt:/fdt:/tdt:）"
              bind:value={testQ}
            />
            <button
              type="button"
              class="rounded-xl bg-slate-600 px-4 py-2 text-sm font-semibold text-white disabled:bg-slate-300 dark:bg-slate-500"
              onclick={runTest}
              disabled={testing || !app.trim()}>测试</button
            >
            <button
              type="button"
              class="rounded-xl bg-slate-100 px-3 py-2 text-sm font-medium text-slate-700 hover:bg-slate-200 dark:bg-slate-800 dark:text-slate-200 dark:hover:bg-slate-700"
              onclick={() => loadHelp()}>{showHelp ? '关闭帮助' : '帮助'}</button
            >
          </div>
          <div class="flex items-center gap-3">
            <button
              type="button"
              class="rounded-xl px-4 py-2 text-sm text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800"
              onclick={cancelEdit}
              disabled={saving}>取消</button
            >
            <button
              type="submit"
              class="rounded-xl bg-indigo-600 px-5 py-2 text-sm font-semibold text-white disabled:bg-indigo-300 dark:bg-indigo-500"
              disabled={saving || !app.trim()}>保存</button
            >
          </div>
        </div>
      </form>

      {#if testError}
        <div class="mt-4"><Alert type="error" message={testError} /></div>
      {/if}
      {#if testResult}
        <section
          class="mt-4 rounded-xl border border-slate-200 bg-slate-50 p-4 text-sm dark:border-slate-700 dark:bg-slate-800/50"
        >
          <div class="mb-2 text-slate-700 dark:text-slate-200">
            清理后查询：<code class="rounded bg-slate-200 px-1 py-0.5 text-xs dark:bg-slate-700"
              >{testResult.cleaned_query}</code
            >
          </div>
          <div class="overflow-auto">
            <pre
              class="max-h-80 rounded bg-white p-3 text-xs break-all whitespace-pre-wrap text-slate-800 dark:bg-slate-900 dark:text-slate-100">{JSON.stringify(
                testResult.sources,
                null,
                2
              )}</pre>
          </div>
        </section>
      {/if}

      {#if showHelp}
        <section
          class="mt-6 rounded-3xl border border-slate-200 bg-white p-6 text-sm shadow-lg dark:border-slate-800 dark:bg-slate-900"
        >
          {#if helpLoading}
            <div class="text-slate-500 dark:text-slate-400">加载帮助文档中…</div>
          {:else if helpError}
            <Alert type="error" message={helpError} />
          {:else}
            <div class="prose dark:prose-invert max-w-none">{@html helpHtml}</div>
          {/if}
        </section>
      {/if}
    </section>
  {/if}
</div>
