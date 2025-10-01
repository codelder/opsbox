<script lang="ts">
  /**
   * 搜索页面（重构版）
   * 使用 LogSeek 模块的 composables 和工具函数
   */
  import { SvelteSet } from 'svelte/reactivity';
  import type { SearchJsonResult, JsonLine, JsonChunk } from '$lib/modules/logseek';
  import { highlight, snippet } from '$lib/modules/logseek';
  import { startSearch as apiStartSearch, extractSessionId } from '$lib/modules/logseek';

  // 查询字符串、结果列表、加载与错误状态
  let q = $state('');
  let results = $state<SearchJsonResult[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let sid = $state('');

  // 分页控制（每批 20 条）
  const PAGE_SIZE = 20;
  let hasMore = $state(true);

  // 流读取持久状态
  let controller: AbortController | null = null;
  let reader: ReadableStreamDefaultReader<Uint8Array> | null = null;
  let decoder: TextDecoder | null = null;
  let buffer = $state('');

  // 读取一批（最多 PAGE_SIZE 条）
  async function readBatch(maxItems = PAGE_SIZE) {
    if (!reader) return;
    loading = true;
    let produced = 0;
    try {
      const dec = decoder ?? (decoder = new TextDecoder());
      while (produced < maxItems && reader) {
        // 1) 先消费缓冲区中已有的完整行
        while (produced < maxItems) {
          const nl = buffer.indexOf('\n');
          if (nl === -1) break;
          const line = buffer.slice(0, nl);
          buffer = buffer.slice(nl + 1);
          const trimmed = line.trim();
          if (!trimmed) continue;
          try {
            const obj = JSON.parse(trimmed);
            results = [...results, obj];
            produced += 1;
          } catch (e) {
            console.error('解析 NDJSON 行失败：', e, trimmed);
          }
        }
        if (produced >= maxItems) break;

        // 2) 读取更多字节补充缓冲区
        const { done, value } = await reader.read();
        if (done) {
          const rest = buffer;
          buffer = '';
          if (rest) {
            const parts = rest.split('\n');
            for (let i = 0; i < parts.length && produced < maxItems; i++) {
              const trimmed = parts[i].trim();
              if (!trimmed) continue;
              try {
                const obj = JSON.parse(trimmed);
                results = [...results, obj];
                produced += 1;
              } catch (e) {
                console.error('解析 NDJSON 尾段失败：', e, trimmed);
              }
            }
          }
          hasMore = false;
          break;
        }
        console.log('profiling: 读取更多字节：', dec.decode(value, { stream: true }).split('\n').length);
        buffer += dec.decode(value, { stream: true });
      }
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { name?: string; message?: string }) : {};
      if (err.name === 'AbortError') return;
      error = err.message || '搜索过程中发生未知错误';
      hasMore = false;
      reader = null;
    } finally {
      loading = false;
    }
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
    const wasExpanded = expandedAllMatches.has(i);
    if (wasExpanded) {
      expandedAllMatches.delete(i);
      // 收起时，延迟滚动以等待DOM更新
      setTimeout(() => {
        const cardElement = document.querySelector(`[data-result-card="${i}"]`);
        if (cardElement) {
          cardElement.scrollIntoView({
            behavior: 'smooth',
            block: 'start',
            inline: 'nearest'
          });
          // 添加临时高亮效果
          cardElement.classList.add('highlight-card');
          setTimeout(() => {
            cardElement.classList.remove('highlight-card');
          }, 2000);
        }
      }, 100);
    } else {
      expandedAllMatches.add(i);
    }
  }
  function isLineExpanded(key: string) {
    return expandedLines.has(key);
  }
  function expandLine(key: string) {
    expandedLines.add(key);
  }

  // 扁平化为行数组，便于“仅显示前7行”
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
    return flat.slice(0, Math.min(7, flat.length));
  }

  // 启动流式搜索（重置状态并读取首批）
  async function startSearch(query: string) {
    controller?.abort();
    controller = new AbortController();

    // 重置状态
    results = [];
    error = null;
    hasMore = true;
    buffer = '';
    decoder = null;
    reader = null;
    loading = true;

    try {
      const response = await apiStartSearch(query);
      sid = extractSessionId(response);
      reader = response.body?.getReader() || null;
      if (!reader) {
        error = '无法获取响应流';
        hasMore = false;
        return;
      }
      await readBatch(PAGE_SIZE);
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message || '搜索失败';
      hasMore = false;
      reader = null;
    }
  }

  // 继续读取下一批
  async function loadMore() {
    if (loading || !hasMore) return;
    await readBatch(PAGE_SIZE);
  }

  // 从地址栏读取 ?q=，并在客户端启动搜索
  // 一次性 $effect：读取 URL 查询并启动搜索
  let searchInit = $state(false);
  $effect(() => {
    if (searchInit) return;
    searchInit = true;
    const params = new URL(window.location.href).searchParams;
    const initial = (params.get('q') || '').trim();
    q = initial;
    if (initial) startSearch(initial);
  });

  // 卸载清理：使用 $effect 的清理函数
  $effect(() => {
    return () => {
      controller?.abort();
    };
  });

  // 表单提交（支持在页面内触发新搜索）
  function handleSubmit(e: Event) {
    e.preventDefault();
    const next = q.trim();
    if (!next) return;
    startSearch(next);
  }

  // 解析 item.path 为 { bucket, tar, inner }
  function splitPath(full: string): { bucket: string | null; tar: string; inner: string | null } {
    const idx = full.indexOf(':');
    const tarFull = idx >= 0 ? full.slice(full.indexOf('/'), idx) : full;
    const inner = idx >= 0 ? full.slice(idx + 1) : null;
    // const tarBase = tarFull.split('/').pop() || tarFull; // 暂未使用
    // const m = /BBIP_(\d+)_APPLOG_/i.exec(tarBase);
    // const bucket = m ? m[1] : null;
    const bucket = full.slice(0, full.indexOf('/', 1));
    return { bucket, tar: tarFull, inner };
  }
