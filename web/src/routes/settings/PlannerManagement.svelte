<script lang="ts">
  import Alert from '$lib/components/Alert.svelte';
  import {
    listPlanners,
    getPlanner,
    savePlanner,
    deletePlanner,
    testPlanner,
    setDefaultPlanner,
    type PlannerMeta
  } from '$lib/modules/logseek';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from '$lib/components/ui/card';
  import { Badge } from '$lib/components/ui/badge';
  import { Separator } from '$lib/components/ui/separator';
  import { Plus, Edit2, Trash2, Check, PlayCircle, HelpCircle, X } from 'lucide-svelte';

  let loading = $state(false);
  let error = $state<string | null>(null);
  let items = $state<PlannerMeta[]>([]);
  let defaultApp = $state<string | null>(null);

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
  let testResult: { cleaned_query: string; sources: Source[]; debug_logs: string[] } | null = $state(null);

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
      const res = await fetch(`${API_BASE}/settings/planners/readme`, { headers: { Accept: 'text/plain' } });
      if (!res.ok) throw new Error(`加载失败：HTTP ${res.status}`);
      const markdown = await res.text();
      // 在前端渲染 Markdown 为 HTML
      const { marked } = await import('marked');
      helpHtml = marked.parse(markdown) as string;
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
      const response = await listPlanners();
      items = response.items;
      defaultApp = response.default;
    } catch (e: unknown) {
      const err = e as { message?: string };
      error = err?.message || '加载失败';
    }
    loading = false;
  }

  async function handleSetDefault(app: string) {
    try {
      await setDefaultPlanner(app);
      defaultApp = app;
      await refresh();
    } catch (e: unknown) {
      const err = e as { message?: string };
      error = err?.message || '设置默认规划脚本失败';
    }
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
      // 传递当前编辑的脚本内容（如果存在），这样可以在保存前测试
      const r = await testPlanner({
        app: app.trim(),
        q: testQ,
        script: script.trim() || undefined // 如果脚本不为空，传递脚本内容
      });
      testResult = r as { cleaned_query: string; sources: Source[]; debug_logs: string[] };
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
    <Card class="border-border/40 dark:border-gray-700/50">
      <CardHeader class="flex flex-row items-center justify-between space-y-0 pb-2">
        <div class="space-y-1">
          <CardTitle>规划脚本</CardTitle>
          <CardDescription>为不同业务(app:xxx)配置 Starlark 脚本</CardDescription>
        </div>
        <Button onclick={startNew} size="sm">
          <Plus class="mr-2 h-4 w-4" />
          新建脚本
        </Button>
      </CardHeader>
      <CardContent>
        {#if loading}
          <div class="py-8 text-center text-sm text-muted-foreground">加载中…</div>
        {:else if items.length === 0}
          <div
            class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/40 py-12 text-center dark:border-gray-700/50"
          >
            <p class="text-sm text-muted-foreground">暂无脚本，点击"新建脚本"添加</p>
          </div>
        {:else}
          <div class="grid gap-4">
            {#each items as it (it.app)}
              <div
                class="flex items-center justify-between rounded-lg border border-border/40 p-4 transition-colors hover:bg-muted/50 dark:border-gray-700/50"
              >
                <div class="grid gap-1">
                  <div class="flex items-center gap-2">
                    <span class="font-semibold">{it.app}</span>
                    {#if defaultApp === it.app}
                      <Badge variant="default" class="text-xs" title="默认规划脚本">默认</Badge>
                    {/if}
                  </div>
                  <p class="text-xs text-muted-foreground">
                    更新于 {new Date(it.updated_at * 1000).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' })}
                  </p>
                </div>
                <div class="flex items-center gap-2">
                  {#if defaultApp !== it.app}
                    <Button
                      variant="ghost"
                      size="sm"
                      class="text-muted-foreground"
                      onclick={() => handleSetDefault(it.app)}
                      title="设为默认规划脚本"
                    >
                      设为默认
                    </Button>
                  {/if}
                  <Button variant="ghost" size="icon" onclick={() => startEdit(it.app)}>
                    <Edit2 class="h-4 w-4" />
                    <span class="sr-only">编辑</span>
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    class="text-destructive hover:bg-destructive/10 hover:text-destructive"
                    onclick={() => remove(it.app)}
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
        <CardTitle>{editingApp ? `编辑脚本：${editingApp}` : '新建脚本'}</CardTitle>
      </CardHeader>
      <form onsubmit={submit}>
        <CardContent class="space-y-4">
          <div class="space-y-2">
            <Label for="planner-app">业务标识（app）</Label>
            <Input
              id="planner-app"
              bind:value={app}
              placeholder="例如：bbip"
              disabled={!!editingApp || saving}
              required
            />
            <p class="text-xs text-muted-foreground">查询中使用 app:&#123;app&#125; 选择脚本</p>
          </div>
          <div class="space-y-2">
            <Label for="planner-script">Starlark 脚本</Label>
            <textarea
              id="planner-script"
              class="flex min-h-80 w-full rounded-md border border-input bg-background px-3 py-2 font-mono text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
              bind:value={script}
              spellcheck={false}
            ></textarea>
          </div>

          <Separator />

          <div class="space-y-2">
            <Label for="test-query">测试脚本</Label>
            <div class="flex gap-2">
              <Input
                id="test-query"
                class="flex-1"
                placeholder="输入完整查询 q（可含 app:/dt:/fdt:/tdt:）"
                bind:value={testQ}
              />
              <Button type="button" variant="secondary" onclick={runTest} disabled={testing || !app.trim()}>
                <PlayCircle class="mr-2 h-4 w-4" />
                {testing ? '测试中…' : '测试'}
              </Button>
              <Button type="button" variant="outline" onclick={() => loadHelp()}>
                <HelpCircle class="mr-2 h-4 w-4" />
                {showHelp ? '关闭帮助' : '帮助'}
              </Button>
            </div>
          </div>
        </CardContent>
        <CardFooter class="flex justify-end gap-2">
          <Button variant="outline" type="button" onclick={cancelEdit} disabled={saving}>取消</Button>
          <Button type="submit" disabled={saving || !app.trim()}>
            {saving ? '保存中…' : '保存'}
          </Button>
        </CardFooter>
      </form>

      {#if testError}
        <div class="px-6 pb-4"><Alert type="error" message={testError} /></div>
      {/if}
      {#if testResult}
        <div class="px-6 pb-6">
          <Card class="border-border/40 dark:border-gray-700/50">
            <CardContent class="space-y-3 p-4">
              <div class="text-sm">
                <span class="text-muted-foreground">清理后查询：</span>
                <code class="ml-2 rounded bg-muted px-2 py-1 font-mono text-xs">{testResult.cleaned_query}</code>
              </div>
              {#if testResult.debug_logs && testResult.debug_logs.length > 0}
                <div>
                  <h4 class="mb-2 text-sm font-medium">调试日志（print 输出）：</h4>
                  <div class="max-h-40 overflow-auto rounded border bg-muted/30 p-3">
                    {#each testResult.debug_logs as log}
                      <div class="mb-1 font-mono text-xs">{log}</div>
                    {/each}
                  </div>
                </div>
              {/if}
              <div class="overflow-auto">
                <pre
                  class="max-h-80 rounded border bg-muted/30 p-3 font-mono text-xs break-all whitespace-pre-wrap">{JSON.stringify(
                    testResult.sources,
                    null,
                    2
                  )}</pre>
              </div>
            </CardContent>
          </Card>
        </div>
      {/if}

      {#if showHelp}
        <div class="px-6 pb-6">
          <Card class="border-border/40 dark:border-gray-700/50">
            <CardContent class="p-6">
              {#if helpLoading}
                <div class="text-sm text-muted-foreground">加载帮助文档中…</div>
              {:else if helpError}
                <Alert type="error" message={helpError} />
              {:else}
                <div class="prose prose-sm dark:prose-invert max-w-none">{@html helpHtml}</div>
              {/if}
            </CardContent>
          </Card>
        </div>
      {/if}
    </Card>
  {/if}
</div>
