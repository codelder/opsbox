<script lang="ts">
  /**
   * Agent 管理页面组件
   * 展示已注册的 Agent 列表与在线状态，并支持为 Agent 添加/移除标签
   */
  import Alert from '$lib/components/Alert.svelte';
  import { useAgents } from '$lib/modules/agent';

  const agentsStore = useAgents();

  // 每个 Agent 的新标签输入状态（以 agentId 为 key）
  let newTagKey: Record<string, string> = $state({});
  let newTagValue: Record<string, string> = $state({});

  // 初始化加载
  let inited = $state(false);
  $effect(() => {
    if (inited) return;
    inited = true;
    agentsStore.load();
  });

  // 工具：格式化心跳时间为北京时间（CST, Asia/Shanghai）
  function formatHeartbeat(ts: number): string {
    if (!ts) return '未知';
    try {
      return new Date(ts * 1000).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' });
    } catch {
      return new Date(ts * 1000).toLocaleString('zh-CN');
    }
  }

  async function handleAddTag(agentId: string) {
    const key = (newTagKey[agentId] || '').trim();
    const value = (newTagValue[agentId] || '').trim();
    if (!key || !value) return;
    await agentsStore.addTag(agentId, key, value);
    newTagKey[agentId] = '';
    newTagValue[agentId] = '';
  }
</script>

