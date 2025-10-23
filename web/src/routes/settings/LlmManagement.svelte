<script lang="ts">
  /**
   * 大模型（LLM）管理组件
   * 支持配置 Ollama 与多个 OpenAI 后端，并设置默认后端
   */
  import Alert from '$lib/components/Alert.svelte';
  import { useLlmBackends } from '$lib/modules/logseek/composables/useLlmBackends.svelte';
  import type { LlmBackendUpsertPayload, LlmProviderType } from '$lib/modules/logseek';

  const store = useLlmBackends();

  // 初始化加载
  let inited = $state(false);
  $effect(() => {
    if (inited) return;
    inited = true;
    store.load();
  });

  // 编辑状态
  let editing = $state(false);
  let editingName = $state<string | null>(null);

  // 表单字段
  let name = $state('');
  let provider = $state<LlmProviderType>('ollama');
  let baseUrl = $state('');
  let model = $state('');
  let timeoutSecs = $state(60);
  let apiKey = $state(''); // openai
  let organization = $state(''); // openai
  let project = $state(''); // openai

  function startNew() {
    editing = true;
    editingName = null;
    name = '';
    provider = 'ollama';
    baseUrl = '';
    model = '';
    timeoutSecs = 60;
    apiKey = '';
    organization = '';
    project = '';
    store.clearSaveState();
  }

  function startEdit(item: {
    name: string;
    provider: LlmProviderType;
    base_url: string;
    model: string;
    timeout_secs: number;
  }) {
    editing = true;
    editingName = item.name;
    name = item.name;
    provider = item.provider;
    baseUrl = item.base_url;
    model = item.model;
    timeoutSecs = item.timeout_secs || 60;
    apiKey = '';
    organization = '';
    project = '';
    store.clearSaveState();
  }

  function cancelEdit() {
    editing = false;
    editingName = null;
    name = '';
    baseUrl = '';
    model = '';
    timeoutSecs = 60;
    apiKey = '';
    organization = '';
    project = '';
    store.clearSaveState();
  }

  async function handleSave(e: Event) {
    e.preventDefault();
    const payload: LlmBackendUpsertPayload = {
      name: name.trim(),
      provider,
      base_url: baseUrl.trim(),
      model: model.trim(),
      timeout_secs: Math.max(1, Number(timeoutSecs) || 60)
    };
    if (provider === 'openai') {
      if (apiKey.trim()) payload.api_key = apiKey.trim();
      if (organization.trim()) payload.organization = organization.trim();
      if (project.trim()) payload.project = project.trim();
    }
    const ok = await store.save(payload);
    if (ok) {
      cancelEdit();
    }
  }

  async function handleDelete(n: string) {
    if (!confirm(`确认删除后端 "${n}"？`)) return;
    store.clearDeleteState();
    await store.remove(n);
  }
</script>

