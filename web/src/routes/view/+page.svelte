<script lang="ts">
  /**
   * 文件查看页面（GitHub 风格重构版）
   * 文件内容查看器（虚拟滚动）
   */
  import { onMount, onDestroy } from 'svelte';
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import { get } from 'svelte/store';
  import { browser } from '$app/environment';
  import { fetchViewCache, escapeHtml, escapeRegExp } from '$lib/modules/logseek';
  import { highlight } from '$lib/modules/logseek/utils/highlight';
  import type { KeywordInfo } from '$lib/modules/logseek/types';
  import { getDisplayName } from '$lib/modules/logseek/utils/fileUrl';
  import Alert from '$lib/components/Alert.svelte';
  import FileHeader from './FileHeader.svelte';
  import LogSeekLogo from '$lib/components/LogSeekLogo.svelte';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import { Button } from '$lib/components/ui/button';
  import { FileText, ChevronLeft, Loader2, ChevronRight } from 'lucide-svelte';

  // URL 参数
  let sid = $state('');
  let initialFile = $state('');

  // 当前查看的文件
  let currentFile = $state('');
  let total = $state(0);
  let end = $state(0);
  let keywords = $state<string[]>([]);
  let lines = $state<{ no: number; text: string }[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  // 虚拟滚动容器引用
  let parentEl = $state<HTMLDivElement | null>(null);
  const EST_ROW = 20;
  const rowVirtualizer = createVirtualizer({
    count: 0,
    getScrollElement: () => parentEl,
    estimateSize: () => EST_ROW,
    overscan: 50,
    measureElement: (el: HTMLElement) => el.getBoundingClientRect().height
  });

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
  const vItems: VirtualItem[] = $derived(browser ? $rowVirtualizer.getVirtualItems() : []);

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

  function handleScroll() {
    if (!browser || loading || !parentEl) return;
    const el = parentEl;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 200 && end < total) {
      loadMore();
    }
  }

  function getLineByIndex(idx0: number): { no: number; text: string } | null {
    const lineNo = idx0 + 1;
    const rec = lines[lineNo - 1];
    return rec ?? null;
  }

  function measureVirtualRow(node: HTMLElement) {
    if (!browser) return {};
    const virtualizer = get(rowVirtualizer);
    if (!virtualizer) return {};

    const ro = new ResizeObserver(() => {
      virtualizer.measureElement(node);
    });

    ro.observe(node);

    return {
      destroy: () => {
        ro.disconnect();
      }
    };
  }

  $effect(() => {
    if (!browser) return;
    const items = $rowVirtualizer.getVirtualItems();
    if (items.length && total > 0 && !loading) {
      const maxIndex = items[items.length - 1].index;
      const maxLineNo = maxIndex + 1;
      if (maxLineNo > end - 50 && end < total) {
        loadMore();
      }
    }
  });

  async function fetchRange(s: number, e: number) {
    return await fetchViewCache(sid, currentFile, s, e);
  }

  async function loadFileContent(filePath: string) {
    if (filePath === currentFile) return; // 已经是当前文件

    try {
      currentFile = filePath;
      loading = true;
      error = null;
      lines = [];
      total = 0;
      end = 0;

      // 获取文件元数据
      const meta = await fetchRange(1, 1);
      total = meta.total;

      if (total <= 0) {
        end = 0;
        lines = [];
        return;
      }

      // 加载全部内容
      const full = await fetchRange(1, total);
      end = full.end;
      keywords = full.keywords || [];
      lines = full.lines || [];

      scheduleVirtualUpdate();

      // 滚动到顶部
      if (browser && parentEl) {
        parentEl.scrollTop = 0;
        try {
          const v = get(rowVirtualizer);
          v?.scrollToIndex?.(0);
        } catch {
          // 忽略滚动错误
        }
      }
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message || '加载文件失败';
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

  function highlightKeywords(text: string): string {
    if (!keywords || keywords.length === 0 || !text) {
      return escapeHtml(text);
    }

    // 将 keywords 字符串数组转换为 KeywordInfo 数组（默认都是 Literal，不区分大小写）
    const keywordInfos: KeywordInfo[] = keywords.map((kw) => ({ type: 'literal', text: kw }));
    return highlight(text, keywordInfos);
  }

  function lineHasMatch(text: string): boolean {
    if (!keywords || keywords.length === 0 || !text) return false;
    return keywords.some((kw) => kw && text.includes(kw));
  }

  function downloadCurrentFile() {
    try {
      if (!lines || lines.length === 0) return;
      const content = lines.map((ln) => ln?.text ?? '').join('\n');
      const blob = new Blob([content], { type: 'text/plain;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      const fileName = getDisplayName(currentFile).replace(/[\\/:*?"<>|]+/g, '_') || 'log.txt';
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
    initialFile = (params.get('file') || '').trim();
    sid = (params.get('sid') || '').trim();

    if (!sid) {
      error = '缺少 sid 参数';
      return;
    }

    // 如果有初始文件，加载它
    if (initialFile) {
      loadFileContent(initialFile);
    }

    // 键盘快捷键
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
    // 清理
  });
</script>

<div class="bg-background text-foreground flex h-screen flex-col">
  <!-- 顶部导航栏 -->
  <header
    class="border-border bg-background/95 supports-backdrop-filter:bg-background/60 sticky top-0 z-50 w-full border-b backdrop-blur"
  >
    <div class="flex h-16 w-full items-center gap-4 px-6">
      <!-- 左侧：Logo -->
      <a href="/" class="flex items-center gap-2 transition-opacity hover:opacity-80">
        <LogSeekLogo size="small" />
      </a>

      <!-- 中间：占位 -->
      <div class="flex-1 px-4"></div>

      <!-- 右侧：操作区 -->
      <div class="flex items-center gap-2">
        <Settings />
        <ThemeToggle />
      </div>
    </div>
  </header>

  <!-- 主内容区域 -->
  <div class="flex min-h-0 flex-1">
    <!-- 文件内容 -->
    <main class="flex min-w-0 flex-1 flex-col">
      {#if error}
        <div class="p-6">
          <Alert type="error" title="加载出错" message={error} />
        </div>
      {/if}

      {#if currentFile}
        <div class="flex min-h-0 flex-1 flex-col">
          <!-- 文件信息标题栏 -->
          <FileHeader
            filePath={currentFile}
            {total}
            loadedLines={end}
            {keywords}
            {loading}
            onDownload={downloadCurrentFile}
          />

          <!-- 虚拟滚动内容区域 -->
          <div
            class="bg-background relative min-h-0 flex-1 overflow-auto dark:bg-[#0d1117]"
            bind:this={parentEl}
            onscroll={handleScroll}
          >
            <div style="height: {$rowVirtualizer.getTotalSize()}px; width: 100%; position: relative;">
              {#if vItems.length === 0}
                {#if lines.length > 0}
                  {#each lines as ln (ln.no)}
                    {@const isMatch = lineHasMatch(ln.text)}
                    <div class="group/line hover:bg-muted/10 flex font-mono text-xs leading-5">
                      <div
                        class="w-[50px] shrink-0 select-none px-3 py-0.5 text-right font-medium {isMatch
                          ? 'text-foreground font-semibold'
                          : 'text-muted-foreground/60'}"
                      >
                        {ln.no}
                      </div>
                      <div class="code-content text-foreground flex-1 whitespace-pre-wrap break-all px-4">
                        {@html highlightKeywords(ln.text)}
                      </div>
                    </div>
                  {/each}
                {:else}
                  <div class="flex h-full items-center justify-center p-10">
                    {#if loading}
                      <div class="text-muted-foreground flex flex-col items-center gap-2">
                        <Loader2 class="h-8 w-8 animate-spin" />
                        <span class="text-sm">加载中...</span>
                      </div>
                    {:else}
                      <div class="text-muted-foreground text-sm">暂无内容</div>
                    {/if}
                  </div>
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
                      <div class="group/line hover:bg-muted/10 flex font-mono text-xs leading-5">
                        <div
                          class="w-[50px] shrink-0 select-none px-3 text-right font-medium {isMatch
                            ? 'text-foreground font-semibold'
                            : 'text-muted-foreground/60'}"
                        >
                          {ln.no}
                        </div>
                        <div class="code-content text-foreground flex-1 whitespace-pre-wrap break-all px-4">
                          {@html highlightKeywords(ln.text)}
                        </div>
                      </div>
                    {:else}
                      <div class="flex font-mono text-xs leading-relaxed opacity-60">
                        <div class="text-muted-foreground w-[50px] shrink-0 select-none px-3 text-right font-medium">
                          {item.index + 1}
                        </div>
                        <div class="code-content text-muted-foreground flex-1 px-4">加载中…</div>
                      </div>
                    {/if}
                  </div>
                {/each}
              {/if}
            </div>
          </div>
        </div>
      {:else}
        <div class="flex flex-1 items-center justify-center">
          <div class="text-center">
            <FileText class="text-muted-foreground/50 mx-auto h-12 w-12" />
            <p class="text-muted-foreground mt-4 text-sm">没有文件可查看</p>
          </div>
        </div>
      {/if}
    </main>
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

  :global(.highlight) {
    background: none;
    color: #d97706;
    font-weight: 600;
  }

  :global(.dark .highlight) {
    background: none;
    color: #fbbf24;
  }
</style>
