<script lang="ts">
  /**
   * 大模型（LLM）管理组件
   * 支持配置 Ollama 与多个 OpenAI 后端，并设置默认后端
   */
  import Alert from '$lib/components/Alert.svelte';
  import { useLlmBackends } from '$lib/modules/logseek/composables/useLlmBackends.svelte';
  import type { LlmBackendUpsertPayload, LlmProviderType } from '$lib/modules/logseek';
  import { listLlmModelsByParams, listLlmModelsByBackend } from '$lib/modules/logseek/api';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from '$lib/components/ui/card';
  import { Badge } from '$lib/components/ui/badge';
  import { Plus, Edit2, Trash2, Check, Loader2, RefreshCw } from 'lucide-svelte';

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

  // 模型下拉候选
  let modelOptions = $state<string[]>([]);
  let modelsLoading = $state(false);
  let modelsError = $state<string | null>(null);

  async function refreshModels() {
    modelsError = null;

    // 已保存配置（含密钥）时优先按名称拉取，避免要求用户重复输入 API Key
    if (editingName) {
      modelsLoading = true;
      try {
        modelOptions = await listLlmModelsByBackend(editingName);
        return;
      } catch (e: unknown) {
        const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
        modelsError = err.message ?? '加载模型失败';
        modelOptions = [];
      } finally {
        modelsLoading = false;
      }
      return;
    }

    // 未保存时，依据当前表单参数临时拉取
    if (!baseUrl.trim()) {
      modelOptions = [];
      return;
    }
    if (provider === 'openai' && !apiKey.trim()) {
      modelOptions = [];
      return;
    }

    modelsLoading = true;
    try {
      const list = await listLlmModelsByParams({
        provider,
        base_url: baseUrl.trim(),
        api_key: provider === 'openai' ? apiKey.trim() || undefined : undefined,
        organization: provider === 'openai' ? organization.trim() || undefined : undefined,
        project: provider === 'openai' ? project.trim() || undefined : undefined
      });
      modelOptions = list;
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      modelsError = err.message ?? '加载模型失败';
      modelOptions = [];
    } finally {
      modelsLoading = false;
    }
  }

  // 当关键字段变化时自动刷新模型列表（简单防抖）
  let modelsTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    if (!editing) return;
    if (modelsTimer) clearTimeout(modelsTimer);
    modelsTimer = setTimeout(() => {
      refreshModels();
    }, 400);
  });

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
    <Card class="border-border/40 dark:border-gray-700/50">
      <CardHeader class="flex flex-row items-center justify-between space-y-0 pb-2">
        <div class="space-y-1">
          <CardTitle>大模型配置</CardTitle>
          <CardDescription>配置 Ollama 与多个 OpenAI 后端，并选择默认</CardDescription>
        </div>
        <Button onclick={startNew} size="sm">
          <Plus class="mr-2 h-4 w-4" />
          新建后端
        </Button>
      </CardHeader>
      <CardContent>
        {#if store.loading}
          <div class="py-8 text-center text-sm text-muted-foreground">加载中…</div>
        {:else if store.backends.length === 0}
          <div
            class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/40 py-12 text-center dark:border-gray-700/50"
          >
            <p class="text-sm text-muted-foreground">暂无配置，点击 "新建后端" 添加</p>
          </div>
        {:else}
          <div class="grid gap-4">
            {#each store.backends as b (b.name)}
              <div
                class="flex items-center justify-between rounded-lg border border-border/40 p-4 transition-colors hover:bg-muted/50 dark:border-gray-700/50"
              >
                <div class="grid gap-1">
                  <div class="flex items-center gap-2">
                    <span class="font-semibold">{b.name}</span>
                    <Badge variant="secondary" class="text-xs font-normal">{b.provider}</Badge>
                    {#if store.defaultName === b.name}
                      <Badge variant="default" class="text-xs">默认</Badge>
                    {/if}
                  </div>
                  <div class="flex items-center text-sm text-muted-foreground">
                    {b.base_url} · {b.model}
                  </div>
                </div>
                <div class="flex items-center gap-2">
                  <Button
                    variant="ghost"
                    size="icon"
                    onclick={() =>
                      startEdit({
                        name: b.name,
                        provider: b.provider,
                        base_url: b.base_url,
                        model: b.model,
                        timeout_secs: b.timeout_secs
                      })}
                  >
                    <Edit2 class="h-4 w-4" />
                    <span class="sr-only">编辑</span>
                  </Button>

                  <Button
                    variant="ghost"
                    size="sm"
                    class={store.defaultName === b.name ? 'text-primary' : 'text-muted-foreground'}
                    onclick={() => store.makeDefault(b.name)}
                    disabled={store.settingDefault || store.defaultName === b.name}
                    title="设为默认"
                  >
                    {#if store.defaultName === b.name}
                      <Check class="mr-1 h-4 w-4" />
                      已默认
                    {:else}
                      设为默认
                    {/if}
                  </Button>

                  <Button
                    variant="ghost"
                    size="icon"
                    class="text-destructive hover:bg-destructive/10 hover:text-destructive"
                    onclick={() => handleDelete(b.name)}
                    disabled={store.deleting}
                  >
                    <Trash2 class="h-4 w-4" />
                    <span class="sr-only">删除</span>
                  </Button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </CardContent>
    </Card>
  {:else}
    <Card class="border-border/40 dark:border-gray-700/50">
      <CardHeader>
        <CardTitle>{editingName ? `编辑后端：${editingName}` : '新建后端'}</CardTitle>
      </CardHeader>
      <form onsubmit={handleSave}>
        <CardContent class="space-y-4">
          <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
            <div class="space-y-2">
              <Label for="llm-name">名称</Label>
              <Input
                id="llm-name"
                bind:value={name}
                placeholder="例如：ollama-local / openai-prod"
                disabled={editingName !== null}
                required
              />
              <p class="text-xs text-muted-foreground">用于标识配置，创建后不可修改</p>
            </div>
            <div class="space-y-2">
              <Label for="llm-provider">接口类型</Label>
              <select
                id="llm-provider"
                class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
                bind:value={provider}
                disabled={store.saving}
              >
                <option value="ollama">ollama</option>
                <option value="openai">openai</option>
              </select>
            </div>
            <div class="space-y-2 md:col-span-2">
              <Label for="llm-base-url">基础地址</Label>
              <Input
                id="llm-base-url"
                bind:value={baseUrl}
                placeholder="http://127.0.0.1:11434 或 https://api.openai.com"
                required
              />
            </div>
            <div class="space-y-2">
              <div class="flex items-center justify-between">
                <Label for="llm-model">模型</Label>
                <Button
                  variant="link"
                  size="sm"
                  class="h-auto p-0 text-xs"
                  onclick={refreshModels}
                  disabled={modelsLoading}
                  type="button"
                >
                  {#if modelsLoading}
                    <Loader2 class="mr-1 h-3 w-3 animate-spin" />
                  {:else}
                    <RefreshCw class="mr-1 h-3 w-3" />
                  {/if}
                  {modelsLoading ? '加载中…' : '加载模型'}
                </Button>
              </div>
              <Input id="llm-model" bind:value={model} placeholder="从下拉选择或手动输入" required list="llm-models" />
              {#if modelsError}
                <p class="text-xs text-destructive">{modelsError}</p>
              {/if}
              <datalist id="llm-models">
                {#each modelOptions as m (m)}
                  <option value={m}></option>
                {/each}
              </datalist>
            </div>
            <div class="space-y-2">
              <Label for="llm-timeout">超时（秒）</Label>
              <Input id="llm-timeout" type="number" min="1" bind:value={timeoutSecs} />
            </div>
            {#if provider === 'openai'}
              <div class="space-y-2 md:col-span-2">
                <Label for="llm-api-key">API Key</Label>
                <Input
                  id="llm-api-key"
                  type="password"
                  bind:value={apiKey}
                  placeholder={editingName ? '留空表示不修改原密钥' : 'sk-...'}
                  autocomplete="off"
                />
              </div>
              <div class="space-y-2">
                <Label for="llm-org">Organization（可选）</Label>
                <Input id="llm-org" bind:value={organization} />
              </div>
              <div class="space-y-2">
                <Label for="llm-project">Project（可选）</Label>
                <Input id="llm-project" bind:value={project} />
              </div>
            {/if}
          </div>
        </CardContent>
        <CardFooter class="flex justify-end gap-2">
          <Button variant="outline" type="button" onclick={cancelEdit} disabled={store.saving}>取消</Button>
          <Button type="submit" disabled={store.saving || !name.trim() || !baseUrl.trim() || !model.trim()}>
            {store.saving ? '保存中…' : '保存'}
          </Button>
        </CardFooter>
      </form>
    </Card>
  {/if}
</div>