<div class="space-y-6">
  {#if store.error}
    <Alert type="error" message={store.error} />
  {/if}
  {#if store.saveError}
    <Alert type="error" message={store.saveError} />
  {/if}
  {#if store.saveSuccess}
    <Alert type="success" message="已保存大模型配置" />
  {/if}

  {#if !editing}
    <section
      class="rounded-3xl border border-slate-200 bg-white shadow-lg shadow-slate-200/40 dark:border-slate-800 dark:bg-slate-900 dark:shadow-black/30"
    >
      <div class="flex items-center justify-between border-b border-slate-200 p-6 dark:border-slate-800">
        <div>
          <h2 class="text-lg font-semibold text-slate-900 dark:text-slate-100">大模型配置</h2>
          <p class="mt-1 text-sm text-slate-500 dark:text-slate-400">配置 Ollama 与多个 OpenAI 后端，并选择默认</p>
        </div>
        <button
          class="inline-flex items-center rounded-xl bg-indigo-600 px-4 py-2 text-sm font-semibold text-white shadow-sm transition hover:bg-indigo-500 focus:ring-4 focus:ring-indigo-200 focus:outline-none dark:bg-indigo-500 dark:hover:bg-indigo-400 dark:focus:ring-indigo-500/40"
          onclick={startNew}
        >
          新建后端
        </button>
      </div>

      <div class="p-6">
        {#if store.loading}
          <div class="text-center text-sm text-slate-500 dark:text-slate-400">加载中…</div>
        {:else if store.backends.length === 0}
          <div
            class="rounded-xl border border-dashed border-slate-200 bg-slate-50 px-4 py-8 text-center dark:border-slate-700 dark:bg-slate-900/50"
          >
            <p class="text-sm text-slate-600 dark:text-slate-400">暂无配置，点击 "新建后端" 添加</p>
          </div>
        {:else}
          <div class="space-y-3">
            {#each store.backends as b (b.name)}
              <div
                class="flex items-center justify-between rounded-xl border border-slate-200 bg-slate-50 p-4 dark:border-slate-700 dark:bg-slate-800/50"
              >
                <div class="flex-1">
                  <div class="flex items-center gap-2">
                    <h3 class="font-semibold text-slate-900 dark:text-slate-100">{b.name}</h3>
                    <span
                      class="rounded-full bg-slate-200 px-2 py-0.5 text-xs text-slate-700 dark:bg-slate-700 dark:text-slate-200"
                      >{b.provider}</span
                    >
                    {#if store.defaultName === b.name}
                      <span
                        class="inline-flex items-center rounded-full bg-blue-100 px-2 py-0.5 text-xs font-medium text-blue-800 dark:bg-blue-900/30 dark:text-blue-300"
                        >默认</span
                      >
                    {/if}
                  </div>
                  <p class="mt-1 text-sm text-slate-600 dark:text-slate-400">{b.base_url} · {b.model}</p>
                </div>
                <div class="flex items-center gap-2">
                  <button
                    class="rounded-lg px-3 py-1.5 text-sm font-medium text-slate-600 transition hover:bg-slate-200 hover:text-slate-900 dark:text-slate-300 dark:hover:bg-slate-700 dark:hover:text-slate-100"
                    onclick={() =>
                      startEdit({
                        name: b.name,
                        provider: b.provider,
                        base_url: b.base_url,
                        model: b.model,
                        timeout_secs: b.timeout_secs
                      })}
                  >
                    编辑
                  </button>
                  <button
                    class="rounded-lg px-3 py-1.5 text-sm font-medium text-indigo-600 transition hover:bg-indigo-100 hover:text-indigo-700 dark:text-indigo-400 dark:hover:bg-indigo-900/30 dark:hover:text-indigo-300"
                    onclick={() => store.makeDefault(b.name)}
                    disabled={store.settingDefault || store.defaultName === b.name}
                  >
                    设为默认
                  </button>
                  <button
                    class="rounded-lg px-3 py-1.5 text-sm font-medium text-red-600 transition hover:bg-red-100 hover:text-red-700 dark:text-red-400 dark:hover:bg-red-900/30 dark:hover:text-red-300"
                    onclick={() => handleDelete(b.name)}
                    disabled={store.deleting}
                  >
                    删除
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </section>
  {:else}
    <section
      class="rounded-3xl border border-slate-200 bg-white shadow-lg shadow-slate-200/40 dark:border-slate-800 dark:bg-slate-900 dark:shadow-black/30"
    >
      <div class="border-b border-slate-200 p-6 dark:border-slate-800">
        <h2 class="text-lg font-semibold text-slate-900 dark:text-slate-100">
          {editingName ? `编辑后端：${editingName}` : '新建后端'}
        </h2>
      </div>
      <form class="space-y-6 p-6" onsubmit={handleSave}>
        <div class="grid grid-cols-1 gap-6 md:grid-cols-2">
          <div>
            <label for="llm-name" class="block text-sm font-medium text-slate-700 dark:text-slate-300">名称</label>
            <input
              id="llm-name"
              class="mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-600 dark:bg-slate-800 dark:text-white"
              bind:value={name}
              placeholder="例如：ollama-local / openai-prod"
              disabled={editingName !== null}
              required
            />
            <p class="mt-1 text-xs text-slate-500 dark:text-slate-400">用于标识配置，创建后不可修改</p>
          </div>
          <div>
            <label for="llm-provider" class="block text-sm font-medium text-slate-700 dark:text-slate-300">提供方</label
            >
            <select
              id="llm-provider"
              class="mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-600 dark:bg-slate-800 dark:text-white"
              bind:value={provider}
              disabled={store.saving}
            >
              <option value="ollama">ollama</option>
              <option value="openai">openai</option>
            </select>
          </div>
          <div class="md:col-span-2">
            <label for="llm-base-url" class="block text-sm font-medium text-slate-700 dark:text-slate-300"
              >基础地址</label
            >
            <input
              id="llm-base-url"
              class="mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-600 dark:bg-slate-800 dark:text-white"
              bind:value={baseUrl}
              placeholder="http://127.0.0.1:11434 或 https://api.openai.com"
              required
            />
          </div>
          <div>
            <label for="llm-model" class="block text-sm font-medium text-slate-700 dark:text-slate-300">模型</label>
            <input
              id="llm-model"
              class="mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-600 dark:bg-slate-800 dark:text-white"
              bind:value={model}
              placeholder="qwen3:8b 或 gpt-4o-mini"
              required
            />
          </div>
          <div>
            <label for="llm-timeout" class="block text-sm font-medium text-slate-700 dark:text-slate-300"
              >超时（秒）</label
            >
            <input
              id="llm-timeout"
              type="number"
              min="1"
              class="mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-600 dark:bg-slate-800 dark:text-white"
              bind:value={timeoutSecs}
            />
          </div>
          {#if provider === 'openai'}
            <div class="md:col-span-2">
              <label for="llm-api-key" class="block text-sm font-medium text-slate-700 dark:text-slate-300"
                >API Key</label
              >
              <input
                id="llm-api-key"
                type="password"
                class="mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-600 dark:bg-slate-800 dark:text-white"
                bind:value={apiKey}
                placeholder={editingName ? '留空表示不修改原密钥' : 'sk-...'}
                autocomplete="off"
              />
            </div>
            <div>
              <label for="llm-org" class="block text-sm font-medium text-slate-700 dark:text-slate-300"
                >Organization（可选）</label
              >
              <input
                id="llm-org"
                class="mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-600 dark:bg-slate-800 dark:text-white"
                bind:value={organization}
              />
            </div>
            <div>
              <label for="llm-project" class="block text-sm font-medium text-slate-700 dark:text-slate-300"
                >Project（可选）</label
              >
              <input
                id="llm-project"
                class="mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-600 dark:bg-slate-800 dark:text-white"
                bind:value={project}
              />
            </div>
          {/if}
        </div>
        <div class="flex justify-end gap-3 border-t border-slate-200 pt-6 dark:border-slate-800">
          <button
            type="button"
            class="rounded-xl px-4 py-2 text-sm text-slate-600 transition hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800"
            onclick={cancelEdit}
            disabled={store.saving}>取消</button
          >
          <button
            type="submit"
            class="inline-flex items-center justify-center rounded-xl bg-indigo-600 px-5 py-2 text-sm font-semibold text-white shadow-sm transition hover:bg-indigo-500 focus:ring-4 focus:ring-indigo-200 focus:outline-none disabled:cursor-not-allowed disabled:bg-indigo-300 dark:bg-indigo-500 dark:hover:bg-indigo-400 dark:focus:ring-indigo-500/40"
            disabled={store.saving || !name.trim() || !baseUrl.trim() || !model.trim()}
            >{store.saving ? '保存中…' : '保存'}</button
          >
        </div>
      </form>
    </section>
  {/if}
</div>
