<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { env } from '$env/dynamic/public';
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import { get } from 'svelte/store';
  import { browser } from '$app/environment';

  const API_BASE = env.PUBLIC_API_BASE || '/api/v1/logsearch';

  let file = '';
  let sid = '';
  let total = 0;
  // 用于分页（内部使用）
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  let start = 1;
  let end = 0;
  // 仅用于展示，可为空
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  let keywords: string[] = [];
  let lines: { no: number; text: string }[] = [];
  let loading = false;
  let error: string | null = null;

  // 中文注释：虚拟滚动容器与虚拟化器
  let parentEl: HTMLDivElement | null = null;
  // 中文注释：PerfectScrollbar 实例
  let ps: any = null;
  // 中文注释：统一行高预估，供虚拟器与兜底高度计算复用
  // 行样式为 text-[13px] + leading-[20px] + py-0.5(上下各2px)，综合约 24px
  const EST_ROW = 24;
  // 中文注释：批量抓取的块大小（一次性加载全部时的每批行数）
  const BULK_CHUNK = 5000;

  const rowVirtualizer = createVirtualizer({
    count: total, // 总行数
    getScrollElement: () => parentEl, // 滚动容器（初始为 null，下面用 setOptions 保持同步）
    estimateSize: () => EST_ROW, // 预估单行高度(px)
    overscan: 20, // 预加载额外行，平衡性能与滚动流畅度
    // 中文注释：启用真实高度测量，避免底部出现多余空白或无法触底
    measureElement: (el: HTMLElement) => el.getBoundingClientRect().height,
  });

  // 中文注释：确保在 parentEl 绑定后，虚拟器获得滚动容器引用
  $: if (browser) {
    try {
      const v = get(rowVirtualizer);
      v?.setOptions?.({ getScrollElement: () => parentEl });
    } catch {}
  }

  // 中文注释：虚拟项缓存，避免在模板中使用 {@const}
  let vItems: Array<{ index: number; start: number; key: any }> = [];
  $: vItems = browser ? $rowVirtualizer.getVirtualItems() : [];

  // 中文注释：内容变化时更新 PerfectScrollbar（仅浏览器环境）
  $: if (browser && ps) {
    // 依赖以下状态变化触发更新
    void vItems;
    void lines.length;
    void end;
    void total;
    try {
      // 放到下一帧，避免布局抖动
      requestAnimationFrame(() => ps?.update?.());
    } catch {}
  }

  // 中文注释：当容器可用但尚未初始化时，初始化 PerfectScrollbar
  $: if (browser && parentEl && !ps) {
    import('perfect-scrollbar')
      .then((mod) => {
        const PerfectScrollbar = (mod as any).default || (mod as any);
        try {
          ps = new PerfectScrollbar(parentEl!, { suppressScrollX: true });
          if (parentEl) {
            parentEl.style.position = parentEl.style.position || 'relative';
            parentEl.style.overflow = 'hidden';
          }
        } catch {}
      })
      .catch(() => {});
  }

  // 中文注释：滚动触底兜底（即使虚拟器未就绪也能触发加载更多）
  function handleScroll() {
    if (!browser || loading || !parentEl) return;
    const el = parentEl;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 200 && end < total) {
      loadMore();
    }
  }

  // 中文注释：根据 index 取对应行（未加载则返回 null）
  function getLineByIndex(idx0: number): { no: number; text: string } | null {
    const lineNo = idx0 + 1; // TanStack 使用 0-based，这里转为 1-based 行号
    const rec = lines[lineNo - 1];
    // 中文注释：放宽校验，若数组存在对应项则直接返回，避免因行号不匹配导致空白
    return rec ?? null;
  }

  // 中文注释：接近底部自动加载更多
  $: {
    const items = $rowVirtualizer.getVirtualItems();
    if (items.length && total > 0 && !loading) {
      const maxIndex = items[items.length - 1].index; // 0-based
      const maxLineNo = maxIndex + 1; // 1-based
      if (maxLineNo > end - 50 && end < total) {
        // 当视口接近已加载末尾 50 行内时，触发加载更多
        loadMore();
      }
    }
  }

  async function fetchRange(s: number, e: number) {
    const url = `${API_BASE}/view.cache.json?sid=${encodeURIComponent(sid)}&file=${encodeURIComponent(file)}&start=${s}&end=${e}`;
    const res = await fetch(url, { headers: { Accept: 'application/json' } });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    return data as {
      file: string;
      total: number;
      start: number;
      end: number;
      keywords: string[];
      lines: { no: number; text: string }[];
    };
  }

  async function loadInitial() {
    try {
      loading = true;
      // 第一步：获取总行数与文件信息（轻量请求）
      const meta = await fetchRange(1, 1);
      file = meta.file;
      total = meta.total;

      if (total <= 0) {
        // 无内容
        start = 0;
        end = 0;
        lines = [];
        return;
      }

      // 第二步：一次性加载全部行
      const full = await fetchRange(1, total);
      start = full.start;
      end = full.end;
      keywords = full.keywords || [];
      lines = full.lines || [];

      // 中文注释：数据填充后强制重新测量，确保总高度与内容精确匹配，消除底部多余空白
      try {
        const v = get(rowVirtualizer);
        if (browser) {
          requestAnimationFrame(() => {
            try {
              v?.measure?.();
              ps?.update?.();
            } catch {}
          });
        }
      } catch {}
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message || '加载失败';
    } finally {
      loading = false;
    }
  }

  async function loadMore() {
    if (end >= total) return;
    try {
      loading = true;
      const nextS = end + 1;
      const nextE = Math.min(nextS + 999, total);
      const data = await fetchRange(nextS, nextE);
      end = data.end;
      lines = [...lines, ...(data.lines || [])];
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message || '加载更多失败';
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    const params = new URL(window.location.href).searchParams;
    file = (params.get('file') || '').trim();
    sid = (params.get('sid') || '').trim();
    if (!file) {
      error = '缺少 file 参数';
      return;
    }
    if (!sid) {
      error = '缺少 sid 参数';
      return;
    }
    loadInitial();

    // 中文注释：首屏渲染后，确保虚拟器定位到起始行，避免初次 items 为空
    // 中文注释：等一帧再滚动，确保容器与虚拟器完成初始化
    if (browser) {
      requestAnimationFrame(() => {
        try {
          const v = get(rowVirtualizer);
          v?.scrollToIndex?.(0);
        } catch {}
      });
    }

    // 中文注释：初始化 PerfectScrollbar（仅浏览器端）
    if (browser && parentEl) {
      import('perfect-scrollbar')
        .then((mod) => {
          const PerfectScrollbar = (mod as any).default || (mod as any);
          try {
            ps = new PerfectScrollbar(parentEl, {
              suppressScrollX: true,
              minScrollbarLength: 28,
              maxScrollbarLength: 200,
              wheelPropagation: false,
              wheelSpeed: 1
            });
            // 确保容器样式符合 PS 要求
            if (parentEl) {
              parentEl.style.position = parentEl.style.position || 'relative';
              parentEl.style.overflow = 'hidden';
              parentEl.style.paddingRight = parentEl.style.paddingRight || '8px';
            }
          } catch {}
        })
        .catch(() => {});
    }
  });

  onDestroy(() => {
    try {
      ps?.destroy?.();
    } catch {}
    ps = null;
  });