</script>

<!-- 页面标题与状态栏 -->
<div class="min-h-screen bg-gradient-to-br from-slate-100 to-gray-200 dark:from-gray-900 dark:to-gray-800">
  <div class="mx-auto max-w-[1560px] px-4 py-8">
    <!-- 顶部区域重新设计 -->
    <div class="mb-12">
      <!-- Logo 区域 -->
      <div class="mb-8 text-center">
        <label
          for="search"
          id="logo-label"
          class="inline-block transform cursor-pointer text-4xl font-extrabold tracking-[-0.25em] italic antialiased transition-transform duration-300 select-none hover:scale-105 md:text-6xl"
        >
          <span class="text-blue-600 drop-shadow-sm">L</span>
          <span class="text-red-600 drop-shadow-sm">o</span>
          <span class="text-yellow-500 drop-shadow-sm">g</span>
          <span class="text-green-600 drop-shadow-sm">s</span>
          <span class="text-blue-600 drop-shadow-sm">e</span>
          <span class="text-red-600 drop-shadow-sm">e</span>
          <span class="text-yellow-500 drop-shadow-sm">k</span>
        </label>
        <p class="mt-3 text-lg font-medium text-gray-600 dark:text-gray-300">快速搜索和浏览日志文件</p>
      </div>

      <!-- 搜索框区域 -->
      <form class="mx-auto max-w-4xl" onsubmit={handleSubmit}>
        <div class="group relative">
          <div class="pointer-events-none absolute inset-y-0 left-0 z-10 flex items-center pl-4">
            <svg
              class="h-6 w-6 text-gray-400 transition-colors duration-200 group-focus-within:text-blue-500"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path
                stroke-linecap="round"
                fill="none"
                stroke-linejoin="round"
                stroke-width="2"
                d="m21 21-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
              />
            </svg>
          </div>
          <input
            id="search"
            class="h-16 w-full rounded-3xl border-0 bg-white/90 py-4 pr-16 pl-14 text-lg placeholder-gray-500 shadow-2xl ring-1 shadow-gray-200/50 ring-gray-200/50 backdrop-blur-sm transition-all duration-300 hover:shadow-blue-200/25 hover:ring-blue-300/50 focus:bg-white focus:ring-2 focus:shadow-blue-300/30 focus:ring-blue-500 focus:outline-none dark:bg-gray-800/90 dark:text-white dark:placeholder-gray-400 dark:shadow-gray-900/30 dark:ring-gray-600/50 dark:hover:ring-blue-400/50 dark:focus:ring-blue-400"
            disabled={loading}
            bind:value={q}
            placeholder="输入查询串或自然语言搜索…"
            autocomplete="off"
          />
          {#if loading}
            <div class="absolute inset-y-0 right-0 flex items-center pr-5">
              <div class="h-7 w-7 animate-spin rounded-full border-2 border-blue-600 border-t-transparent"></div>
            </div>
          {:else if q}
            <button
              type="button"
              class="absolute inset-y-0 right-0 flex items-center pr-5 text-gray-400 transition-colors duration-200 hover:text-gray-600"
              onclick={() => {
                q = '';
                results = [];
              }}
              aria-label="清除搜索内容"
            >
              <svg class="h-6 w-6" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          {/if}
        </div>
      </form>
    </div>

    <!-- 结果区域 -->
    <div class="space-y-8">
      {#each results as item, i (item.path + '-' + i)}
        {#if item && item.path && item.chunks}
          <div
            class="group overflow-hidden rounded-2xl border border-white/60 bg-white/95 shadow-xl shadow-slate-300/40 backdrop-blur-sm transition-all duration-300 hover:shadow-2xl hover:shadow-slate-400/50 dark:border-gray-700/50 dark:bg-gray-800/80 dark:shadow-gray-900/20 dark:hover:shadow-gray-900/30"
            data-result-card={i}
          >
            <!-- 结果头：文件路径（可折叠）- 重新设计为多层级布局 -->
            <button
              type="button"
              class="w-full border-b border-slate-200 bg-gradient-to-r from-slate-50 to-gray-100 px-6 py-6 text-left text-sm text-slate-700 transition-all duration-200 hover:from-slate-100 hover:to-slate-200 focus:ring-2 focus:ring-blue-500/20 focus:outline-none focus:ring-inset dark:border-gray-700/50 dark:from-gray-800/50 dark:to-gray-700/50 dark:text-gray-300 dark:hover:from-gray-700/50 dark:hover:to-gray-600/50"
              onclick={() => toggleFileCollapsed(i)}
            >
              <!-- 容器：小屏纵向排列，大屏左右分布 -->
              <span class="flex w-full flex-col gap-3 md:flex-row md:items-start md:justify-between">
                <!-- 左侧：标题 + 元信息 -->
                <span class="min-w-0 flex-1">
                  <!-- 主标题（inner 作为重点信息）：字号更大，颜色更深 -->
                  {#if splitPath(item.path).inner}
                    <span class="mb-2 flex items-start gap-2">
                      <span
                        role="link"
                        tabindex="0"
                        class="group/link cursor-pointer font-mono text-base leading-tight font-semibold text-slate-900 transition-colors duration-200 hover:text-blue-700 md:text-lg lg:text-xl dark:text-gray-100 dark:hover:text-blue-300"
                        title={splitPath(item.path).inner}
                        onclick={(e) => {
                          e.stopPropagation();
                          // 带上 sid 以便 /view 从缓存读取；在新标签页打开
                          const base = '/view';
                          const url = `${base}?sid=${encodeURIComponent(sid)}&file=${encodeURIComponent(item.path)}`;
                          window.open(url, '_blank', 'noopener');
                        }}
                        onkeydown={(e) => {
                          // 无障碍支持 Enter/Space 键（新标签页打开）
                          if (e.key === 'Enter' || e.key === ' ') {
                            e.preventDefault();
                            e.stopPropagation();
                            const base = '/view';
                            const url = `${base}?sid=${encodeURIComponent(sid)}&file=${encodeURIComponent(item.path)}`;
                            window.open(url, '_blank', 'noopener');
                          }
                        }}
                      >
                        <span class="line-clamp-2 group-hover/link:underline md:line-clamp-1"
                          >{splitPath(item.path).inner}</span
                        >
                      </span>
                      <svg
                        class="mt-1 h-4 w-4 shrink-0 text-blue-600 opacity-0 transition-opacity duration-200 group-hover/link:opacity-100 dark:text-blue-400"
                        viewBox="0 0 24 24"
                        stroke="currentColor"
                      >
                        <path
                          stroke-linecap="round"
                          stroke-linejoin="round"
                          stroke-width="2"
                          d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"
                        />
                      </svg>
                    </span>
                  {:else}
                    <!-- 如果没有 inner，显示完整路径作为主标题 -->
                    <span class="mb-2">
                      <span
                        class="font-mono text-base leading-tight font-semibold text-slate-900 md:text-lg dark:text-gray-100"
                      >
                        {item.path}
                      </span>
                    </span>
                  {/if}

                  <!-- 元信息行：允许换行，字号更小，灰度更轻 -->
                  <span class="flex flex-wrap items-center gap-2 text-xs text-slate-600 dark:text-gray-400">
                    {#if splitPath(item.path).bucket}
                      <span
                        class="inline-flex items-center rounded-full bg-blue-50 px-2.5 py-1 text-[11px] font-medium text-blue-700 ring-1 ring-blue-200 dark:bg-blue-900/30 dark:text-blue-300 dark:ring-blue-800"
                      >
                        <svg class="mr-1 h-3 w-3" fill="currentColor" viewBox="0 0 20 20">
                          <path
                            d="M3 4a1 1 0 011-1h3a1 1 0 011 1v3a1 1 0 01-1 1H4a1 1 0 01-1-1V4zM3 10a1 1 0 011-1h3a1 1 0 011 1v3a1 1 0 01-1 1H4a1 1 0 01-1-1v-3zM10 4a1 1 0 011-1h3a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1V4zM10 10a1 1 0 011-1h3a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-3z"
                          />
                        </svg>
                        bucket {splitPath(item.path).bucket}
                      </span>
                    {/if}

                    <span
                      class="inline-flex items-center rounded-md bg-gray-100 px-2.5 py-1 text-[11px] font-medium text-gray-700 ring-1 ring-gray-200 dark:bg-gray-700/50 dark:text-gray-300 dark:ring-gray-600"
                    >
                      <svg class="mr-1 h-3 w-3" fill="currentColor" viewBox="0 0 20 20">
                        <path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z" />
                      </svg>
                      {splitPath(item.path).tar}
                    </span>

                    {#if item.keywords?.length}
                      <!-- 关键词放在元信息行，尺寸更小，可多行换行 -->
                      <span class="flex flex-wrap items-center gap-1.5">
                        {#each item.keywords.slice(0, 4) as keyword (keyword)}
                          <span
                            class="inline-flex items-center rounded-md bg-yellow-50 px-2 py-0.5 text-[11px] font-medium text-yellow-800 ring-1 ring-yellow-600/20 dark:bg-yellow-900/30 dark:text-yellow-300 dark:ring-yellow-500/20"
                            >{keyword}</span
                          >
                        {/each}
                        {#if item.keywords.length > 4}
                          <span class="text-[11px] text-gray-500 dark:text-gray-400">+{item.keywords.length - 4}</span>
                        {/if}
                      </span>
                    {/if}

                    <!-- 统计信息：匹配行数 -->
                    <span
                      class="inline-flex items-center rounded-md bg-green-50 px-2 py-0.5 text-[11px] font-medium text-green-700 ring-1 ring-green-200 dark:bg-green-900/30 dark:text-green-300 dark:ring-green-800"
                    >
                      <svg class="mr-1 h-3 w-3" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4" />
                      </svg>
                      {totalMatches(item)} 行匹配
                    </span>
                  </span>
                </span>

                <!-- 右侧：折叠箭头（与标题对齐） -->
                <span class="mt-2 flex shrink-0 items-center md:mt-0">
                  <span
                    class="flex h-8 w-8 items-center justify-center rounded-full bg-gray-200/60 transition-colors duration-200 group-hover:bg-gray-300/60 dark:bg-gray-600/50 dark:group-hover:bg-gray-500/50"
                  >
                    <svg
                      class="h-4 w-4 text-gray-700 transition-transform duration-200 {isFileCollapsed(i)
                        ? ''
                        : 'rotate-180'} dark:text-gray-200"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                    >
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                    </svg>
                  </span>
                </span>
              </span>
            </button>

            {#if !isFileCollapsed(i)}
              <!-- 代码块区域：行号 + 内容（默认仅前7行） -->
              <div
                class="overflow-hidden bg-gradient-to-r from-slate-50 to-white transition-all duration-500 ease-in-out dark:from-gray-900/50 dark:to-gray-800/50"
              >
                {#each visibleLines(item, i) as ln (i + '-' + ln._ci + '-' + ln._li)}
                  <div
                    class="group/line grid grid-cols-[80px_1fr] gap-0 font-mono text-sm leading-[24px] transition-colors duration-150 hover:bg-blue-50/40 dark:hover:bg-blue-900/10"
                  >
                    <div
                      class="border-r border-slate-300 bg-gradient-to-r from-slate-100 to-slate-50 px-4 py-2 text-right font-medium text-slate-600 transition-all duration-150 select-none group-hover/line:from-blue-100 group-hover/line:to-blue-50 group-hover/line:text-blue-700 dark:border-gray-700/60 dark:from-gray-800/80 dark:to-gray-900/80 dark:text-gray-400 dark:group-hover/line:from-blue-900/20 dark:group-hover/line:to-blue-800/20 dark:group-hover/line:text-blue-400"
                    >
                      {ln.no}
                    </div>
                    <div
                      class="code-content bg-white px-4 py-2 text-slate-900 transition-colors duration-150 group-hover/line:bg-blue-50/20 group-hover/line:text-slate-950 dark:bg-transparent dark:text-gray-200 dark:group-hover/line:text-gray-100"
                    >
                      {#if isLineExpanded(lineKey(i, ln._ci, ln._li))}
                        <span class="code-content-text">{@html highlight(ln.text, item.keywords)}</span>
                      {:else}
                        {#key i + '-' + ln._ci + '-' + ln._li + '-snippet'}
                          {@const sn = snippet(ln.text, item.keywords)}
                          {#if sn.leftTrunc}
                            <button
                              type="button"
                              class="group/expand mx-0.5 inline-flex items-center rounded-md border border-blue-200 bg-blue-50 px-1.5 py-0.5 text-xs font-medium text-blue-700 transition-all duration-200 hover:border-blue-300 hover:bg-blue-100 hover:text-blue-800 focus:ring-2 focus:ring-blue-500/20 focus:outline-none dark:border-blue-700/50 dark:bg-blue-900/30 dark:text-blue-300 dark:hover:bg-blue-800/50 dark:hover:text-blue-200"
                              onclick={() => expandLine(lineKey(i, ln._ci, ln._li))}
                              title="展开显示完整内容"
                            >
                              <svg
                                class="mr-1 h-3 w-3 transition-transform duration-200 group-hover/expand:scale-105"
                                viewBox="0 0 24 24"
                                stroke="currentColor"
                              >
                                <path
                                  stroke-linecap="round"
                                  stroke-linejoin="round"
                                  stroke-width="2"
                                  d="M15 19l-7-7 7-7"
                                />
                              </svg>
                              <span>…</span>
                            </button>
                          {/if}
                          <span class="code-content-text">{@html sn.html}</span>
                          {#if sn.rightTrunc}
                            <button
                              type="button"
                              class="group/expand mx-0.5 inline-flex items-center rounded-md border border-blue-200 bg-blue-50 px-1.5 py-0.5 text-xs font-medium text-blue-700 transition-all duration-200 hover:border-blue-300 hover:bg-blue-100 hover:text-blue-800 focus:ring-2 focus:ring-blue-500/20 focus:outline-none dark:border-blue-700/50 dark:bg-blue-900/30 dark:text-blue-300 dark:hover:bg-blue-800/50 dark:hover:text-blue-200"
                              onclick={() => expandLine(lineKey(i, ln._ci, ln._li))}
                              title="展开显示完整内容"
                            >
                              <span>…</span>
                              <svg
                                class="ml-1 h-3 w-3 transition-transform duration-200 group-hover/expand:scale-105"
                                viewBox="0 0 24 24"
                                stroke="currentColor"
                              >
                                <path
                                  stroke-linecap="round"
                                  stroke-linejoin="round"
                                  stroke-width="2"
                                  d="M9 5l7 7-7 7"
                                />
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
                <div
                  class="border-t border-slate-200 bg-gradient-to-r from-slate-100 to-gray-100 px-6 py-4 dark:border-gray-700/50 dark:from-gray-800/80 dark:to-gray-700/80"
                >
                  <button
                    class="group inline-flex items-center rounded-lg px-3 py-2 text-sm font-medium text-blue-600 transition-all duration-200 hover:bg-blue-50 hover:text-blue-700 focus:ring-2 focus:ring-blue-500/20 focus:ring-offset-2 focus:ring-offset-gray-50 focus:outline-none dark:text-blue-400 dark:hover:bg-blue-900/20 dark:hover:text-blue-300 dark:focus:ring-offset-gray-800"
                    onclick={() => toggleFileShowAll(i)}
                  >
                    {#if isFileShowAll(i)}
                      <svg
                        class="mr-2 h-4 w-4 transition-transform duration-200 group-hover:-translate-y-0.5"
                        viewBox="0 0 24 24"
                        stroke="currentColor"
                      >
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" />
                      </svg>
                      收起显示前 7 行
                    {:else}
                      <svg
                        class="mr-2 h-4 w-4 transition-transform duration-200 group-hover:translate-y-0.5"
                        viewBox="0 0 24 24"
                        stroke="currentColor"
                      >
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                      </svg>
                      显示剩余的
                      <span
                        class="mx-1 rounded-full bg-blue-100 px-2 py-0.5 text-xs font-semibold text-blue-800 dark:bg-blue-900/50 dark:text-blue-200"
                        >{totalMatches(item) - 7}</span
                      > 行匹配
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

      <!-- 空状态和错误状态 -->
      {#if error}
        <div class="mx-auto max-w-md text-center">
          <div class="rounded-2xl bg-red-50 p-8 shadow-lg ring-1 ring-red-200 dark:bg-red-900/20 dark:ring-red-800/50">
            <div class="mx-auto flex h-16 w-16 items-center justify-center rounded-full bg-red-100 dark:bg-red-900/50">
              <svg class="h-8 w-8 text-red-600 dark:text-red-400" viewBox="0 0 24 24" stroke="currentColor">
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.732 16.5c-.77.833.192 2.5 1.732 2.5z"
                />
              </svg>
            </div>
            <h3 class="mt-4 text-lg font-semibold text-red-900 dark:text-red-200">搜索出错</h3>
            <p class="mt-2 text-sm text-red-700 dark:text-red-300">{error}</p>
            <button
              class="mt-4 inline-flex items-center rounded-lg bg-red-600 px-4 py-2 text-sm font-medium text-white transition-colors duration-200 hover:bg-red-700 focus:ring-2 focus:ring-red-500 focus:ring-offset-2 focus:outline-none"
              onclick={() => {
                error = null;
                if (q) startSearch(q);
              }}
            >
              <svg class="mr-2 -ml-1 h-4 w-4" viewBox="0 0 24 24" stroke="currentColor">
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                />
              </svg>
              重新搜索
            </button>
          </div>
        </div>
      {:else if !loading && results.length === 0 && q && !hasMore}
        <div class="mx-auto max-w-md text-center">
          <div class="rounded-2xl bg-gray-50 p-8 shadow-lg ring-1 ring-gray-200 dark:bg-gray-800/50 dark:ring-gray-700">
            <div class="mx-auto flex h-16 w-16 items-center justify-center rounded-full bg-gray-100 dark:bg-gray-700">
              <svg class="h-8 w-8 text-gray-400" viewBox="0 0 24 24" stroke="currentColor">
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                />
              </svg>
            </div>
            <h3 class="mt-4 text-lg font-semibold text-gray-900 dark:text-gray-200">无匹配结果</h3>
            <p class="mt-2 text-sm text-gray-600 dark:text-gray-400">尝试使用不同的关键词或更广泛的搜索词汇</p>
          </div>
        </div>
      {:else if !loading && !error && results.length === 0 && !q}
        <div class="mx-auto max-w-lg text-center">
          <div
            class="rounded-2xl bg-gradient-to-br from-blue-50 to-indigo-50 p-8 shadow-lg ring-1 ring-blue-200 dark:from-blue-900/20 dark:to-indigo-900/20 dark:ring-blue-800/50"
          >
            <div
              class="mx-auto flex h-16 w-16 items-center justify-center rounded-full bg-gradient-to-br from-blue-100 to-indigo-100 dark:from-blue-800/50 dark:to-indigo-800/50"
            >
              <svg class="h-8 w-8 text-blue-600 dark:text-blue-400" viewBox="0 0 24 24" stroke="currentColor">
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                />
              </svg>
            </div>
            <h3 class="mt-4 text-xl font-semibold text-blue-900 dark:text-blue-200">开始搜索</h3>
            <p class="mt-2 text-sm text-blue-700 dark:text-blue-300">
              在上方输入框中输入关键词或自然语言查询，开始搜索日志
            </p>
          </div>
        </div>
      {/if}
    </div>

    <!-- 分页控制按钮 -->
    <div class="mt-12 flex items-center justify-center">
      {#if hasMore}
        <button
          class="group inline-flex items-center rounded-2xl bg-gradient-to-r from-blue-600 to-blue-700 px-8 py-4 text-base font-semibold text-white shadow-xl shadow-blue-500/25 transition-all duration-300 hover:from-blue-700 hover:to-blue-800 hover:shadow-2xl hover:shadow-blue-500/30 focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:ring-offset-gray-50 focus:outline-none disabled:from-gray-400 disabled:to-gray-500 disabled:shadow-gray-400/25 dark:focus:ring-offset-gray-900 dark:disabled:from-gray-600 dark:disabled:to-gray-700"
          onclick={loadMore}
          disabled={loading}
        >
          {#if loading}
            <div class="mr-3 h-5 w-5 animate-spin rounded-full border-2 border-white border-t-transparent"></div>
            加载中…
          {:else}
            <svg
              class="mr-3 h-5 w-5 transition-transform duration-200 group-hover:translate-y-0.5"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
            </svg>
            加载更多结果
          {/if}
        </button>
      {:else if results.length > 0}
        <div class="text-center">
          <div
            class="inline-flex items-center rounded-full bg-green-100 px-6 py-3 text-sm font-medium text-green-800 ring-1 ring-green-200 dark:bg-green-900/30 dark:text-green-300 dark:ring-green-800/50"
          >
            <svg class="mr-2 h-4 w-4" viewBox="0 0 24 24" stroke="currentColor">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
            已显示所有搜索结果
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  /* 动态添加的临时高亮效果类，用于搜索结果卡片收起时的视觉反馈 - Used dynamically in toggleFileShowAll() */
  :global(.highlight-card) {
    border: 3px solid rgba(59, 130, 246, 0.5);
    box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.2);
    animation: highlight-pulse 2s ease-in-out;
  }

  /* 动态添加的临时高亮效果类，用于搜索结果卡片收起时的视觉反馈 - Used dynamically in toggleFileShowAll() */
  :global(.dark .highlight-card) {
    border: 3px solid rgba(96, 165, 250, 0.5);
    box-shadow: 0 0 0 2px rgba(96, 165, 250, 0.2);
  }

  .code-content {
    font-family: var(--font-ui), monospace;
    font-feature-settings:
      'liga' 0,
      'calt' 0;
    font-variant-ligatures: none;
  }

  .code-content-text {
    white-space: pre-wrap;
    word-break: break-all;
    display: inline;
  }

  @keyframes highlight-pulse {
    0%,
    100% {
      transform: scale(1);
      opacity: 1;
    }
    50% {
      transform: scale(1.002);
      opacity: 0.95;
    }
  }
</style>
