<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { env } from '$env/dynamic/public';

  // 中文注释：查询字符串、结果列表、加载与错误状态
  let q = $state('');
  let results = $state<Array<Record<string, any>>>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  // 中文注释：用于取消请求的控制器
  let controller: AbortController | null = null;

  // 中文注释：启动流式搜索
  async function startSearch(query: string) {
    // 清理上一次的流
    controller?.abort();
    controller = new AbortController();
    const signal = controller.signal;

    results = [];
    loading = true;
    error = null;

    try {
      // 中文注释：API 基地址可通过 PUBLIC_API_BASE 配置，默认使用 /api/v1/logsearch
      const API_BASE = env.PUBLIC_API_BASE || '/api/v1/logsearch';
      const endpoint = `${API_BASE}/stream.ndjson`;

      const res = await fetch(endpoint, {
        method: 'POST',
        headers: {
          // 中文注释：声明期待 NDJSON
          Accept: 'application/x-ndjson',
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({ q: query }),
        signal
      });

      if (!res.ok || !res.body) {
        throw new Error(`服务端返回异常状态：${res.status}`);
      }

      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() ?? '';
        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed) continue;
          try {
            const obj = JSON.parse(trimmed);
            results = [...results, obj];
          } catch (e) {
            // 中文报错：单行解析失败不阻断整体
            console.error('解析 NDJSON 行失败：', e, trimmed);
          }
        }
      }

      // 中文注释：处理末尾残留（无换行结尾）
      const last = buffer.trim();
      if (last) {
        try {
          const obj = JSON.parse(last);
          results = [...results, obj];
        } catch (e) {
          console.error('解析 NDJSON 尾行失败：', e, last);
        }
      }
    } catch (e: any) {
      if (e?.name === 'AbortError') return; // 中文注释：主动取消不视为错误
      error = e?.message || '搜索过程中发生未知错误';
    } finally {
      loading = false;
    }
  }

  // 中文注释：从地址栏读取 ?q=，并在客户端启动搜索
  onMount(() => {
    const params = new URL(window.location.href).searchParams;
    const initial = (params.get('q') || '').trim();
    q = initial;
    if (initial) startSearch(initial);
  });

  onDestroy(() => {
    controller?.abort();
  });
</script>

<!-- 中文注释：页面标题与状态栏 -->
<div class="mx-auto max-w-5xl px-4 py-6">
  <h1 class="mb-4 text-xl font-semibold">搜索结果</h1>
  {#if q}
    <p class="mb-6 text-sm text-gray-600 dark:text-gray-300">关键词：<span class="font-mono">{q}</span></p>
  {:else}
    <p class="mb-6 text-sm text-gray-600 dark:text-gray-300">请输入关键词后回车进行搜索。</p>
  {/if}

  {#if loading}
    <div class="mb-4 text-sm text-blue-600 dark:text-blue-400">正在加载（流式传输中）…</div>
  {/if}

  {#if error}
    <div class="mb-4 rounded border border-red-300 bg-red-50 p-3 text-red-700 dark:border-red-800 dark:bg-red-950 dark:text-red-300">
      出错了：{error}
    </div>
  {/if}

  <!-- 中文注释：结果列表（流式追加） -->
  <div class="space-y-3">
    {#each results as item, i}
      <div class="rounded border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-3">
        <pre class="whitespace-pre-wrap break-words text-sm leading-relaxed">{JSON.stringify(item, null, 2)}</pre>
      </div>
    {/each}

    {#if !loading && !error && results.length === 0 && q}
      <div class="text-sm text-gray-500 dark:text-gray-400">没有匹配结果。</div>
    {/if}
  </div>
</div>

