<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { env } from '$env/dynamic/public';

  // 中文注释：查询字符串、结果列表、加载与错误状态
  let q = $state('');
  let results = $state<Array<Record<string, any>>>([]);
  let loading = $state(false); // 当前是否正在读取一批
  let error = $state<string | null>(null);

  // 中文注释：分页控制（每批 20 条）
  const PAGE_SIZE = 20;
  let hasMore = $state(true); // 是否还有更多可读
  let paused = $state(false); // 是否处于“暂停等待加载更多”状态

  // 中文注释：流读取持久状态
  let controller: AbortController | null = null; // 取消当前请求
  let reader: ReadableStreamDefaultReader<Uint8Array> | null = null; // 当前响应的 reader
  let decoder: TextDecoder | null = null; // 统一的 TextDecoder
  let buffer = $state(''); // 分片缓冲（可能出现半行）

  // 中文注释：读取一批（最多 PAGE_SIZE 条）。读满或读到流结束就停止。
  async function readBatch(maxItems = PAGE_SIZE) {
    if (!reader) return;
    loading = true;
    paused = false;
    let produced = 0;
    try {
      while (produced < maxItems && reader) {
        const { done, value } = await reader.read();
        if (done) {
          // 处理尾部残留
          const last = buffer.trim();
          if (last) {
            try {
              const obj = JSON.parse(last);
              results = [...results, obj];
            } catch (e) {
              console.error('解析 NDJSON 尾行失败：', e, last);
            }
            buffer = '';
          }
          hasMore = false;
          break;
        }
        const dec = decoder ?? (decoder = new TextDecoder());
        buffer += dec.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() ?? '';
        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed) continue;
          try {
            const obj = JSON.parse(trimmed);
            results = [...results, obj];
            produced += 1;
            if (produced >= maxItems) {
              break;
            }
          } catch (e) {
            // 中文报错：单行解析失败不阻断整体
            console.error('解析 NDJSON 行失败：', e, trimmed);
          }
        }
      }
    } catch (e: any) {
      if (e?.name === 'AbortError') return; // 主动取消
      error = e?.message || '搜索过程中发生未知错误';
      hasMore = false;
      reader = null;
    } finally {
      loading = false;
      // 若还有更多但已达到本批上限，则进入暂停态，等待“加载更多”
      if (hasMore && produced >= maxItems) {
        paused = true;
      }
    }
  }

  // 中文注释：转义与高亮（模仿 GitHub 高亮效果，使用 <mark>）
  function escapeHtml(s: string): string {
    return s
      .replaceAll('&', '&amp;')
      .replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;')
      .replaceAll('"', '&quot;')
      .replaceAll("'", '&#39;');
  }
  function escapeRegExp(s: string): string {
    return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  }
  function highlight(line: string, keywords: string[]): string {
    let out = escapeHtml(line);
    const kws = (keywords || []).filter((k) => k && k.length > 0);
    for (const kw of kws) {
      const re = new RegExp(escapeRegExp(kw), 'g');
      out = out.replace(re, (m) => `<mark>${escapeHtml(m)}</mark>`);
    }
    return out;
  }

  // 中文注释：启动流式搜索（重置状态并读取首批）
  async function startSearch(query: string) {
    // 终止上一次
    controller?.abort();
    controller = new AbortController();
    const signal = controller.signal;

    // 重置状态
    results = [];
    error = null;
    hasMore = true;
    paused = false;
    buffer = '';
    decoder = null;
    reader = null;

    try {
      const API_BASE = env.PUBLIC_API_BASE || '/api/v1/logsearch';
      const endpoint = `${API_BASE}/stream.ndjson`;
      const payload: Record<string, any> = { q: query };

      const res = await fetch(endpoint, {
        method: 'POST',
        headers: {
          Accept: 'application/x-ndjson',
          'Content-Type': 'application/json'
        },
        body: JSON.stringify(payload),
        signal
      });

      if (!res.ok || !res.body) {
        throw new Error(`服务端返回异常状态：${res.status}`);
      }

      reader = res.body.getReader();
      await readBatch(PAGE_SIZE);
    } catch (e: any) {
      if (e?.name === 'AbortError') return; // 主动取消不视为错误
      error = e?.message || '搜索过程中发生未知错误';
      hasMore = false;
      reader = null;
    }
  }

  // 中文注释：继续读取下一批
  async function loadMore() {
    if (loading || !hasMore) return;
    await readBatch(PAGE_SIZE);
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

  // 中文注释：表单提交（支持在页面内触发新搜索）
  function handleSubmit(e: Event) {
    e.preventDefault();
    const next = q.trim();
    if (!next) return;
    startSearch(next);
  }
</script>

<!-- 中文注释：页面标题与状态栏 -->
<div class="mx-auto max-w-5xl px-4 py-6">
  <h1 class="mb-4 text-xl font-semibold">搜索结果</h1>

  <form class="mb-4 flex gap-2" on:submit|preventDefault={handleSubmit}>
    <input
      class="flex-1 rounded border border-gray-300 bg-white px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-900"
      placeholder="输入关键词（可包含 dt:YYYYMMDD / fdt:YYYYMMDD / tdt:YYYYMMDD）"
      bind:value={q}
    />
    <button
      class="rounded bg-blue-600 px-3 py-2 text-sm text-white disabled:opacity-50"
      disabled={loading}
    >搜索</button>
  </form>

  {#if q}
    <p class="mb-4 text-sm text-gray-600 dark:text-gray-300">关键词：<span class="font-mono">{q}</span></p>
  {:else}
    <p class="mb-6 text-sm text-gray-600 dark:text-gray-300">请输入关键词后回车进行搜索。</p>
  {/if}

  {#if loading}
    <div class="mb-2 text-sm text-blue-600 dark:text-blue-400">正在加载（本批）…</div>
  {/if}

  {#if paused && hasMore}
    <div class="mb-2 text-sm text-gray-600 dark:text-gray-300">已加载 {PAGE_SIZE} 条，已暂停。</div>
  {/if}

  {#if error}
    <div
      class="mb-4 rounded border border-red-300 bg-red-50 p-3 text-red-700 dark:border-red-800 dark:bg-red-950 dark:text-red-300"
    >
      出错了：{error}
    </div>
  {/if}

  <!-- 中文注释：结果列表（GitHub 风格） -->
  <div class="space-y-6">
    {#each results as item, i}
      {#if item && item.path && item.chunks}
        <div class="rounded border border-gray-200 dark:border-gray-700 overflow-hidden">
          <!-- 结果头：文件路径 -->
          <div class="flex items-center justify-between bg-gray-50 px-3 py-2 text-xs text-gray-700 dark:bg-gray-900 dark:text-gray-300 border-b border-gray-200 dark:border-gray-700">
            <div class="truncate font-mono">{item.path}</div>
            {#if item.keywords?.length}
              <div class="ml-2 shrink-0 text-gray-500 dark:text-gray-400">{item.keywords.join(', ')}</div>
            {/if}
          </div>

          <!-- 代码块区域：行号 + 内容 -->
          <div class="bg-white dark:bg-gray-800">
            {#each item.chunks as chunk}
              <div class="border-b border-gray-100 dark:border-gray-700">
                {#each chunk.lines as ln}
                  <div class="grid grid-cols-[72px_1fr] gap-0 font-mono text-[13px] leading-[20px]">
                    <div class="select-none text-right px-3 py-0.5 text-gray-400 dark:text-gray-500 bg-gray-50 dark:bg-gray-900 border-r border-gray-100 dark:border-gray-700">{ln.no}</div>
                    <div class="px-3 py-0.5 whitespace-pre-wrap break-words">{@html highlight(ln.text, item.keywords)}</div>
                  </div>
                {/each}
              </div>
            {/each}
          </div>
        </div>
      {:else}
        <!-- 兼容其他对象：兜底显示 -->
        <div class="rounded border border-gray-200 bg-white p-3 dark:border-gray-700 dark:bg-gray-800">
          <pre class="text-sm leading-relaxed break-all whitespace-pre-wrap">{JSON.stringify(item, null, 2)}</pre>
        </div>
      {/if}
    {/each}

    {#if !loading && !error && results.length === 0 && q}
      <div class="text-sm text-gray-500 dark:text-gray-400">没有匹配结果。</div>
    {/if}
  </div>

  <!-- 中文注释：分页控制按钮 -->
  <div class="mt-6 flex items-center gap-3">
    {#if hasMore}
      <button
        class="rounded bg-gray-700 px-3 py-2 text-sm text-white disabled:opacity-50"
        on:click|preventDefault={loadMore}
        disabled={loading}
      >{loading ? '加载中…' : '加载更多'}</button>
    {:else}
      {#if results.length > 0}
        <span class="text-sm text-gray-500 dark:text-gray-400">已到结尾。</span>
      {/if}
    {/if}
  </div>
</div>
