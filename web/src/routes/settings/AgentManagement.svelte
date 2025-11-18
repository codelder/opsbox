<script lang="ts">
  /**
   * Agent 管理页面组件
   * 展示已注册的 Agent 列表与在线状态，并支持为 Agent 添加/移除标签
   */
  import Alert from '$lib/components/Alert.svelte';
  import { useAgents } from '$lib/modules/agent';
  import {
    fetchAgentLogConfig,
    updateAgentLogLevel,
    updateAgentLogRetention,
    type LogConfigResponse
  } from '$lib/modules/agent/api';

  const agentsStore = useAgents();

  // 每个 Agent 的新标签输入状态（以 agentId 为 key）
  let newTagKey: Record<string, string> = $state({});
  let newTagValue: Record<string, string> = $state({});

  // 日志设置展开状态
  let expandedLogSettings: Record<string, boolean> = $state({});

  // 每个 Agent 的日志配置
  let agentLogConfigs: Record<string, LogConfigResponse> = $state({});

  // 日志配置加载状态
  let logConfigLoading: Record<string, boolean> = $state({});

  // 日志配置错误
  let logConfigError: Record<string, string> = $state({});

  // 日志配置成功消息
  let logConfigSuccess: Record<string, string> = $state({});

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

  async function toggleLogSettings(agentId: string) {
    expandedLogSettings[agentId] = !expandedLogSettings[agentId];
    
    // 如果展开且还没有加载配置，则加载
    if (expandedLogSettings[agentId] && !agentLogConfigs[agentId]) {
      await loadAgentLogConfig(agentId);
    }
  }

  async function loadAgentLogConfig(agentId: string) {
    logConfigLoading[agentId] = true;
    logConfigError[agentId] = '';
    try {
      agentLogConfigs[agentId] = await fetchAgentLogConfig(agentId);
    } catch (e) {
      logConfigError[agentId] = e instanceof Error ? e.message : '加载日志配置失败';
    } finally {
      logConfigLoading[agentId] = false;
    }
  }

  async function handleSaveAgentLogConfig(agentId: string) {
    const config = agentLogConfigs[agentId];
    if (!config) return;

    logConfigLoading[agentId] = true;
    logConfigError[agentId] = '';
    logConfigSuccess[agentId] = '';

    try {
      // 更新日志级别
      await updateAgentLogLevel(agentId, config.level);

      // 更新保留数量
      await updateAgentLogRetention(agentId, config.retention_count);

      // 重新加载配置以获取最新值
      await loadAgentLogConfig(agentId);

      logConfigSuccess[agentId] = '配置已保存';
      setTimeout(() => {
        logConfigSuccess[agentId] = '';
      }, 3000);
    } catch (e) {
      logConfigError[agentId] = e instanceof Error ? e.message : '保存失败';
    } finally {
      logConfigLoading[agentId] = false;
    }
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
          value={agentsStore.tagFilter}
          oninput={(event) => {
            const target = event.currentTarget as HTMLInputElement;
            agentsStore.tagFilter = target.value;
          }}
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
            checked={agentsStore.onlineOnly}
            onchange={(event) => {
              const target = event.currentTarget as HTMLInputElement;
              agentsStore.onlineOnly = target.checked;
              agentsStore.load();
            }}
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
                  data-testid={`tag-key-${a.id}`}
                  bind:value={newTagKey[a.id]}
                />
                <input
                  class="w-44 rounded-lg border border-[var(--border)] bg-[var(--surface)] px-2 py-2 text-xs text-[var(--text)] shadow-inner shadow-black/5 focus:border-[var(--primary)] focus:ring-2 focus:ring-[var(--ring)] focus:outline-none"
                  placeholder="value"
                  data-testid={`tag-value-${a.id}`}
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

            <!-- 日志设置（可展开） -->
            <div class="mt-4 border-t border-[var(--border)] pt-4">
              <button
                class="flex w-full items-center justify-between text-sm font-medium text-[var(--text)] hover:text-[var(--primary)]"
                onclick={() => toggleLogSettings(a.id)}
              >
                <span>日志设置</span>
                <svg
                  class="h-4 w-4 transition-transform {expandedLogSettings[a.id] ? 'rotate-180' : ''}"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  fill="none"
                >
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                </svg>
              </button>

              {#if expandedLogSettings[a.id]}
                <div class="mt-4 space-y-3">
                  {#if logConfigError[a.id]}
                    <div class="rounded-lg border border-red-300 bg-red-50 px-3 py-2 text-xs text-red-700 dark:border-red-800 dark:bg-red-950 dark:text-red-200">
                      {logConfigError[a.id]}
                    </div>
                  {/if}

                  {#if logConfigSuccess[a.id]}
                    <div class="rounded-lg border border-emerald-300 bg-emerald-50 px-3 py-2 text-xs text-emerald-700 dark:border-emerald-800 dark:bg-emerald-950 dark:text-emerald-200">
                      {logConfigSuccess[a.id]}
                    </div>
                  {/if}

                  {#if logConfigLoading[a.id] && !agentLogConfigs[a.id]}
                    <div class="py-4 text-center text-xs text-[var(--muted)]">加载中…</div>
                  {:else if agentLogConfigs[a.id]}
                    <!-- 日志级别 -->
                    <div class="flex items-center gap-3">
                      <label for="log-level-{a.id}" class="w-24 text-xs text-[var(--muted)]">日志级别</label>
                      <select
                        id="log-level-{a.id}"
                        class="flex-1 rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2 text-xs text-[var(--text)] focus:border-[var(--primary)] focus:ring-2 focus:ring-[var(--ring)] focus:outline-none"
                        bind:value={agentLogConfigs[a.id].level}
                        disabled={a.status?.type !== 'Online'}
                      >
                        <option value="error">ERROR</option>
                        <option value="warn">WARN</option>
                        <option value="info">INFO</option>
                        <option value="debug">DEBUG</option>
                        <option value="trace">TRACE</option>
                      </select>
                    </div>

                    <!-- 日志保留 -->
                    <div class="flex items-center gap-3">
                      <label for="log-retention-{a.id}" class="w-24 text-xs text-[var(--muted)]">日志保留</label>
                      <input
                        id="log-retention-{a.id}"
                        type="number"
                        class="flex-1 rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2 text-xs text-[var(--text)] focus:border-[var(--primary)] focus:ring-2 focus:ring-[var(--ring)] focus:outline-none"
                        bind:value={agentLogConfigs[a.id].retention_count}
                        min="1"
                        max="365"
                        disabled={a.status?.type !== 'Online'}
                      />
                      <span class="text-xs text-[var(--muted)]">天</span>
                    </div>

                    <!-- 日志路径（只读） -->
                    <div class="flex items-center gap-3">
                      <label for="log-dir-{a.id}" class="w-24 text-xs text-[var(--muted)]">日志路径</label>
                      <input
                        id="log-dir-{a.id}"
                        type="text"
                        class="flex-1 rounded-lg border border-[var(--border)] bg-slate-100 px-3 py-2 text-xs text-slate-600 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-400"
                        value={agentLogConfigs[a.id].log_dir}
                        disabled
                      />
                    </div>

                    <!-- 操作按钮 -->
                    <div class="flex items-center gap-2">
                      <button
                        class="rounded-lg bg-[var(--primary)] px-4 py-1.5 text-xs font-semibold text-[var(--primary-foreground)] shadow-sm transition hover:opacity-90 focus:ring-4 focus:ring-[var(--ring)] focus:outline-none disabled:opacity-50"
                        onclick={() => handleSaveAgentLogConfig(a.id)}
                        disabled={a.status?.type !== 'Online' || logConfigLoading[a.id]}
                      >
                        {logConfigLoading[a.id] ? '保存中…' : '保存'}
                      </button>
                      {#if a.status?.type !== 'Online'}
                        <span class="text-xs text-amber-600 dark:text-amber-400">Agent 离线，无法修改配置</span>
                      {/if}
                    </div>

                    <!-- 提示信息 -->
                    <div class="rounded-lg border border-blue-300 bg-blue-50 px-3 py-2 text-xs text-blue-700 dark:border-blue-800 dark:bg-blue-950 dark:text-blue-200">
                      <ul class="list-disc space-y-1 pl-4">
                        <li>修改日志级别会立即生效，无需重启 Agent</li>
                        <li>修改日志保留数量会在下次日志滚动时生效</li>
                      </ul>
                    </div>
                  {/if}
                </div>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </section>
</div>
