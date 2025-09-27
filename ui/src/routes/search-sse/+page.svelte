<script lang="ts">
  // 与 NDJSON 页面一致的外观与交互，仅将数据源切换为 SSE
  import { SvelteSet } from 'svelte/reactivity';
  import { env } from '$env/dynamic/public';

  // 查询与会话
  let q = $state('');
  let sid = $state(''); // 从 SSE 首条 meta 事件获取
  let loading = $state(false);
  let error = $state<string | null>(null);

  // 结果数据结构与列表（与 NDJSON 保持一致）
  type JsonLine = { no: number; text: string };
  type JsonChunk = { range: [number, number] | { 0: number; 1: number }; lines: JsonLine[] };
  type SearchJsonResult = { path: string; keywords: string[]; chunks: JsonChunk[] };
  let results = $state<SearchJsonResult[]>([]);

  // SSE 实例
  let es: EventSource | null = null;

  // 高亮与辅助函数（与 NDJSON 一致）
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

    if (start > 0 && line[start] !== ' ' && line[start - 1] !== ' ') {
      const prevSpace = line.lastIndexOf(' ', start);
      if (prevSpace >= 0 && start - prevSpace < 20) {
        start = prevSpace;
      }
    }

    const leftTrunc = start > 0;
    const rightTrunc = end < line.length;
    const slice = line.slice(start, end);
    return { html: highlight(slice, keywords), leftTrunc, rightTrunc };
  }

  // UI 状态（与 NDJSON 保持一致）
  const collapsedFiles = new SvelteSet<number>();
  const expandedAllMatches = new SvelteSet<number>();
  const expandedLines = new SvelteSet<string>();
  const lineKey = (fileIdx: number, chunkIdx: number, lineIdx: number) => `${fileIdx}-${chunkIdx}-${lineIdx}`;
  function isFileCollapsed(i: number) { return collapsedFiles.has(i); }
  function toggleFileCollapsed(i: number) {
    if (collapsedFiles.has(i)) collapsedFiles.delete(i); else collapsedFiles.add(i);
  }
  function isFileShowAll(i: number) { return expandedAllMatches.has(i); }
  function toggleFileShowAll(i: number) {
    const wasExpanded = expandedAllMatches.has(i);
    if (wasExpanded) {
      expandedAllMatches.delete(i);
      setTimeout(() => {
        const cardElement = document.querySelector(`[data-result-card="${i}"]`);
        if (cardElement) {
          cardElement.scrollIntoView({ behavior: 'smooth', block: 'start', inline: 'nearest' });
          cardElement.classList.add('highlight-card');
          setTimeout(() => { cardElement.classList.remove('highlight-card'); }, 2000);
        }
      }, 100);
    } else {
      expandedAllMatches.add(i);
    }
  }
  function isLineExpanded(key: string) { return expandedLines.has(key); }
  function expandLine(key: string) { expandedLines.add(key); }

  // 结果扁平化（与 NDJSON 保持一致）
  function flattenLines(item: SearchJsonResult): Array<{ no: number; text: string; _ci: number; _li: number }> {
    const arr: Array<{ no: number; text: string; _ci: number; _li: number }> = [];
    (item?.chunks || []).forEach((chunk: JsonChunk, ci: number) => {
      (chunk?.lines || []).forEach((ln: JsonLine, li: number) =>
        arr.push({ no: ln.no, text: ln.text, _ci: ci, _li: li })
      );
    });
    return arr;
  }
  function totalMatches(item: SearchJsonResult): number { return flattenLines(item).length; }
  function visibleLines(item: SearchJsonResult, fileIdx: number) {
    const flat = flattenLines(item);
    if (expandedAllMatches.has(fileIdx)) return flat;
    return flat.slice(0, Math.min(7, flat.length));
  }

  // 路径解析（与 NDJSON 保持一致）
  function splitPath(full: string): { bucket: string | null; tar: string; inner: string | null } {
    const idx = full.indexOf(':');
    const tarFull = idx >= 0 ? full.slice(full.indexOf('/'), idx) : full;
    const inner = idx >= 0 ? full.slice(idx + 1) : null;
    const bucket = full.slice(0, full.indexOf('/', 1));
    return { bucket, tar: tarFull, inner };
  }

  // 启动搜索（SSE）
  function startSearch(query: string) {
    // 关闭旧连接
    if (es) { es.close(); es = null; }

    // 重置状态
    results = [];
    error = null;
    sid = '';
    loading = true;

    const API_BASE = env.PUBLIC_API_BASE || '/api/v1/logsearch';
    const url = `${API_BASE}/stream.s3.sse?q=${encodeURIComponent(query)}&context=3`;
    const src = new EventSource(url);
    es = src;

    src.onmessage = (ev: MessageEvent) => {
      try {
        const data = ev.data as string;
        if (!data) return;
        const obj = JSON.parse(data);
        if (obj && obj.type === 'meta' && obj.sid) {
          sid = obj.sid as string;
        } else if (obj && obj.path) {
          // 结果项
          results = [...results, obj as SearchJsonResult];
        }
      } catch (e) {
        console.error('解析 SSE 数据失败：', e);
      } finally {
        loading = false;
      }
    };

    src.onerror = (_ev: Event) => {
      // 连接中断或服务端结束
      loading = false;
      if (es === src) {
        try { src.close(); } catch {}
        es = null;
      }
    };
  }

  // 初始化：从 URL 读 q 并启动 SSE
  let searchInit = $state(false);
  $effect(() => {
    if (searchInit) return;
    searchInit = true;
    const params = new URL(window.location.href).searchParams;
    const initial = (params.get('q') || '').trim();
    q = initial;
    if (initial) startSearch(initial);
  });

  // 页面卸载时关闭 SSE
  $effect(() => {
    return () => {
      if (es) { try { es.close(); } catch {} es = null; }
    };
  });

  // 表单提交
  function handleSubmit(e: Event) {
    e.preventDefault();
    const next = q.trim();
    if (!next) return;
    startSearch(next);
  }
