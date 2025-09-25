<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { env } from '$env/dynamic/public';
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import { get } from 'svelte/store';
  import { browser } from '$app/environment';

  const API_BASE = env.PUBLIC_API_BASE || '/api/v1/logsearch';

  // 中文注释：使用 Svelte 5 Runes 语法定义响应式状态
  let file = $state('');
  let sid = $state('');
  let total = $state(0);
  // 用于分页（内部使用）
  // let start = $state(1); // 暂未使用
  let end = $state(0);
  // 仅用于展示，可为空
  let keywords = $state<string[]>([]);
  let lines = $state<{ no: number; text: string }[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  // 中文注释：首屏加载触发标志，避免重复加载
  let initTriggered = $state(false);

  // 中文注释：PerfectScrollbar 类型定义
  interface PerfectScrollbarInstance {
    update?: () => void;
    destroy?: () => void;
  }

  // 中文注释：虚拟滚动容器与虚拟化器
  let parentEl = $state<HTMLDivElement | null>(null);
  // 中文注释：PerfectScrollbar 实例
  let ps = $state<PerfectScrollbarInstance | null>(null);
  // 中文注释：统一行高预估，供虚拟器与兜底高度计算复用
  // 行样式为 text-xs + leading-[16px] + py-0.5(上下各2px)，综合约 16px
  const EST_ROW = 16;
  // 中文注释：批量抓取的块大小（一次性加载全部时的每批行数）
  // const BULK_CHUNK = 5000; // 暂未使用

  const rowVirtualizer = createVirtualizer({
    count: 0, // 初始为 0，在 effect 中更新
    getScrollElement: () => parentEl, // 滚动容器（初始为 null，下面用 setOptions 保持同步）
    estimateSize: () => EST_ROW, // 预估单行高度(px)
    overscan: 20, // 预加载额外行，平衡性能与滚动流畅度
    // 中文注释：启用真实高度测量，避免底部出现多余空白或无法触底
    measureElement: (el: HTMLElement) => el.getBoundingClientRect().height,
    getItemKey: (index: number) => index
  });

  // 中文注释：虚拟项缓存，使用 $derived 自动计算
  let vItems = $derived(browser ? $rowVirtualizer.getVirtualItems() : []);

  // 中文注释：更新虚拟器的总行数
  $effect(() => {
    try {
      const v = get(rowVirtualizer);
      v?.setOptions?.({ count: total });
    } catch {
      // 错误处理：静默忽略
    }
  });

  // 中文注释：确保在 parentEl 绑定后，虚拟器获得滚动容器引用
  $effect(() => {
    if (browser && parentEl !== undefined) {
      try {
        const v = get(rowVirtualizer);
        v?.setOptions?.({ getScrollElement: () => parentEl });
      } catch {
        // 错误处理：静默忽略
      }
    }
  });

  // 中文注释：检查是否需要加载更多的副作用
  $effect(() => {
    if (vItems.length > 0) {
      checkNeedLoadMore();
    }
  });

  // 中文注释：内容变化时更新 PerfectScrollbar（仅浏览器环境）
  $effect(() => {
    if (browser && ps) {
      // 依赖以下状态变化触发更新
      vItems;
      lines.length;
      end;
      total;
      try {
        // 放到下一帧，避免布局抖动
        requestAnimationFrame(() => ps?.update?.());
      } catch {
        // 错误处理：静默忽略
      }
    }
  });

  // 中文注释：当容器可用但尚未初始化时，初始化 PerfectScrollbar
  // 中文注释：初始化 PerfectScrollbar 的函数（避免无限循环）
  let psInitialized = $state(false);
  function initializePerfectScrollbar() {
    if (browser && parentEl && !ps && !psInitialized) {
      import('perfect-scrollbar')
        .then((mod) => {
          const PerfectScrollbarClass = (mod as { default?: unknown }).default || mod;
          try {
            psInitialized = true;
            ps = new (PerfectScrollbarClass as new (
              el: Element,
              opts: Record<string, unknown>
            ) => PerfectScrollbarInstance)(parentEl!, { suppressScrollX: true });
            if (parentEl) {
              parentEl.style.position = parentEl.style.position || 'relative';
              parentEl.style.overflow = 'hidden';
            }
          } catch {
            psInitialized = false;
            // 错误处理：静默忽略
          }
        })
        .catch(() => {
          // 错误处理：静默忍受加载失败
        });
    }
  }

  // 在 parentEl 变化时调用初始化
  $effect(() => {
    if (parentEl) {
      initializePerfectScrollbar();
    }
  });

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

  // 中文注释：接近底部自动加载更多（使用函数避免无限循环）
  let lastCheckedEnd = $state(0);
  let checkLoadTimeout = $state<ReturnType<typeof setTimeout> | null>(null);
  function checkNeedLoadMore() {
    if (checkLoadTimeout) clearTimeout(checkLoadTimeout);
    checkLoadTimeout = setTimeout(() => {
      const items = $rowVirtualizer.getVirtualItems();
      if (items.length && total > 0 && !loading && end !== lastCheckedEnd) {
        const maxIndex = items[items.length - 1].index; // 0-based
        const maxLineNo = maxIndex + 1; // 1-based
        if (maxLineNo > end - 50 && end < total) {
          lastCheckedEnd = end;
          // 当视口接近已加载末尾 50 行内时，触发加载更多
          loadMore();
        }
      }
    }, 100);
  }

  async function fetchRange(
    s: number,
    e: number
  ): Promise<{
    success: boolean;
    data?: {
      file: string;
      total: number;
      start: number;
      end: number;
      keywords: string[];
      lines: { no: number; text: string }[];
    };
    error?: string;
  }> {
    const url = `${API_BASE}/view.cache.json?sid=${encodeURIComponent(sid)}&file=${encodeURIComponent(file)}&start=${s}&end=${e}`;
    try {
      console.log('[view] fetchRange ->', { url, s, e, sid, file });
      const res = await fetch(url, { headers: { Accept: 'application/json' } });
      console.log('[view] fetchRange status', res.status);
      if (!res.ok) {
        return { success: false, error: `HTTP ${res.status}` };
      }
      const data = (await res.json()) as {
        file: string;
        total: number;
        start: number;
        end: number;
        keywords: string[];
        lines: { no: number; text: string }[];
      };
      console.log('[view] fetchRange ok', { total: data.total, start: data.start, end: data.end });
      return { success: true, data };
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      console.error('[view] fetchRange error', err);
      return { success: false, error: err.message || '网络请求失败' };
    }
  }

  async function loadInitial() {
    console.log('[view] loadInitial start', { sid, file });
    loading = true;
    try {
      // 第一步：获取总行数与文件信息（轻量请求）
      const metaResult = await fetchRange(1, 1);
      if (!metaResult.success) {
        error = metaResult.error || '加载失败';
        return;
      }

      const meta = metaResult.data!;
      file = meta.file;
      total = meta.total;
      console.log('[view] meta loaded', { total });

      if (total <= 0) {
        // 无内容
        // start = 0; // 暂未使用
        end = 0;
        lines = [];
        return;
      }

      // 第二步：一次性加载全部行
      const fullResult = await fetchRange(1, total);
      if (!fullResult.success) {
        error = fullResult.error || '加载失败';
        return;
      }

      const full = fullResult.data!;
      // start = full.start; // 暂未使用
      end = full.end;
      keywords = full.keywords || [];
      lines = full.lines || [];
      console.log('[view] full loaded', { end, lines: lines.length });

      // 中文注释：数据填充后强制重新测量，确保总高度与内容精确匹配，消除底部多余空白
      try {
        const v = get(rowVirtualizer);
        if (browser) {
          requestAnimationFrame(() => {
            try {
              v?.measure?.();
              ps?.update?.();
            } catch {
              // 错误处理：静默忽略
            }
          });
        }
      } catch {
        // 错误处理：静默忽略
      }
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message || '加载失败';
    } finally {
      loading = false;
    }
  }

  async function loadMore() {
    if (end >= total) return;
    loading = true;
    try {
      const nextS = end + 1;
      const nextE = Math.min(nextS + 999, total);
      const result = await fetchRange(nextS, nextE);

      if (!result.success) {
        error = result.error || '加载更多失败';
        return;
      }

      const data = result.data!;
      end = data.end;
      lines = [...lines, ...(data.lines || [])];
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message || '加载更多失败';
    } finally {
      loading = false;
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
    return s.replace(/[.*+?^${}()|[\\]\\]/g, '\\$&');
  }
  // highlight 函数暂未使用，保留以备未来使用
  // function highlight(line: string, keywords: string[]): string {
  //   let out = escapeHtml(line);
  //   const kws = (keywords || []).filter((k) => k && k.length > 0);
  //   for (const kw of kws) {
  //     const re = new RegExp(escapeRegExp(kw), 'g');
  //     out = out.replace(re, (m) => `<mark>${escapeHtml(m)}</mark>`);
  //   }
  //   return out;
  // }

  // 关键词高亮函数，用于在正文中高亮显示搜索关键词
  function highlightKeywords(text: string): string {
    if (!keywords || keywords.length === 0 || !text) {
      return escapeHtml(text);
    }

    let result = escapeHtml(text);

    // 对每个关键词进行高亮处理
    for (const keyword of keywords) {
      if (keyword && keyword.trim()) {
        const escapedKeyword = escapeRegExp(keyword.trim());
        // 大小写敏感匹配
        const regex = new RegExp(escapedKeyword, 'g');
        result = result.replace(regex, (match) => {
          return `<mark class="bg-yellow-200/80 py-0.5 rounded-sm text-yellow-900 dark:bg-yellow-400/30 dark:text-yellow-200">${match}</mark>`;
        });
      }
    }

    return result;
  }

  // 中文注释：派生下载文件名（将不合法的文件名字符替换为下划线）
  function deriveDownloadName(): string {
    const base = extractFileName(file) || 'log.txt';
    const safe = base.replace(/[\\/:*?"<>|]+/g, '_');
    return safe || 'log.txt';
  }

  // 中文注释：提取文件名（去掉tar包路径，只保留实际文件名）
  function extractFileName(fullPath: string): string {
    if (!fullPath) return '未知文件';

    // 如果路径包含冒号，说明是 tar包:文件路径 格式
    const colonIndex = fullPath.indexOf(':');
    if (colonIndex >= 0) {
      // 取冒号后面的部分（tar包内的文件路径）
      const innerPath = fullPath.slice(colonIndex + 1);
      // 返回文件名（路径的最后一部分）
      return innerPath.split('/').pop() || innerPath || '未知文件';
    }

    // 如果没有冒号，直接取路径的最后一部分
    return fullPath.split('/').pop() || fullPath || '未知文件';
  }

  // 中文注释：提取tar包名称
  function extractTarName(fullPath: string): string | null {
    if (!fullPath) return null;

    // 如果路径包含冒号，说明是 tar包:文件路径 格式
    const colonIndex = fullPath.indexOf(':');
    if (colonIndex >= 0) {
      // 取冒号前面的部分（tar包路径）
      const tarPath = fullPath.slice(0, colonIndex);
      // 返回tar包文件名（路径的最后一部分）
      return tarPath.split('/').pop() || tarPath || null;
    }

    // 如果没有冒号，说明不是tar包内的文件
    return null;
  }

  // 中文注释：下载当前视图的完整原始文本（不含行号）
  function downloadCurrentFile() {
    try {
      if (!lines || lines.length === 0) return;
      const content = lines.map((ln) => ln?.text ?? '').join('\n');
      const blob = new Blob([content], { type: 'text/plain;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = deriveDownloadName();
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message || '下载失败';
    }
  }

  onMount(() => {
    console.log('[view] onMount');
    const params = new URL(window.location.href).searchParams;
    file = (params.get('file') || '').trim();
    sid = (params.get('sid') || '').trim();
    console.log('[view] parsed params', { sid, file });
    if (!file) {
      console.error('[view] 缺少 file 参数');
      error = '缺少 file 参数';
      return;
    }
    if (!sid) {
      console.error('[view] 缺少 sid 参数');
      error = '缺少 sid 参数';
      return;
    }
    initTriggered = true;
    loadInitial();

    // 中文注释：首屏渲染后，确保虚拟器定位到起始行，避免初次 items 为空
    // 中文注释：等一帧再滚动，确保容器与虚拟器完成初始化
    if (browser) {
      requestAnimationFrame(() => {
        try {
          const v = get(rowVirtualizer);
          v?.scrollToIndex?.(0);
        } catch {
          // 错误处理：静默忽略
        }
      });
    }

    // 中文注释：初始化 PerfectScrollbar（仅浏览器端）
    if (browser && parentEl) {
      import('perfect-scrollbar')
        .then((mod) => {
          const PerfectScrollbarClass2 = (mod as { default?: unknown }).default || mod;
          try {
            ps = new (PerfectScrollbarClass2 as new (
              el: Element,
              opts: Record<string, unknown>
            ) => PerfectScrollbarInstance)(parentEl!, {
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
          } catch {
            // 错误处理：静默忽略
          }
        })
        .catch(() => {
          // 错误处理：静默忍受加载失败
        });
    }
  });

  // 中文注释：兜底副作用——当解析到 sid 与 file 且尚未触发初始化时，自动调用一次加载
  $effect(() => {
    if (browser && !initTriggered && !loading && lines.length === 0 && sid && file) {
      console.log('[view] fallback $effect triggers loadInitial', { sid, file });
      initTriggered = true;
      // 避免在同一 tick 内与其它副作用冲突，下一轮微任务触发
      Promise.resolve().then(() => loadInitial());
    }
  });

  onDestroy(() => {
    try {
      ps?.destroy?.();
    } catch {
      // 错误处理：静默忽略
    }
    ps = null;
    if (checkLoadTimeout) {
      clearTimeout(checkLoadTimeout);
    }
  });
</script>

<!-- 中文注释：页面标题与状态栏 -->
<div
  class="-mt-16 flex h-screen flex-col bg-gradient-to-br from-slate-100 to-gray-200 dark:from-gray-900 dark:to-gray-800"
>
  <div class="mx-auto flex w-full max-w-[1560px] flex-1 flex-col px-4">
    {#if error}
      <div class="mx-auto mb-4 max-w-md flex-shrink-0 text-center">
        <div class="rounded-xl bg-red-50 p-4 shadow-lg ring-1 ring-red-200 dark:bg-red-900/20 dark:ring-red-800/50">
          <div class="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-red-100 dark:bg-red-900/50">
            <svg class="h-6 w-6 text-red-600 dark:text-red-400" viewBox="0 0 24 24" stroke="currentColor">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.732 16.5c-.77.833.192 2.5 1.732 2.5z"
              />
            </svg>
          </div>
          <h3 class="mt-2 text-base font-semibold text-red-900 dark:text-red-200">加载出错</h3>
          <p class="mt-1 text-sm text-red-700 dark:text-red-300">{error}</p>
        </div>
      </div>
    {/if}

    {#if browser}
      <!-- 主内容卡片：文件信息 + 虚拟滚动容器 -->
      <div
        class="my-6 flex flex-1 flex-col overflow-hidden rounded-2xl border border-white/60 bg-white/95 shadow-xl shadow-slate-300/40 backdrop-blur-sm transition-all duration-300 hover:shadow-2xl hover:shadow-slate-400/50 dark:border-gray-700/50 dark:bg-gray-800/80 dark:shadow-gray-900/20 dark:hover:shadow-gray-900/30"
      >
        <!-- 文件信息标题栏 -->
        <div
          class="flex-shrink-0 border-b border-slate-200 bg-gradient-to-r from-slate-50 to-gray-100 px-6 py-4 dark:border-gray-700/50 dark:from-gray-800/50 dark:to-gray-700/50"
        >
          <!-- 三列布局：左侧文件信息，中间LogSeek标志，右侧下载按钮 -->
          <div class="grid grid-cols-[1fr_auto_1fr] items-center gap-4">
            <!-- 左侧：文件信息 -->
            <div class="min-w-0">
              <h2
                class="font-mono text-sm leading-tight font-semibold break-all text-slate-900 md:text-base dark:text-gray-100"
              >
                {extractFileName(file)}
              </h2>
              {#if extractTarName(file)}
                <p class="mt-0.5 font-mono text-[11px] break-all text-slate-500 dark:text-gray-400">
                  来自: {extractTarName(file)}
                </p>
              {/if}
              {#if total > 0}
                <div class="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-slate-600 dark:text-gray-400">
                  <span
                    class="inline-flex items-center rounded-md bg-blue-50 px-2 py-0.5 text-[10px] font-medium text-blue-700 ring-1 ring-blue-200 dark:bg-blue-900/30 dark:text-blue-300 dark:ring-blue-800"
                  >
                    <svg class="mr-1 h-2.5 w-2.5" viewBox="0 0 24 24" stroke="currentColor">
                      <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="2"
                        d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                      />
                    </svg>
                    总共 {total} 行
                  </span>
                  <span
                    class="inline-flex items-center rounded-md bg-green-50 px-2 py-0.5 text-[10px] font-medium text-green-700 ring-1 ring-green-200 dark:bg-green-900/30 dark:text-green-300 dark:ring-green-800"
                  >
                    <svg class="mr-1 h-2.5 w-2.5" viewBox="0 0 24 24" stroke="currentColor">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4" />
                    </svg>
                    已加载 {end} 行
                  </span>

                  {#if keywords?.length}
                    <!-- 关键词显示 -->
                    {#each keywords.slice(0, 3) as keyword (keyword)}
                      <span
                        class="inline-flex items-center rounded-md bg-yellow-50 px-2 py-0.5 text-[10px] font-medium text-yellow-800 ring-1 ring-yellow-600/20 dark:bg-yellow-900/30 dark:text-yellow-300 dark:ring-yellow-500/20"
                        >{keyword}</span
                      >
                    {/each}
                    {#if keywords.length > 3}
                      <span class="text-[10px] text-gray-500 dark:text-gray-400">+{keywords.length - 3}</span>
                    {/if}
                  {/if}
                </div>
              {/if}
            </div>

            <!-- 中间：LogSeek标志 -->
            <div class="flex items-center justify-center">
              <div
                class="inline-block transform text-3xl font-extrabold tracking-[-0.25em] italic antialiased transition-transform duration-300 select-none hover:scale-105 md:text-4xl"
              >
                <span class="text-blue-600 drop-shadow-sm">L</span>
                <span class="text-red-600 drop-shadow-sm">o</span>
                <span class="text-yellow-500 drop-shadow-sm">g</span>
                <span class="text-green-600 drop-shadow-sm">S</span>
                <span class="text-blue-600 drop-shadow-sm">e</span>
                <span class="text-red-600 drop-shadow-sm">e</span>
                <span class="text-yellow-500 drop-shadow-sm">k</span>
              </div>
            </div>

            <!-- 右侧：下载按钮 -->
            <div class="flex justify-end">
              <button
                class="inline-flex items-center rounded-lg bg-gradient-to-r from-blue-600 to-blue-700 px-3 py-2 text-sm font-semibold text-white shadow-lg shadow-blue-500/25 transition-all duration-300 hover:from-blue-700 hover:to-blue-800 hover:shadow-xl hover:shadow-blue-500/30 focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:ring-offset-gray-50 focus:outline-none disabled:from-gray-400 disabled:to-gray-500 disabled:shadow-gray-400/25 dark:focus:ring-offset-gray-900 dark:disabled:from-gray-600 dark:disabled:to-gray-700"
                onclick={downloadCurrentFile}
                disabled={loading || total <= 0}
                title="下载当前文件"
              >
                <svg class="mr-1.5 h-4 w-4" viewBox="0 0 24 24" stroke="currentColor">
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                  />
                </svg>
                下载
              </button>
            </div>
          </div>
        </div>

        <!-- 虚拟滚动内容区域 -->
        <div
          class="ps flex-1 bg-gradient-to-r from-slate-50 to-white transition-all duration-500 ease-in-out dark:from-gray-900/50 dark:to-gray-800/50"
          bind:this={parentEl}
          onscroll={handleScroll}
        >
          <!-- 始终渲染 spacer：若虚拟器未就绪，使用兜底总高度，确保可滚动区域存在 -->
          <div style="height: {$rowVirtualizer.getTotalSize()}px; width: 100%; position: relative;">
            {#if vItems.length === 0}
              <!-- 兜底：虚拟器未就绪或总高度未知时，先用正常流渲染已加载的前200行，避免空白 -->
              {#if lines.length > 0}
                {#each lines as ln (ln.no)}
                  <div
                    class="group/line grid grid-cols-[70px_1fr] font-mono text-xs leading-[16px] transition-colors duration-150 hover:bg-blue-50/40 dark:hover:bg-blue-900/10"
                  >
                    <div
                      class="border-r border-slate-300 bg-gradient-to-r from-slate-100 to-slate-50 px-3 py-0.5 text-right font-medium text-slate-600 transition-all duration-150 select-none group-hover/line:from-blue-100 group-hover/line:to-blue-50 group-hover/line:text-blue-700 dark:border-gray-700/60 dark:from-gray-800/80 dark:to-gray-900/80 dark:text-gray-400 dark:group-hover/line:from-blue-900/20 dark:group-hover/line:to-blue-800/20 dark:group-hover/line:text-blue-400"
                    >
                      {ln.no}
                    </div>
                    <div
                      class="code-content bg-white px-3 py-0.5 break-all whitespace-pre-wrap text-slate-900 transition-colors duration-150 group-hover/line:bg-blue-50/20 group-hover/line:text-slate-950 dark:bg-transparent dark:text-gray-200 dark:group-hover/line:text-gray-100"
                    >
                      {@html highlightKeywords(ln.text)}
                    </div>
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
                    <div
                      class="group/line grid grid-cols-[70px_1fr] font-mono text-xs leading-[16px] transition-colors duration-150 hover:bg-blue-50/40 dark:hover:bg-blue-900/10"
                    >
                      <div
                        class="border-r border-slate-300 bg-gradient-to-r from-slate-100 to-slate-50 px-3 py-0.5 text-right font-medium text-slate-600 transition-all duration-150 select-none group-hover/line:from-blue-100 group-hover/line:to-blue-50 group-hover/line:text-blue-700 dark:border-gray-700/60 dark:from-gray-800/80 dark:to-gray-900/80 dark:text-gray-400 dark:group-hover/line:from-blue-900/20 dark:group-hover/line:to-blue-800/20 dark:group-hover/line:text-blue-400"
                      >
                        {ln.no}
                      </div>
                      <div
                        class="code-content bg-white px-3 py-0.5 break-all whitespace-pre-wrap text-slate-900 transition-colors duration-150 group-hover/line:bg-blue-50/20 group-hover/line:text-slate-950 dark:bg-transparent dark:text-gray-200 dark:group-hover/line:text-gray-100"
                      >
                        {@html highlightKeywords(ln.text)}
                      </div>
                    </div>
                  {:else}
                    <!-- 中文注释：占位行（尚未加载到该行），高度尽量匹配 estimateSize -->
                    <div class="grid grid-cols-[70px_1fr] font-mono text-xs leading-[16px] opacity-60">
                      <div
                        class="border-r border-slate-300 bg-gradient-to-r from-slate-100 to-slate-50 px-3 py-0.5 text-right font-medium text-slate-400 select-none dark:border-gray-700/60 dark:from-gray-800/80 dark:to-gray-900/80 dark:text-gray-600"
                      >
                        {item.index + 1}
                      </div>
                      <div
                        class="code-content bg-white px-3 py-0.5 text-slate-500 dark:bg-transparent dark:text-gray-500"
                      >
                        加载中…
                      </div>
                    </div>
                  {/if}
                </div>
              {/each}
            {/if}
          </div>
        </div>
      </div>
    {:else}
      <!-- SSR 兖底：仅渲染已加载部分，避免 SSR 阶段报错/空白 -->
      <div
        class="my-6 flex flex-1 flex-col overflow-hidden rounded-2xl border border-white/60 bg-white/95 shadow-xl shadow-slate-300/40 backdrop-blur-sm transition-all duration-300 hover:shadow-2xl hover:shadow-slate-400/50 dark:border-gray-700/50 dark:bg-gray-800/80 dark:shadow-gray-900/20 dark:hover:shadow-gray-900/30"
      >
        <!-- 文件信息标题栏 -->
        <div
          class="flex-shrink-0 border-b border-slate-200 bg-gradient-to-r from-slate-50 to-gray-100 px-6 py-4 dark:border-gray-700/50 dark:from-gray-800/50 dark:to-gray-700/50"
        >
          <!-- 三列布局：左侧文件信息，中间LogSeek标志，右侧下载按钮 -->
          <div class="grid grid-cols-[1fr_auto_1fr] items-center gap-4">
            <!-- 左侧：文件信息 -->
            <div class="min-w-0">
              <h2
                class="font-mono text-sm leading-tight font-semibold break-all text-slate-900 md:text-base dark:text-gray-100"
              >
                {extractFileName(file)}
              </h2>
              {#if extractTarName(file)}
                <p class="mt-0.5 font-mono text-[11px] break-all text-slate-500 dark:text-gray-400">
                  来自: {extractTarName(file)}
                </p>
              {/if}
              {#if total > 0}
                <div class="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-slate-600 dark:text-gray-400">
                  <span
                    class="inline-flex items-center rounded-md bg-blue-50 px-2 py-0.5 text-[10px] font-medium text-blue-700 ring-1 ring-blue-200 dark:bg-blue-900/30 dark:text-blue-300 dark:ring-blue-800"
                  >
                    <svg class="mr-1 h-2.5 w-2.5" viewBox="0 0 24 24" stroke="currentColor">
                      <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="2"
                        d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                      />
                    </svg>
                    {total}
                  </span>
                  <span
                    class="inline-flex items-center rounded-md bg-green-50 px-2 py-0.5 text-[10px] font-medium text-green-700 ring-1 ring-green-200 dark:bg-green-900/30 dark:text-green-300 dark:ring-green-800"
                  >
                    <svg class="mr-1 h-2.5 w-2.5" viewBox="0 0 24 24" stroke="currentColor">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4" />
                    </svg>
                    {end}
                  </span>

                  {#if keywords?.length}
                    <!-- 关键词显示 -->
                    {#each keywords.slice(0, 3) as keyword (keyword)}
                      <span
                        class="inline-flex items-center rounded-md bg-yellow-50 px-2 py-0.5 text-[10px] font-medium text-yellow-800 ring-1 ring-yellow-600/20 dark:bg-yellow-900/30 dark:text-yellow-300 dark:ring-yellow-500/20"
                        >{keyword}</span
                      >
                    {/each}
                    {#if keywords.length > 3}
                      <span class="text-[10px] text-gray-500 dark:text-gray-400">+{keywords.length - 3}</span>
                    {/if}
                  {/if}
                </div>
              {/if}
            </div>

            <!-- 中间：LogSeek标志 -->
            <div class="flex items-center justify-center">
              <div
                class="inline-block transform text-3xl font-extrabold tracking-[-0.25em] italic antialiased transition-transform duration-300 select-none hover:scale-105 md:text-4xl"
              >
                <span class="text-blue-600 drop-shadow-sm">L</span>
                <span class="text-red-600 drop-shadow-sm">o</span>
                <span class="text-yellow-500 drop-shadow-sm">g</span>
                <span class="text-green-600 drop-shadow-sm">S</span>
                <span class="text-blue-600 drop-shadow-sm">e</span>
                <span class="text-red-600 drop-shadow-sm">e</span>
                <span class="text-yellow-500 drop-shadow-sm">k</span>
              </div>
            </div>

            <!-- 右侧：下载按钮 -->
            <div class="flex justify-end">
              <button
                class="inline-flex items-center rounded-lg bg-gradient-to-r from-blue-600 to-blue-700 px-3 py-2 text-sm font-semibold text-white shadow-lg shadow-blue-500/25 transition-all duration-300 hover:from-blue-700 hover:to-blue-800 hover:shadow-xl hover:shadow-blue-500/30 focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:ring-offset-gray-50 focus:outline-none disabled:from-gray-400 disabled:to-gray-500 disabled:shadow-gray-400/25 dark:focus:ring-offset-gray-900 dark:disabled:from-gray-600 dark:disabled:to-gray-700"
                onclick={downloadCurrentFile}
                disabled={loading || total <= 0}
                title="下载当前文件"
              >
                <svg class="mr-1.5 h-4 w-4" viewBox="0 0 24 24" stroke="currentColor">
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                  />
                </svg>
                下载
              </button>
            </div>
          </div>
        </div>

        <!-- 内容区域 -->
        <div
          class="flex-1 overflow-auto bg-gradient-to-r from-slate-50 to-white transition-all duration-500 ease-in-out dark:from-gray-900/50 dark:to-gray-800/50"
        >
          {#each lines as ln (ln.no)}
            <div
              class="group/line grid grid-cols-[70px_1fr] font-mono text-xs leading-[16px] transition-colors duration-150 hover:bg-blue-50/40 dark:hover:bg-blue-900/10"
            >
              <div
                class="border-r border-slate-300 bg-gradient-to-r from-slate-100 to-slate-50 px-3 py-0.5 text-right font-medium text-slate-600 transition-all duration-150 select-none group-hover/line:from-blue-100 group-hover/line:to-blue-50 group-hover/line:text-blue-700 dark:border-gray-700/60 dark:from-gray-800/80 dark:to-gray-900/80 dark:text-gray-400 dark:group-hover/line:from-blue-900/20 dark:group-hover/line:to-blue-800/20 dark:group-hover/line:text-blue-400"
              >
                {ln.no}
              </div>
              <div
                class="code-content bg-white px-3 py-0.5 break-all whitespace-pre-wrap text-slate-900 transition-colors duration-150 group-hover/line:bg-blue-50/20 group-hover/line:text-slate-950 dark:bg-transparent dark:text-gray-200 dark:group-hover/line:text-gray-100"
              >
                {@html highlightKeywords(ln.text)}
              </div>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .code-content {
    font-family: var(--font-ui), monospace;
    font-feature-settings:
      'liga' 0,
      'calt' 0;
    font-variant-ligatures: none;
  }
</style>
