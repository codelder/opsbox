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
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Card, CardHeader, CardTitle, CardDescription, CardContent } from '$lib/components/ui/card';
  import { Info } from 'lucide-svelte';

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

  <Card class="border-border/40 dark:border-gray-700/50">
    <CardHeader>
      <CardTitle>Server 日志设置</CardTitle>
      <CardDescription>配置 Server 的日志级别和保留策略</CardDescription>
    </CardHeader>
    <CardContent>
      {#if loading && !config}
        <div class="py-10 text-center text-sm text-muted-foreground">加载中…</div>
      {:else if config}
        <div class="space-y-6">
          <!-- 日志级别 -->
          <div class="space-y-3">
            <div class="space-y-1">
              <Label for="log-level" class="text-sm font-semibold">日志级别</Label>
              <p class="text-xs text-muted-foreground">控制日志输出的详细程度</p>
            </div>
            <select
              id="log-level"
              class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
              bind:value={config.level}
            >
              <option value="error">ERROR - 仅错误</option>
              <option value="warn">WARN - 警告及以上</option>
              <option value="info">INFO - 信息及以上（推荐）</option>
              <option value="debug">DEBUG - 调试及以上</option>
              <option value="trace">TRACE - 全部日志</option>
            </select>
          </div>

          <!-- 日志保留 -->
          <div class="space-y-3">
            <div class="space-y-1">
              <Label for="log-retention" class="text-sm font-semibold">日志保留</Label>
              <p class="text-xs text-muted-foreground">保留最近 N 天的日志文件</p>
            </div>
            <div class="flex items-center gap-2">
              <Input
                id="log-retention"
                type="number"
                bind:value={config.retention_count}
                min="1"
                max="365"
                class="flex-1"
              />
              <span class="text-sm">天</span>
            </div>
          </div>

          <!-- 日志路径（只读） -->
          <div class="space-y-3">
            <div class="space-y-1">
              <Label for="log-dir" class="text-sm font-semibold">日志路径</Label>
              <p class="text-xs text-muted-foreground">日志文件存储位置（启动时指定）</p>
            </div>
            <Input id="log-dir" type="text" class="bg-muted text-muted-foreground" value={config.log_dir} disabled />
          </div>

          <!-- 操作按钮 -->
          <div class="flex items-center gap-3 pt-2">
            <Button onclick={handleSave} disabled={loading}>
              {loading ? '保存中…' : '保存'}
            </Button>
            <Button variant="outline" onclick={loadConfig} disabled={loading}>重置</Button>
          </div>
        </div>
      {/if}
    </CardContent>
  </Card>

  <!-- 提示信息 -->
  <Card class="border-border/40 bg-blue-50/30 dark:border-gray-700/50 dark:bg-blue-950/20">
    <CardContent class="p-4">
      <div class="flex items-start gap-3">
        <Info class="mt-0.5 h-5 w-5 shrink-0 text-blue-600 dark:text-blue-400" />
        <div class="min-w-0 flex-1 space-y-2 text-sm">
          <p class="font-semibold text-foreground">提示</p>
          <ul class="list-disc space-y-1 pl-5 text-sm text-muted-foreground">
            <li>修改日志级别会立即生效，无需重启服务</li>
            <li>修改日志保留数量会在下次日志滚动时生效</li>
            <li>DEBUG 和 TRACE 级别会产生大量日志，建议仅在排查问题时使用</li>
          </ul>
        </div>
      </div>
    </CardContent>
  </Card>
</div>