</script>

<!-- 页面结构与样式：完整复用 NDJSON 页 -->
<div class="min-h-screen bg-gradient-to-br from-slate-100 to-gray-200 dark:from-gray-900 dark:to-gray-800">
  <div class="mx-auto max-w-[1560px] px-4 py-8">
    <!-- 顶部区域 -->
    <div class="mb-12">
      <div class="mb-8 text-center">
        <label for="search" id="logo-label" class="inline-block transform cursor-pointer text-4xl font-extrabold tracking-[-0.25em] italic antialiased transition-transform duration-300 select-none hover:scale-105 md:text-6xl">
          <span class="text-blue-600 drop-shadow-sm">L</span>
          <span class="text-red-600 drop-shadow-sm">o</span>
          <span class="text-yellow-500 drop-shadow-sm">g</span>
          <span class="text-green-600 drop-shadow-sm">s</span>
          <span class="text-blue-600 drop-shadow-sm">e</span>
          <span class="text-red-600 drop-shadow-sm">e</span>
          <span class="text-yellow-500 drop-shadow-sm">k</span>
        </label>
        <p class="mt-3 text-lg font-medium text-gray-600 dark:text-gray-300">快速搜索和浏览日志文件（SSE）</p>
      </div>

      <!-- 搜索框 -->
      <form class="mx-auto max-w-4xl" onsubmit={handleSubmit}>
        <div class="group relative">
          <div class="pointer-events-none absolute inset-y-0 left-0 z-10 flex items-center pl-4">
            <svg class="h-6 w-6 text-gray-400 transition-colors duration-200 group-focus-within:text-blue-500" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m21 21-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
          </div>
          <input id="search" class="h-16 w-full rounded-3xl border-0 bg-white/90 py-4 pr-16 pl-14 text-lg placeholder-gray-500 shadow-2xl ring-1 shadow-gray-200/50 ring-gray-200/50 backdrop-blur-sm transition-all duration-300 hover:shadow-blue-200/25 hover:ring-blue-300/50 focus:bg-white focus:ring-2 focus:shadow-blue-300/30 focus:ring-blue-500 focus:outline-none dark:bg-gray-800/90 dark:text-white dark:placeholder-gray-400 dark:shadow-gray-900/30 dark:ring-gray-600/50 dark:hover:ring-blue-400/50 dark:focus:ring-blue-400" disabled={loading} bind:value={q} placeholder="输入查询串或自然语言搜索…" autocomplete="off" />
          {#if loading}
            <div class="absolute inset-y-0 right-0 flex items-center pr-5">
              <div class="h-7 w-7 animate-spin rounded-full border-2 border-blue-600 border-t-transparent"></div>
            </div>
          {:else if q}
            <button type="button" class="absolute inset-y-0 right-0 flex items-center pr-5 text-gray-400 transition-colors duration-200 hover:text-gray-600" onclick={() => { q = ''; results = []; }} aria-label="清除搜索内容">
              <svg class="h-6 w-6" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          {/if}
        </div>
      </form>
    </div>

    <!-- 错误提示 -->
    {#if error}
      <div class="mx-auto mb-6 max-w-md text-center">
        <div class="rounded-xl bg-red-50 p-4 shadow-lg ring-1 ring-red-200 dark:bg-red-900/20 dark:ring-red-800/50">
          <div class="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-red-100 dark:bg-red-900/50">
            <svg class="h-6 w-6 text-red-600 dark:text-red-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.732 16.5c-.77.833.192 2.5 1.732 2.5z" />
            </svg>
          </div>
          <h3 class="mt-2 text-base font-semibold text-red-900 dark:text-red-200">加载出错</h3>
          <p class="mt-1 text-sm text-red-700 dark:text-red-300">{error}</p>
        </div>
      </div>
    {/if}

    <!-- 结果列表（与 NDJSON 一致） -->
    <div class="space-y-8">
      {#each results as item, i (item.path + '-' + i)}
        {#if item && item.path && item.chunks}
          <div class="group overflow-hidden rounded-2xl border border-white/60 bg-white/95 shadow-xl shadow-slate-300/40 backdrop-blur-sm transition-all duration-300 hover:shadow-2xl hover:shadow-slate-400/50 dark:border-gray-700/50 dark:bg-gray-800/80 dark:shadow-gray-900/20 dark:hover:shadow-gray-900/30" data-result-card={i}>
            <!-- 结果头 -->
            <button type="button" class="w-full border-b border-slate-200 bg-gradient-to-r from-slate-50 to-gray-100 px-6 py-6 text-left text-sm text-slate-700 transition-all duration-200 hover:from-slate-100 hover:to-slate-200 focus:ring-2 focus:ring-blue-500/20 focus:outline-none focus:ring-inset dark:border-gray-700/50 dark:from-gray-800/50 dark:to-gray-700/50 dark:text-gray-300 dark:hover:from-gray-700/50 dark:hover:to-gray-600/50" onclick={() => toggleFileCollapsed(i)}>
              <span class="flex w-full flex-col gap-3 md:flex-row md:items-start md:justify-between">
                <span class="min-w-0 flex-1">
                  {#if splitPath(item.path).inner}
                    <span class="mb-2 flex items-start gap-2">
                      <span role="link" tabindex="0" class="group/link cursor-pointer font-mono text-base leading-tight font-semibold text-slate-900 transition-colors duration-200 hover:text-blue-700 md:text-lg lg:text-xl dark:text-gray-100 dark:hover:text-blue-300" title={splitPath(item.path).inner}
                        onclick={(e) => {
                          e.stopPropagation();
                          const base = '/view';
                          const url = `${base}?sid=${encodeURIComponent(sid)}&file=${encodeURIComponent(item.path)}`;
                          window.open(url, '_blank', 'noopener');
                        }}
                        onkeydown={(e) => {
                          if (e.key === 'Enter' || e.key === ' ') {
                            e.preventDefault(); e.stopPropagation();
                            const base = '/view';
                            const url = `${base}?sid=${encodeURIComponent(sid)}&file=${encodeURIComponent(item.path)}`;
                            window.open(url, '_blank', 'noopener');
                          }
                        }}
                      >
                        <span class="line-clamp-2 group-hover/link:underline md:line-clamp-1">{splitPath(item.path).inner}</span>
                      </span>
                      <svg class="mt-1 h-4 w-4 shrink-0 text-blue-600 opacity-0 transition-opacity duration-200 group-hover/link:opacity-100 dark:text-blue-400" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                      </svg>
                    </span>
                  {:else}
                    <span class="mb-2">
                      <span class="font-mono text-base leading-tight font-semibold text-slate-900 md:text-lg dark:text-gray-100">{item.path}</span>
                    </span>
                  {/if}

                  <span class="flex flex-wrap items-center gap-2 text-xs text-slate-600 dark:text-gray-400">
                    {#if splitPath(item.path).bucket}
                      <span class="inline-flex items-center rounded-full bg-blue-50 px-2.5 py-1 text-[11px] font-medium text-blue-700 ring-1 ring-blue-200 dark:bg-blue-900/30 dark:text-blue-300 dark:ring-blue-800">
                        <svg class="mr-1 h-3 w-3" fill="currentColor" viewBox="0 0 20 20">
                          <path d="M3 4a1 1 0 011-1h3a1 1 0 011 1v3a1 1 0 01-1 1H4a1 1 0 01-1-1V4zM3 10a1 1 0 011-1h3a1 1 0 011 1v3a1 1 0 01-1 1H4a1 1 0 01-1-1v-3zM10 4a1 1 0 011-1h3a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1V4zM10 10a1 1 0 011-1h3a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-3z" />
                        </svg>
                        bucket {splitPath(item.path).bucket}
                      </span>
                    {/if}

                    <span class="inline-flex items-center rounded-md bg-gray-100 px-2.5 py-1 text-[11px] font-medium text-gray-700 ring-1 ring-gray-200 dark:bg-gray-700/50 dark:text-gray-300 dark:ring-gray-600">
                      <svg class="mr-1 h-3 w-3" fill="currentColor" viewBox="0 0 20 20">
                        <path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z" />
                      </svg>
                      {splitPath(item.path).tar}
                    </span>

                    {#if item.keywords?.length}
                      <span class="flex flex-wrap items-center gap-1.5">
                        {#each item.keywords.slice(0, 4) as keyword (keyword)}
                          <span class="inline-flex items-center rounded-md bg-yellow-50 px-2 py-0.5 text-[11px] font-medium text-yellow-800 ring-1 ring-yellow-600/20 dark:bg-yellow-900/30 dark:text-yellow-300 dark:ring-yellow-500/20">{keyword}</span>
                        {/each}
                        {#if item.keywords.length > 4}
                          <span class="text-[11px] text-gray-500 dark:text-gray-400">+{item.keywords.length - 4}</span>
                        {/if}
                      </span>
                    {/if}

                    <span class="inline-flex items-center rounded-md bg-green-50 px-2 py-0.5 text-[11px] font-medium text-green-700 ring-1 ring-green-200 dark:bg-green-900/30 dark:text-green-300 dark:ring-green-800">
                      <svg class="mr-1 h-3 w-3" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4" />
                      </svg>
                      {totalMatches(item)} 行匹配
                    </span>
                  </span>
                </span>

                <!-- 右侧：折叠箭头（与标题对齐） -->
                <span class="mt-2 flex shrink-0 items-center md:mt-0">
                  <span class="flex h-8 w-8 items-center justify-center rounded-full bg-gray-200/60 transition-colors duration-200 group-hover:bg-gray-300/60 dark:bg-gray-600/50 dark:group-hover:bg-gray-500/50">
                    <svg class="h-4 w-4 text-gray-700 transition-transform duration-200 {isFileCollapsed(i) ? '' : 'rotate-180'} dark:text-gray-200" viewBox="0 0 24 24" stroke="currentColor">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                    </svg>
                  </span>
                </span>
              </span>
            </button>

            {#if !isFileCollapsed(i)}
              <div class="overflow-hidden bg-gradient-to-r from-slate-50 to-white transition-all duration-500 ease-in-out dark:from-gray-900/50 dark:to-gray-800/50">
                {#each visibleLines(item, i) as ln (i + '-' + ln._ci + '-' + ln._li)}
                  <div class="group/line grid grid-cols-[80px_1fr] gap-0 font-mono text-sm leading-[24px] transition-colors duration-150 hover:bg-blue-50/40 dark:hover:bg-blue-900/10">
                    <div class="border-r border-slate-300 bg-gradient-to-r from-slate-100 to-slate-50 px-4 py-2 text-right font-medium text-slate-600 transition-all duration-150 select-none group-hover/line:from-blue-100 group-hover/line:to-blue-50 group-hover/line:text-blue-700 dark:border-gray-700/60 dark:from-gray-800/80 dark:to-gray-900/80 dark:text-gray-400 dark:group-hover/line:from-blue-900/20 dark:group-hover/line:to-blue-800/20 dark:group-hover/line:text-blue-400">
                      {ln.no}
                    </div>
                    <div class="code-content bg-white px-4 py-2 text-slate-900 transition-colors duration-150 group-hover/line:bg-blue-50/20 group-hover/line:text-slate-950 dark:bg-transparent dark:text-gray-200 dark:group-hover/line:text-gray-100">
                      {#if isLineExpanded(lineKey(i, ln._ci, ln._li))}
                        <span class="code-content-text">{@html highlight(ln.text, item.keywords)}</span>
                      {:else}
                        {#key i + '-' + ln._ci + '-' + ln._li + '-snippet'}
                          {@const sn = snippet(ln.text, item.keywords)}
                          {#if sn.leftTrunc}
                            <button type="button" class="group/expand mx-0.5 inline-flex items-center rounded-md border border-blue-200 bg-blue-50 px-1.5 py-0.5 text-xs font-medium text-blue-700 transition-all duration-200 hover:border-blue-300 hover:bg-blue-100 hover:text-blue-800 focus:ring-2 focus:ring-blue-500/20 focus:outline-none dark:border-blue-700/50 dark:bg-blue-900/30 dark:text-blue-300 dark:hover:bg-blue-800/50 dark:hover:text-blue-200" onclick={() => expandLine(lineKey(i, ln._ci, ln._li))} title="展开显示完整内容">
                              <svg class="mr-1 h-3 w-3 transition-transform duration-200 group-hover/expand:scale-105" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
                              </svg>
                              <span>…</span>
                            </button>
                          {/if}
                          <span class="code-content-text">{@html sn.html}</span>
                          {#if sn.rightTrunc}
                            <button type="button" class="group/expand mx-0.5 inline-flex items-center rounded-md border border-blue-200 bg-blue-50 px-1.5 py-0.5 text-xs font-medium text-blue-700 transition-all duration-200 hover:border-blue-300 hover:bg-blue-100 hover:text-blue-800 focus:ring-2 focus:ring-blue-500/20 focus:outline-none dark:border-blue-700/50 dark:bg-blue-900/30 dark:text-blue-300 dark:hover:bg-blue-800/50 dark:hover:text-blue-200" onclick={() => expandLine(lineKey(i, ln._ci, ln._li))} title="展开显示完整内容">
                              <span>…</span>
                              <svg class="ml-1 h-3 w-3 transition-transform duration-200 group-hover/expand:scale-105" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                              </svg>
                            </button>
                          {/if}
                        {/key}
                      {/if}
                    </div>
                  </div>
                {/each}
              </div>

              <!-- 卡片 foot：展开更多 matches -->
              {#if totalMatches(item) > 7}
                <div class="border-t border-slate-200 bg-gradient-to-r from-slate-100 to-gray-100 px-6 py-4 dark:border-gray-700/50 dark:from-gray-800/80 dark:to-gray-700/80">
                  <button class="group inline-flex items-center rounded-lg px-3 py-2 text-sm font-medium text-blue-600 transition-all duration-200 hover:bg-blue-50 hover:text-blue-700 focus:ring-2 focus:ring-blue-500/20 focus:ring-offset-2 focus:ring-offset-gray-50 focus:outline-none dark:text-blue-400 dark:hover:bg-blue-900/20 dark:hover:text-blue-300 dark:focus:ring-offset-gray-800" onclick={() => toggleFileShowAll(i)}>
                    {#if isFileShowAll(i)}
                      <svg class="mr-2 h-4 w-4" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" />
                      </svg>
                      收起全部匹配
                    {:else}
                      <svg class="mr-2 h-4 w-4" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                      </svg>
                      展开全部匹配
                    {/if}
                  </button>
                </div>
              {/if}
            {/if}
          </div>
        {/if}
      {/each}
    </div>
  </div>
</div>

<style>
  .code-content {
    font-family: var(--font-ui);
    font-feature-settings: 'liga' 0, 'calt' 0;
    font-variant-ligatures: none;
  }
</style>