<div class="space-y-6">
  {#if agentsStore.error}
    <Alert type="error" message={agentsStore.error} />
  {/if}

  <!-- 过滤与操作栏 -->
  <section class="rounded-3xl border border-[var(--border)] bg-[var(--surface)] p-6 shadow-lg shadow-black/5">
    <div class="flex flex-col gap-4 md:flex-row md:items-end md:justify-between">
      <div class="flex flex-1 flex-col gap-3 md:max-w-2xl">
        <label for="agent-filter" class="text-xs font-semibold tracking-[0.2em] text-[var(--primary)] uppercase"
          >标签筛选</label
        >
        <input
          id="agent-filter"
          class="w-full rounded-xl border border-[var(--border)] bg-[var(--surface)] px-3 py-3 text-sm text-[var(--text)] shadow-inner shadow-black/5 focus:border-[var(--primary)] focus:ring-4 focus:ring-[var(--ring)] focus:outline-none"
          placeholder="key=value,team=frontend"
          bind:value={agentsStore.tagFilter}
          onkeydown={(e) => {
            if (e.key === 'Enter') agentsStore.load();
          }}
        />
        <p class="text-xs text-[var(--muted)]">用逗号分隔多个条件，例如：env=production,team=frontend</p>
      </div>
      <div class="flex items-center gap-4">
        <label class="inline-flex items-center gap-2 text-sm text-[var(--text)]">
          <input
            type="checkbox"
            class="h-4 w-4"
            bind:checked={agentsStore.onlineOnly}
            onchange={() => agentsStore.load()}
          />
          只看在线
        </label>
        <button
          class="rounded-xl bg-[var(--surface-2)] px-4 py-2 text-sm font-medium text-[var(--text)] transition hover:bg-[var(--surface)]"
          onclick={() => agentsStore.load()}>刷新</button
        >
      </div>
    </div>
  </section>

  <!-- Agent 列表 -->
  <section class="rounded-3xl border border-[var(--border)] bg-[var(--surface)] p-6 shadow-lg shadow-black/5">
    <div class="mb-4 flex items-center justify-between">
      <div>
        <h2 class="text-lg font-semibold text-[var(--text)]">已注册 Agent</h2>
        <p class="mt-1 text-sm text-[var(--muted)]">共 {agentsStore.total} 个</p>
      </div>
    </div>

    {#if agentsStore.loading}
      <div class="py-10 text-center text-sm text-[var(--muted)]">加载中…</div>
    {:else if agentsStore.agents.length === 0}
      <div class="rounded-xl border border-dashed border-[var(--border)] bg-[var(--surface-2)] px-4 py-8 text-center">
        <p class="text-sm text-[var(--muted)]">暂无数据</p>
      </div>
    {:else}
      <div class="space-y-3">
        {#each agentsStore.agents as a (a.id)}
          <div class="rounded-xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
            <div class="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
              <div class="flex min-w-0 flex-1 items-center gap-3">
                <!-- 状态点 -->
                {#if a.status?.type === 'Online'}
                  <span class="inline-block h-2.5 w-2.5 rounded-full bg-green-500"></span>
                {:else if a.status?.type === 'Busy'}
                  <span class="inline-block h-2.5 w-2.5 rounded-full bg-yellow-500"></span>
                {:else}
                  <span class="inline-block h-2.5 w-2.5 rounded-full bg-gray-400"></span>
                {/if}
                <div class="min-w-0">
                  <div class="flex flex-wrap items-center gap-2">
                    <h3 class="truncate text-base font-semibold text-[var(--text)]">{a.name}</h3>
                    <span class="truncate text-xs text-[var(--muted)]">{a.id}</span>
                    {#if a.version}
                      <span class="rounded-full bg-[var(--surface)] px-2 py-0.5 text-xs text-[var(--muted)]"
                        >v{a.version}</span
                      >
                    {/if}
                  </div>
                  <div class="mt-1 flex flex-wrap items-center gap-3 text-xs text-[var(--text)]">
                    <span>主机：{a.hostname}</span>
                    <span
                      >状态：{a.status?.type === 'Online' ? '在线' : a.status?.type === 'Busy' ? '忙碌' : '离线'}</span
                    >
                    <span>上次心跳：{formatHeartbeat(a.last_heartbeat)}</span>
                  </div>
                </div>
              </div>
            </div>

            <!-- 标签管理 -->
            <div class="mt-3 flex flex-col gap-2">
              <div class="flex flex-wrap gap-2">
                {#each a.tags || [] as t (`${a.id}-${t.key}=${t.value}`)}
                  <span
                    class="inline-flex items-center gap-1 rounded-full bg-indigo-50 px-2 py-1 text-xs text-indigo-700 dark:bg-indigo-900/30 dark:text-indigo-300"
                  >
                    {t.key}={t.value}
                    <button
                      class="ml-1 rounded-full p-0.5 text-indigo-700/70 hover:bg-indigo-100 hover:text-indigo-900 dark:text-indigo-300/70 dark:hover:bg-indigo-800 dark:hover:text-indigo-100"
                      title="移除标签"
                      onclick={() => agentsStore.removeTag(a.id, t.key, t.value)}>×</button
                    >
                  </span>
                {/each}
                {#if !a.tags || a.tags.length === 0}
                  <span class="text-xs text-[var(--muted)]">暂无标签</span>
                {/if}
              </div>

              <div class="flex flex-wrap items-center gap-2">
                <input
                  class="w-36 rounded-lg border border-[var(--border)] bg-[var(--surface)] px-2 py-2 text-xs text-[var(--text)] shadow-inner shadow-black/5 focus:border-[var(--primary)] focus:ring-2 focus:ring-[var(--ring)] focus:outline-none"
                  placeholder="key"
                  bind:value={newTagKey[a.id]}
                />
                <input
                  class="w-44 rounded-lg border border-[var(--border)] bg-[var(--surface)] px-2 py-2 text-xs text-[var(--text)] shadow-inner shadow-black/5 focus:border-[var(--primary)] focus:ring-2 focus:ring-[var(--ring)] focus:outline-none"
                  placeholder="value"
                  bind:value={newTagValue[a.id]}
                  onkeydown={(e) => {
                    if (e.key === 'Enter') handleAddTag(a.id);
                  }}
                />
                <button
                  class="rounded-lg bg-[var(--primary)] px-3 py-1.5 text-xs font-semibold text-[var(--primary-foreground)] shadow-sm transition hover:opacity-90 focus:ring-4 focus:ring-[var(--ring)] focus:outline-none"
                  onclick={() => handleAddTag(a.id)}
                  disabled={!newTagKey[a.id]?.trim() || !newTagValue[a.id]?.trim() || agentsStore.loading}
                  >添加标签</button
                >
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </section>
</div>