</script>

<div class="mx-auto max-w-[1560px] px-4 py-6">
  <h2 class="mb-2 font-mono text-sm text-gray-600 dark:text-gray-300">{file}</h2>
  {#if error}
    <div
      class="mb-3 rounded border border-red-300 bg-red-50 p-3 text-red-700 dark:border-red-800 dark:bg-red-950 dark:text-red-300"
    >
      {error}
    </div>
  {/if}
  <div class="mb-2 text-xs text-gray-500 dark:text-gray-400">
    {#if total > 0}
      共 {total} 行 · 已加载 {end} 行
    {/if}
  </div>

  {#if browser}
    <div
      class="ps rounded border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-800"
      bind:this={parentEl}
      onscroll={handleScroll}
      style="height: 90vh;"
    >
      <!-- 始终渲染 spacer：若虚拟器未就绪，使用兜底总高度，确保可滚动区域存在 -->
      <div
        style="height: {$rowVirtualizer.getTotalSize()}px; width: 100%; position: relative;"
      >
        {#if vItems.length === 0}
          <!-- 兜底：虚拟器未就绪或总高度未知时，先用正常流渲染已加载的前200行，避免空白 -->
          {#if lines.length > 0}
            {#each lines as ln (ln.no)}
              <div class="grid grid-cols-[72px_1fr] gap-0 font-mono text-[13px] leading-[20px]">
                <div
                  class="border-r border-gray-100 bg-gray-50 px-3 py-0.5 text-right text-gray-400 select-none dark:border-gray-700 dark:bg-gray-900 dark:text-gray-500"
                >
                  {ln.no}
                </div>
                <div class="px-3 py-0.5 break-all whitespace-pre-wrap">{ln.text}</div>
              </div>
            {/each}
          {:else}
            <div class="p-3 text-sm text-gray-500 dark:text-gray-400">暂无内容</div>
          {/if}
        {:else}
          {#each vItems as item (item.key)}
            {@const ln = getLineByIndex(item.index)}
            <div
              style="position: absolute; top: 0; left: 0; width: 100%; transform: translateY({item.start}px);"
              data-index={item.index}
            >
              {#if ln}
                <div class="grid grid-cols-[72px_1fr] gap-0 font-mono text-[13px] leading-[20px]">
                  <div
                    class="border-r border-gray-100 bg-gray-50 px-3 py-0.5 text-right text-gray-400 select-none dark:border-gray-700 dark:bg-gray-900 dark:text-gray-500"
                  >
                    {ln.no}
                  </div>
                  <div class="px-3 py-0.5 break-all whitespace-pre-wrap">{ln.text}</div>
                </div>
              {:else}
                <!-- 中文注释：占位行（尚未加载到该行），高度尽量匹配 estimateSize -->
                <div class="grid grid-cols-[72px_1fr] gap-0 font-mono text-[13px] leading-[20px] opacity-60">
                  <div
                    class="border-r border-gray-100 bg-gray-50 px-3 py-0.5 text-right text-gray-300 select-none dark:border-gray-700 dark:bg-gray-900 dark:text-gray-600"
                  >
                    {item.index + 1}
                  </div>
                  <div class="px-3 py-0.5 text-gray-400 dark:text-gray-500">加载中…</div>
                </div>
              {/if}
            </div>
          {/each}
        {/if}
      </div>
    </div>
  {:else}
    <!-- SSR 兜底：仅渲染已加载部分，避免 SSR 阶段报错/空白 -->
    <div class="rounded border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-800">
      {#each lines as ln (ln.no)}
        <div class="grid grid-cols-[72px_1fr] gap-0 font-mono text-[13px] leading-[20px]">
          <div
            class="border-r border-gray-100 bg-gray-50 px-3 py-0.5 text-right text-gray-400 select-none dark:border-gray-700 dark:bg-gray-900 dark:text-gray-500"
          >
            {ln.no}
          </div>
          <div class="px-3 py-0.5 break-all whitespace-pre-wrap">{ln.text}</div>
        </div>
      {/each}
    </div>
  {/if}

  <div class="mt-4">
    {#if total > 0}
      <span class="text-sm text-gray-500 dark:text-gray-400">已加载全部（{end}/{total}）。</span>
    {/if}
  </div>
</div>
