<script lang="ts">
  /**
   * 文件查看页面（重构版）
   * 使用 LogSeek 模块的 API 客户端和工具函数
   */
  import { onMount, onDestroy } from 'svelte';
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import { get } from 'svelte/store';
  import { browser } from '$app/environment';
  import { fetchViewCache, escapeHtml, escapeRegExp } from '$lib/modules/logseek';
  import { getDisplayName } from '$lib/modules/logseek/utils/fileUrl';
  import Alert from '$lib/components/Alert.svelte';
  import FileHeader from './FileHeader.svelte';
  import LogSeekLogo from '$lib/components/LogSeekLogo.svelte';

  let file = $state('');
  let sid = $state('');
  let total = $state(0);
  let end = $state(0);
  let keywords = $state<string[]>([]);
  let lines = $state<{ no: number; text: string }[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  // 虚拟滚动容器引用
  let parentEl = $state<HTMLDivElement | null>(null);
  // 统一行高预估，供虚拟器与兜底高度计算复用
  // 行样式为 text-[13px] + leading-[20px] + py-1，综合约 32px
  const EST_ROW = 32;
  const rowVirtualizer = createVirtualizer({
    count: 0, // 初始值为 0，会在下面的 $effect 中响应式更新为实际的 total
    getScrollElement: () => parentEl, // 滚动容器（初始为 null，下面用 setOptions 保持同步）
    estimateSize: () => EST_ROW, // 预估单行高度(px)
    overscan: 20, // 预加载额外行，平衡性能与滚动流畅度
    // 启用真实高度测量，避免底部出现多余空白或无法触底
    measureElement: (el: HTMLElement) => el.getBoundingClientRect().height
  });

  // 确保在 parentEl 绑定后，虚拟器获得滚动容器引用
  $effect(() => {
    if (!browser) return;
    try {
      const v = get(rowVirtualizer);
      v?.setOptions?.({
        getScrollElement: () => parentEl,
        count: total
      });
    } catch {
      // 忽略虚拟器配置错误
    }
  });

  type VirtualItem = { index: number; start: number; key: string | number | bigint };
  // 虚拟项缓存，避免在模板中使用 {@const}
  const vItems: VirtualItem[] = $derived(browser ? $rowVirtualizer.getVirtualItems() : []);

  // 统一调度虚拟器测量
  function scheduleVirtualUpdate() {
    if (!browser) return;
    requestAnimationFrame(() => {
      try {
        get(rowVirtualizer)?.measure?.();
      } catch {
        // 忽略测量错误
      }
    });
  }

  // 滚动触底兜底（即使虚拟器未就绪也能触发加载更多）
  function handleScroll() {
    if (!browser || loading || !parentEl) return;
    const el = parentEl;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 200 && end < total) {
      loadMore();
    }
  }

  // 根据 index 取对应行（未加载则返回 null）
  function getLineByIndex(idx0: number): { no: number; text: string } | null {
    const lineNo = idx0 + 1; // TanStack 使用 0-based，这里转为 1-based 行号
    const rec = lines[lineNo - 1];
    // 放宽校验，若数组存在对应项则直接返回，避免因行号不匹配导致空白
    return rec ?? null;
  }

  // 轻量行高测量，消除折行遮挡问题
  function measureVirtualRow(node: HTMLElement) {
    if (!browser) return {};
    const virtualizer = get(rowVirtualizer);
    if (!virtualizer) return {};

    let frame = -1;
    const schedule = () => {
      if (frame !== -1) cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        try {
          virtualizer.measureElement?.(node);
        } catch {
          // 忽略测量元素错误
        }
      });
    };

    schedule();

    return {
      update: () => schedule(),
      destroy: () => {
        if (frame !== -1) cancelAnimationFrame(frame);
      }
    };
  }

  // 接近底部自动加载更多
  $effect(() => {
    if (!browser) return;
    const items = $rowVirtualizer.getVirtualItems();
    if (items.length && total > 0 && !loading) {
      const maxIndex = items[items.length - 1].index; // 0-based
      const maxLineNo = maxIndex + 1; // 1-based
      if (maxLineNo > end - 50 && end < total) {
        // 当视口接近已加载末尾 50 行内时，触发加载更多
        loadMore();
      }
    }
  });

  async function fetchRange(s: number, e: number) {
    return await fetchViewCache(sid, file, s, e);
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
        end = 0;
        lines = [];
        return;
      }

      // 第二步：一次性加载全部行
      const full = await fetchRange(1, total);
      end = full.end;
      keywords = full.keywords || [];
      lines = full.lines || [];

      // 数据填充后强制重新测量，确保总高度与内容精确匹配
      scheduleVirtualUpdate();
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
      scheduleVirtualUpdate();
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message || '加载更多失败';
    } finally {
      loading = false;
    }
  }

  // 关键词高亮函数（使用 LogSeek 模块的工具函数）
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
        result = result.replace(regex, (match: string) => {
          return `<mark class="highlight">${match}</mark>`;
        });
      }
    }

    return result;
  }

  // 检测行是否包含关键词（用于高亮行号）
  function lineHasMatch(text: string): boolean {
    if (!keywords || keywords.length === 0 || !text) return false;
    return keywords.some((kw) => kw && text.includes(kw));
  }

  // 下载当前视图的完整原始文本（不含行号）
  function downloadCurrentFile() {
    try {
      if (!lines || lines.length === 0) return;
      const content = lines.map((ln) => ln?.text ?? '').join('\n');
      const blob = new Blob([content], { type: 'text/plain;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      // 提取文件名并清理非法字符
      const fileName = getDisplayName(file).replace(/[\\/:*?"<>|]+/g, '_') || 'log.txt';
      a.download = fileName;
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

    // 首屏渲染后，确保虚拟器定位到起始行，避免初次 items 为空
    // 等一帧再滚动，确保容器与虚拟器完成初始化
    if (browser) {
      requestAnimationFrame(() => {
        try {
          const v = get(rowVirtualizer);
          v?.scrollToIndex?.(0);
        } catch {
          // 忽略滚动初始化错误
        }
      });
    }

    // 使用原生滚动条时，确保容器允许滚动
    const el = parentEl;
    if (browser && el) {
      el.style.overflow = el.style.overflow || 'auto';
      el.style.position = el.style.position || 'relative';
    }

    const handleKeydown = (event: KeyboardEvent) => {
      if (!browser) return;
      const el = parentEl;
      if (!el) return;
      if (!event.ctrlKey || event.metaKey || event.altKey) return;

      const target = event.target as HTMLElement | null;
      if (target) {
        const tag = target.tagName;
        if (tag === 'INPUT' || tag === 'TEXTAREA' || target.isContentEditable) {
          return;
        }
      }

      const viewHeight = el.clientHeight;
      const half = viewHeight / 2;
      const full = viewHeight;

      const scrollBy = (delta: number) => {
        const next = Math.max(0, Math.min(el.scrollHeight, el.scrollTop + delta));
        el.scrollTo({ top: next, behavior: 'smooth' });
      };

      const scrollToEdge = (edge: 'top' | 'bottom') => {
        const top = edge === 'top' ? 0 : el.scrollHeight;
        el.scrollTo({ top, behavior: 'smooth' });
      };

      switch (event.key.toLowerCase()) {
        case 'g':
          event.preventDefault();
          if (event.shiftKey) scrollToEdge('top');
          else scrollToEdge('bottom');
          break;
        case 'u':
          event.preventDefault();
          scrollBy(-half);
          break;
        case 'd':
          event.preventDefault();
          scrollBy(half);
          break;
        case 'b':
          event.preventDefault();
          scrollBy(-full);
          break;
        case 'f':
          event.preventDefault();
          scrollBy(full);
          break;
        default:
          break;
      }
    };

    window.addEventListener('keydown', handleKeydown);

    return () => {
      window.removeEventListener('keydown', handleKeydown);
    };
  });

  onDestroy(() => {
    // 当前使用原生滚动条，无需销毁额外实例
  });
</script>

<!-- 页面标题与状态栏 -->
<div class="bg-background text-foreground min-h-screen">
  <div class="mx-auto flex h-screen max-w-[1560px] flex-col px-6 py-6">
    {#if error}
      <div class="mx-auto mb-6 max-w-md">
        <Alert type="error" title="加载出错" message={error} />
      </div>
    {/if}

    <div class="flex min-h-0 flex-1 flex-col gap-6">
      <!-- 页面 Logo（卡片外部） -->
      <div class="flex items-center justify-center">
        <LogSeekLogo size="small" hoverable />
      </div>
      <!-- 主内容卡片：文件信息 + 虚拟滚动容器 -->
      <div
        class="border-border bg-card flex flex-1 flex-col overflow-hidden rounded-md border transition-all dark:border-gray-700 dark:bg-[#0d1117]"
      >
        <!-- 文件信息标题栏 -->
        <FileHeader filePath={file} {total} loadedLines={end} {keywords} {loading} onDownload={downloadCurrentFile} />

        <!-- 虚拟滚动内容区域 -->
        <div
          class="bg-background relative min-h-0 flex-1 overflow-auto dark:bg-[#0d1117]"
          bind:this={parentEl}
          onscroll={handleScroll}
        >
          <!-- 始终渲染 spacer：若虚拟器未就绪，使用兜底总高度，确保可滚动区域存在 -->
          <div style="height: {$rowVirtualizer.getTotalSize()}px; width: 100%; position: relative;">
            {#if vItems.length === 0}
              <!-- 兜底：虚拟器未就绪或总高度未知时，先用正常流渲染已加载的前200行，避免空白 -->
              {#if lines.length > 0}
                {#each lines as ln (ln.no)}
                  {@const isMatch = lineHasMatch(ln.text)}
                  <div class="group/line hover:bg-muted/10 flex font-mono text-xs leading-relaxed">
                    <div
                      class="w-[50px] shrink-0 select-none px-3 py-0.5 text-right font-medium {isMatch
                        ? 'text-foreground font-semibold'
                        : 'text-muted-foreground/60'}"
                    >
                      {ln.no}
                    </div>
                    <div class="code-content text-foreground flex-1 whitespace-pre-wrap break-all px-4 py-0.5">
                      {@html highlightKeywords(ln.text)}
                    </div>
                  </div>
                {/each}
              {:else}
                <div class="text-muted-foreground p-3 text-sm">暂无内容</div>
              {/if}
            {:else}
              {#each vItems as item (item.key)}
                {@const ln = getLineByIndex(item.index)}
                <div
                  style="position: absolute; top: 0; left: 0; width: 100%; transform: translateY({item.start}px);"
                  data-index={item.index}
                  use:measureVirtualRow
                >
                  {#if ln}
                    {@const isMatch = lineHasMatch(ln.text)}
                    <div class="group/line hover:bg-muted/10 flex font-mono text-xs leading-relaxed">
                      <div
                        class="w-[50px] shrink-0 select-none px-3 py-0.5 text-right font-medium {isMatch
                          ? 'text-foreground font-semibold'
                          : 'text-muted-foreground/60'}"
                      >
                        {ln.no}
                      </div>
                      <div class="code-content text-foreground flex-1 whitespace-pre-wrap break-all px-4 py-0.5">
                        {@html highlightKeywords(ln.text)}
                      </div>
                    </div>
                  {:else}
                    <!-- 占位行（尚未加载到该行），高度尽量匹配 estimateSize -->
                    <div class="flex font-mono text-xs leading-relaxed opacity-60">
                      <div
                        class="text-muted-foreground w-[50px] shrink-0 select-none px-3 py-0.5 text-right font-medium"
                      >
                        {item.index + 1}
                      </div>
                      <div class="code-content text-muted-foreground flex-1 px-4 py-0.5">加载中…</div>
                    </div>
                  {/if}
                </div>
              {/each}
            {/if}
          </div>
        </div>
      </div>
    </div>
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

  /* 关键词高亮样式 - 字体颜色高亮 */
  :global(.highlight) {
    background: none;
    color: #d97706; /* amber-600 */
    font-weight: 600;
  }

  :global(.dark .highlight) {
    background: none;
    color: #fbbf24; /* amber-400 */
  }
</style>
