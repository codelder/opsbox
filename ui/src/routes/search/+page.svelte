<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { env } from '$env/dynamic/public';

  // 中文注释：查询字符串、结果列表、加载与错误状态
  let q = $state('');
  // 结构类型：与后端 NDJSON 对齐
  type JsonLine = { no: number; text: string };
  type JsonChunk = { range: [number, number] | { 0: number; 1: number }; lines: JsonLine[] };
  type SearchJsonResult = { path: string; keywords: string[]; chunks: JsonChunk[] };
  let results = $state<SearchJsonResult[]>([]);
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
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { name?: string; message?: string }) : {};
      if (err.name === 'AbortError') return; // 主动取消
      error = err.message || '搜索过程中发生未知错误';
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

  // 中文注释：长行截断（优先保留首次命中关键字），支持左右“省略号”点击展开
  function snippet(
    line: string,
    keywords: string[],
    opts: { max?: number; context?: number } = {}
  ): { html: string; leftTrunc: boolean; rightTrunc: boolean } {
    const max = opts.max ?? 540;
    const ctx = opts.context ?? 230;
    if (line.length <= max) {
      return { html: highlight(line, keywords), leftTrunc: false, rightTrunc: false };
    }
    const kws = (keywords || []).filter((k) => k && k.length > 0);
    let firstIdx = -1;
    let firstLen = 0;
    for (const kw of kws) {
      const idx = line.indexOf(kw);
      if (idx !== -1 && (firstIdx === -1 || idx < firstIdx)) {
        firstIdx = idx;
        firstLen = kw.length;
      }
    }
    let start = 0;
    let end = 0;
    if (firstIdx >= 0) {
      start = Math.max(0, firstIdx - ctx);
      end = Math.min(line.length, firstIdx + firstLen + ctx);
      if (end - start < max) {
        const deficit = max - (end - start);
        const addLeft = Math.min(start, Math.floor(deficit / 2));
        const addRight = Math.min(line.length - end, deficit - addLeft);
        start -= addLeft;
        end += addRight;
      }
    } else {
      start = 0;
      end = max;
    }
    const leftTrunc = start > 0;
    const rightTrunc = end < line.length;
    const slice = line.slice(start, end);
    return { html: highlight(slice, keywords), leftTrunc, rightTrunc };
  }

  // 每个结果的 UI 状态（折叠、展开所有匹配、单行展开）
  const collapsedFiles = new SvelteSet<number>();
  const expandedAllMatches = new SvelteSet<number>();
  const expandedLines = new SvelteSet<string>();
  const lineKey = (fileIdx: number, chunkIdx: number, lineIdx: number) => `${fileIdx}-${chunkIdx}-${lineIdx}`;
  function isFileCollapsed(i: number) {
    return collapsedFiles.has(i);
  }
  function toggleFileCollapsed(i: number) {
    if (collapsedFiles.has(i)) collapsedFiles.delete(i);
    else collapsedFiles.add(i);
  }
  function isFileShowAll(i: number) {
    return expandedAllMatches.has(i);
  }
  function toggleFileShowAll(i: number) {
    if (expandedAllMatches.has(i)) expandedAllMatches.delete(i);
    else expandedAllMatches.add(i);
  }
  function isLineExpanded(key: string) {
    return expandedLines.has(key);
  }
  function expandLine(key: string) {
    expandedLines.add(key);
  }

  // 扁平化为行数组，便于“仅显示前6行”
  function flattenLines(item: SearchJsonResult): Array<{ no: number; text: string; _ci: number; _li: number }> {
    const arr: Array<{ no: number; text: string; _ci: number; _li: number }> = [];
    (item?.chunks || []).forEach((chunk: JsonChunk, ci: number) => {
      (chunk?.lines || []).forEach((ln: JsonLine, li: number) =>
        arr.push({ no: ln.no, text: ln.text, _ci: ci, _li: li })
      );
    });
    return arr;
  }
  function totalMatches(item: SearchJsonResult): number {
    return flattenLines(item).length;
  }
  function visibleLines(
    item: SearchJsonResult,
    fileIdx: number
  ): Array<{ no: number; text: string; _ci: number; _li: number }> {
    const flat = flattenLines(item);
    if (expandedAllMatches.has(fileIdx)) return flat;
    return flat.slice(0, Math.min(6, flat.length));
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
    // 启动阶段立即标记加载中（即使暂未收到任何字节）
    loading = true;

    try {
      const API_BASE = env.PUBLIC_API_BASE || '/api/v1/logsearch';
      const endpoint = `${API_BASE}/stream.ndjson`;
      const payload: { q: string } = { q: query };

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
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { name?: string; message?: string }) : {};
      if (err.name === 'AbortError') return; // 主动取消不视为错误
      error = err.message || '搜索过程中发生未知错误';
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
<div class="mx-auto max-w-[1560px] px-4 py-10">
  <div class="flex items-center justify-between md:mask-b-from-10">
    <label
      for="search"
      id="logo-label"
      class="mr-10 mb-4 block text-2xl font-extrabold tracking-[-0.25em] italic antialiased select-none md:text-4xl"
    >
      <span class="text-blue-600">L</span>
      <span class="text-red-600">o</span>
      <span class="text-yellow-500">G</span>
      <span class="text-green-600">o</span>
      <span class="text-blue-600">o</span>
      <span class="text-red-600">g</span>
      <span class="text-yellow-500">l</span>
      <span class="text-green-600">e</span>
    </label>

    <form class="mb-4 flex flex-1 gap-2" onsubmit={handleSubmit}>
      <input
        class="h-12 flex-1 rounded-2xl border border-gray-300 bg-white px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-900"
        disabled={loading}
        bind:value={q}
      />
    </form>
  </div>

  {#if q}
    <p class="mb-4 text-sm text-gray-600 dark:text-gray-300">关键词：<span class="font-mono">{q}</span></p>
  {:else}
    <p class="mb-6 text-sm text-gray-600 dark:text-gray-300">请输入关键词后回车进行搜索。</p>
  {/if}

  <div class="mb-2 text-sm text-blue-600 dark:text-blue-400">{loading}</div>

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
  <div class="mt-4 text-sm text-blue-600 dark:text-blue-400">
    {#if loading}正在加载…{/if}
  </div>

  <div class="mt-10 space-y-6">
    {#each results as item, i (item.path + '-' + i)}
      {#if item && item.path && item.chunks}
        <div class="overflow-hidden rounded border border-gray-200 dark:border-gray-700">
          <!-- 结果头：文件路径（可折叠） -->
          <button
            type="button"
            class="flex w-full items-center justify-between border-b border-gray-200 bg-gray-50 px-3 py-2 text-left text-xs text-gray-700 hover:bg-gray-100 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-300 dark:hover:bg-gray-800"
            onclick={() => toggleFileCollapsed(i)}
          >
            <div class="truncate font-mono">{item.path}</div>
            <div class="ml-2 flex shrink-0 items-center gap-3">
              {#if item.keywords?.length}
                <span class="hidden text-gray-500 sm:inline dark:text-gray-400">{item.keywords.join(', ')}</span>
              {/if}
              <span class="text-gray-400">{isFileCollapsed(i) ? '▶' : '▼'}</span>
            </div>
          </button>

          {#if !isFileCollapsed(i)}
            <!-- 代码块区域：行号 + 内容（默认仅前6行） -->
            <div class="bg-white dark:bg-gray-800">
              {#each visibleLines(item, i) as ln (i + '-' + ln._ci + '-' + ln._li)}
                <div class="grid grid-cols-[72px_1fr] gap-0 font-mono text-[13px] leading-[20px]">
                  <div
                    class="border-r border-gray-100 bg-gray-50 px-3 py-0.5 text-right text-gray-400 select-none dark:border-gray-700 dark:bg-gray-900 dark:text-gray-500"
                  >
                    {ln.no}
                  </div>
                  <div class="px-3 py-0.5 break-all whitespace-pre-wrap">
                    {#if isLineExpanded(lineKey(i, ln._ci, ln._li))}
                      <span>{@html highlight(ln.text, item.keywords)}</span>
                    {:else}
                      {#key i + '-' + ln._ci + '-' + ln._li + '-snippet'}
                        {@const sn = snippet(ln.text, item.keywords)}
                        {#if sn.leftTrunc}
                          <button
                            type="button"
                            class="text-blue-600 hover:underline"
                            onclick={() => expandLine(lineKey(i, ln._ci, ln._li))}>&hellip;</button
                          >
                        {/if}
                        <span>{@html sn.html}</span>
                        {#if sn.rightTrunc}
                          <button
                            type="button"
                            class="text-blue-600 hover:underline"
                            onclick={() => expandLine(lineKey(i, ln._ci, ln._li))}>&hellip;</button
                          >
                        {/if}
                      {/key}
                    {/if}
                  </div>
                </div>
              {/each}
            </div>

            <!-- 卡片 foot：展开更多 matches -->
            {#if totalMatches(item) > visibleLines(item, i).length}
              <div
                class="border-t border-gray-200 bg-gray-50 px-3 py-2 text-xs text-gray-600 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-300"
              >
                <button class="text-blue-600 hover:underline" onclick={() => toggleFileShowAll(i)}>
                  {#if isFileShowAll(i)}
                    Show fewer matches
                  {:else}
                    显示剩余的 {totalMatches(item) - visibleLines(item, i).length} 行
                  {/if}
                </button>
              </div>
            {/if}
          {/if}
        </div>
      {:else}
        <!-- 兼容其他对象：兜底显示 -->
        <div class="rounded border border-gray-200 bg-white p-3 dark:border-gray-700 dark:bg-gray-800">
          <pre class="text-sm leading-relaxed break-all whitespace-pre-wrap">{JSON.stringify(item, null, 2)}</pre>
        </div>
      {/if}
    {/each}

    {#if !loading && !error && results.length === 0 && q && !hasMore}
      <div class="text-sm text-gray-500 dark:text-gray-400">没有匹配结果。</div>
    {/if}
  </div>

  <!-- 中文注释：分页控制按钮 -->
  <div class="mt-6 flex items-center gap-3">
    {#if hasMore}
      <button
        class="rounded bg-gray-700 px-3 py-2 text-sm text-white disabled:opacity-50"
        onclick={loadMore}
        disabled={loading}>{loading ? '加载中…' : '加载更多'}</button
      >
    {:else if results.length > 0}
      <span class="text-sm text-gray-500 dark:text-gray-400">已到结尾。</span>
    {/if}
  </div>
</div>
