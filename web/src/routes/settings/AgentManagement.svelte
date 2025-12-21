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
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Card, CardHeader, CardTitle, CardDescription, CardContent } from '$lib/components/ui/card';
  import { Badge } from '$lib/components/ui/badge';
  import { Switch } from '$lib/components/ui/switch';
  import {
    RefreshCw,
    Search,
    Server,
    Activity,
    Clock,
    Tag,
    Settings,
    ChevronDown,
    ChevronUp,
    X,
    Plus,
    Folder
  } from 'lucide-svelte';

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
  <Card class="border-border/40 dark:border-gray-700/50">
    <CardContent class="p-6">
      <div class="flex flex-col gap-4 md:flex-row md:items-end md:justify-between">
        <div class="flex flex-1 flex-col gap-3 md:max-w-2xl">
          <Label for="agent-filter" class="text-xs font-semibold tracking-wider text-primary uppercase">标签筛选</Label>
          <div class="relative">
            <Search class="absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              id="agent-filter"
              class="pl-9"
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
          </div>
          <p class="text-xs text-muted-foreground">用逗号分隔多个条件，例如：env=production,team=frontend</p>
        </div>
        <div class="flex items-center gap-4">
          <div class="flex items-center space-x-2">
            <Switch
              id="online-only"
              checked={agentsStore.onlineOnly}
              onCheckedChange={(v) => {
                agentsStore.onlineOnly = v;
                agentsStore.load();
              }}
            />
            <Label
              for="online-only"
              class="text-sm leading-none font-medium peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
            >
              只看在线
            </Label>
          </div>
          <Button variant="outline" onclick={() => agentsStore.load()} class="gap-2">
            <RefreshCw class="h-4 w-4" />
            刷新
          </Button>
        </div>
      </div>
    </CardContent>
  </Card>

  <!-- Agent 列表 -->
  <Card class="border-border/40 dark:border-gray-700/50">
    <CardHeader class="flex flex-row items-center justify-between space-y-0 pb-2">
      <div class="space-y-1">
        <CardTitle>已注册 Agent</CardTitle>
        <CardDescription>共 {agentsStore.total} 个</CardDescription>
      </div>
    </CardHeader>
    <CardContent>
      {#if agentsStore.loading}
        <div class="py-10 text-center text-sm text-muted-foreground">加载中…</div>
      {:else if agentsStore.agents.length === 0}
        <div
          class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/40 py-12 text-center dark:border-gray-700/50"
        >
          <Server class="h-10 w-10 text-muted-foreground/50" />
          <p class="mt-4 text-sm text-muted-foreground">暂无数据</p>
        </div>
      {:else}
        <div class="space-y-4">
          {#each agentsStore.agents as a (a.id)}
            <div
              class="rounded-lg border border-border/40 bg-card p-4 transition-colors hover:bg-muted/30 dark:border-gray-700/50"
            >
              <div class="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
                <div class="flex min-w-0 flex-1 items-start gap-3">
                  <!-- 状态点 -->
                  <div class="mt-1.5">
                    {#if a.status?.type === 'Online'}
                      <span class="block h-2.5 w-2.5 rounded-full bg-green-500 shadow-sm"></span>
                    {:else if a.status?.type === 'Busy'}
                      <span class="block h-2.5 w-2.5 rounded-full bg-yellow-500 shadow-sm"></span>
                    {:else}
                      <span class="block h-2.5 w-2.5 rounded-full bg-gray-300 dark:bg-gray-600"></span>
                    {/if}
                  </div>

                  <div class="min-w-0 flex-1 space-y-1">
                    <div class="flex flex-wrap items-center gap-2">
                      <h3 class="truncate font-semibold text-foreground">{a.name}</h3>
                      <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs text-muted-foreground">{a.id}</code>
                      {#if a.version}
                        <Badge variant="secondary" class="text-xs font-normal">v{a.version}</Badge>
                      {/if}
                    </div>

                    <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
                      <div class="flex items-center gap-1">
                        <Server class="h-3 w-3" />
                        <span>{a.hostname}</span>
                      </div>
                      <div class="flex items-center gap-1">
                        <Activity class="h-3 w-3" />
                        <span>{a.status?.type === 'Online' ? '在线' : a.status?.type === 'Busy' ? '忙碌' : '离线'}</span
                        >
                      </div>
                      <div class="flex items-center gap-1">
                        <Clock class="h-3 w-3" />
                        <span>{formatHeartbeat(a.last_heartbeat)}</span>
                      </div>
                    </div>

                    <!-- Search Roots Display -->
                    {#if a.search_roots && a.search_roots.length > 0}
                      <div class="mt-2 flex flex-wrap gap-1">
                        {#each a.search_roots as root (root)}
                          <div
                            class="flex items-center gap-1 rounded bg-muted/50 px-1.5 py-0.5 text-[10px] text-muted-foreground"
                            title="Search Root"
                          >
                            <Folder class="h-3 w-3" />
                            <span class="font-mono">{root}</span>
                          </div>
                        {/each}
                      </div>
                    {/if}
                  </div>
                </div>
              </div>

              <!-- 标签管理 -->
              <div class="mt-4 flex flex-col gap-3 border-t border-border/40 pt-3 dark:border-gray-700/50">
                <div class="flex flex-wrap items-center gap-2">
                  <Tag class="h-3.5 w-3.5 text-muted-foreground" />
                  {#each a.tags || [] as t (`${a.id}-${t.key}=${t.value}`)}
                    <Badge
                      variant="outline"
                      class="gap-1 bg-indigo-50/50 text-indigo-700 hover:bg-indigo-100 dark:bg-indigo-900/20 dark:text-indigo-300 dark:hover:bg-indigo-900/40"
                    >
                      {t.key}={t.value}
                      <button
                        class="ml-1 rounded-full p-0.5 hover:bg-indigo-200/50 dark:hover:bg-indigo-800/50"
                        title="移除标签"
                        onclick={() => agentsStore.removeTag(a.id, t.key, t.value)}
                      >
                        <X class="h-3 w-3" />
                      </button>
                    </Badge>
                  {/each}
                  {#if !a.tags || a.tags.length === 0}
                    <span class="text-xs text-muted-foreground">暂无标签</span>
                  {/if}
                </div>

                <div class="flex flex-wrap items-center gap-2">
                  <Input
                    class="h-8 w-32 text-xs"
                    placeholder="key"
                    data-testid={`tag-key-${a.id}`}
                    bind:value={newTagKey[a.id]}
                  />
                  <Input
                    class="h-8 w-40 text-xs"
                    placeholder="value"
                    data-testid={`tag-value-${a.id}`}
                    bind:value={newTagValue[a.id]}
                    onkeydown={(e) => {
                      if (e.key === 'Enter') handleAddTag(a.id);
                    }}
                  />
                  <Button
                    size="sm"
                    variant="secondary"
                    class="h-8 text-xs"
                    onclick={() => handleAddTag(a.id)}
                    disabled={!newTagKey[a.id]?.trim() || !newTagValue[a.id]?.trim() || agentsStore.loading}
                  >
                    <Plus class="mr-1 h-3 w-3" />
                    添加标签
                  </Button>
                </div>
              </div>

              <!-- 日志设置（可展开） -->
              <div class="mt-4 border-t border-border/40 pt-2 dark:border-gray-700/50">
                <Button
                  variant="ghost"
                  size="sm"
                  class="h-8 w-full justify-between text-muted-foreground hover:text-foreground"
                  onclick={() => toggleLogSettings(a.id)}
                >
                  <span class="flex items-center gap-2">
                    <Settings class="h-3.5 w-3.5" />
                    日志设置
                  </span>
                  {#if expandedLogSettings[a.id]}
                    <ChevronUp class="h-4 w-4" />
                  {:else}
                    <ChevronDown class="h-4 w-4" />
                  {/if}
                </Button>

                {#if expandedLogSettings[a.id]}
                  <div class="mt-4 space-y-4 rounded-lg bg-muted/30 p-4">
                    {#if logConfigError[a.id]}
                      <Alert type="error" message={logConfigError[a.id]} />
                    {/if}

                    {#if logConfigSuccess[a.id]}
                      <Alert type="success" message={logConfigSuccess[a.id]} />
                    {/if}

                    {#if logConfigLoading[a.id] && !agentLogConfigs[a.id]}
                      <div class="py-4 text-center text-xs text-muted-foreground">加载中…</div>
                    {:else if agentLogConfigs[a.id]}
                      <div class="grid gap-4 sm:grid-cols-2">
                        <!-- 日志级别 -->
                        <div class="space-y-2">
                          <Label for="log-level-{a.id}" class="text-xs">日志级别</Label>
                          <select
                            id="log-level-{a.id}"
                            class="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm transition-colors focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
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
                        <div class="space-y-2">
                          <Label for="log-retention-{a.id}" class="text-xs">日志保留 (天)</Label>
                          <Input
                            id="log-retention-{a.id}"
                            type="number"
                            class="h-9"
                            bind:value={agentLogConfigs[a.id].retention_count}
                            min="1"
                            max="365"
                            disabled={a.status?.type !== 'Online'}
                          />
                        </div>

                        <!-- 日志路径（只读） -->
                        <div class="space-y-2 sm:col-span-2">
                          <Label for="log-dir-{a.id}" class="text-xs">日志路径</Label>
                          <Input
                            id="log-dir-{a.id}"
                            type="text"
                            class="h-9 bg-muted text-muted-foreground"
                            value={agentLogConfigs[a.id].log_dir}
                            disabled
                          />
                        </div>
                      </div>

                      <!-- 操作按钮 -->
                      <div class="flex items-center justify-between pt-2">
                        <div class="text-xs text-muted-foreground">
                          {#if a.status?.type !== 'Online'}
                            <span class="text-amber-600 dark:text-amber-400">Agent 离线，无法修改配置</span>
                          {:else}
                            修改日志级别立即生效，保留天数在下次滚动时生效
                          {/if}
                        </div>
                        <Button
                          size="sm"
                          onclick={() => handleSaveAgentLogConfig(a.id)}
                          disabled={a.status?.type !== 'Online' || logConfigLoading[a.id]}
                        >
                          {logConfigLoading[a.id] ? '保存中…' : '保存配置'}
                        </Button>
                      </div>
                    {/if}
                  </div>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </CardContent>
  </Card>
</div>
