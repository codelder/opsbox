<script lang="ts">
  /**
   * 文件查看页面（GitHub 风格重构版）
   * 文件内容查看器（虚拟滚动）
   */
  import { onMount, onDestroy } from 'svelte';
  import { SvelteMap, SvelteSet } from 'svelte/reactivity';
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import { get } from 'svelte/store';
  import { browser } from '$app/environment';
  import { resolve } from '$app/paths';
  import { fetchViewCache, fetchViewDownload, escapeHtml } from '$lib/modules/logseek';
  import { highlight } from '$lib/modules/logseek/utils/highlight';
  import type { KeywordInfo } from '$lib/modules/logseek/types';
  import { getDisplayName, parseFileUrl } from '$lib/modules/logseek/utils/fileUrl';
  import Alert from '$lib/components/Alert.svelte';
  import FileHeader from './FileHeader.svelte';
  import LogSeekLogo from '$lib/components/LogSeekLogo.svelte';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import { FileText, LoaderCircle } from 'lucide-svelte';

  // URL 参数
  let sid = $state('');
  let initialFile = $state('');

  // 当前查看的文件
  let currentFile = $state('');
  let total = $state(0);
  let end = $state(0);
  let keywords = $state<KeywordInfo[]>([]);
  let lines = $state<{ no: number; text: string }[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let fontSize = $state('sm');

  // 字体大小类映射
  let fontSizeClass = $state('text-base whitespace-pre-wrap');

  $effect(() => {
    switch (fontSize) {
      case 'xs':
        fontSizeClass = 'text-xs whitespace-pre-wrap';
        break;
      case 'sm':
        fontSizeClass = 'text-sm whitespace-pre-wrap';
        break;
      case 'base':
        fontSizeClass = 'text-base whitespace-pre-wrap';
        break;
      case 'lg':
        fontSizeClass = 'text-lg whitespace-pre-wrap';
        break;
      case 'xl':
        fontSizeClass = 'text-xl whitespace-pre-wrap';
        break;
      default:
        fontSizeClass = 'text-base whitespace-pre-wrap';
    }
  });

  // 根据字体大小估算行高
  const estimatedRowHeight = $derived.by(() => {
    switch (fontSize) {
      case 'xs':
        return 18;
      case 'sm':
        return 20;
      case 'base':
        return 22;
      case 'lg':
        return 24;
      case 'xl':
        return 28;
      default:
        return 20;
    }
  });

  function handleFontSizeChange(newSize: string) {
    fontSize = newSize;
  }

  // 分块加载相关状态
  const CHUNK_SIZE = 1000;
  const MAX_CONCURRENT_CHUNKS = 3; // 最大并发加载块数
  const loadedChunks = new SvelteSet<number>();
  const loadingChunks = new SvelteSet<number>();
  const pendingChunkLoads = new SvelteSet<number>(); // 等待加载的块
  let lastChunkCheckTime = $state(0);

  // 高亮缓存
  const highlightCache = new SvelteMap<string, string>();
  let cachedKeywordsHash = '';

  // 虚拟滚动容器引用
  let parentEl = $state<HTMLDivElement | null>(null);

  // 延迟测量队列 - 批量处理避免同步布局阻塞
  let measureQueue: HTMLElement[] = [];
  let measureScheduled = false;

  function scheduleMeasure() {
    if (measureScheduled || measureQueue.length === 0) return;
    measureScheduled = true;

    // 使用 requestIdleCallback 在浏览器空闲时批量测量，避免阻塞滚动
    const callback = () => {
      measureScheduled = false;
      if (measureQueue.length === 0) return;

      const virtualizer = get(rowVirtualizer);
      if (!virtualizer) {
        measureQueue = [];
        return;
      }

      // 批量测量所有排队的元素
      const elements = measureQueue.splice(0, measureQueue.length);
      for (const el of elements) {
        if (el.isConnected) {
          virtualizer.measureElement(el);
        }
      }
    };

    if ('requestIdleCallback' in window) {
      requestIdleCallback(callback, { timeout: 500 });
    } else {
      setTimeout(callback, 100);
    }
  }

  function queueMeasure(el: HTMLElement) {
    measureQueue.push(el);
    scheduleMeasure();
  }

  const rowVirtualizer = createVirtualizer({
    count: 0,
    getScrollElement: () => parentEl,
    estimateSize: () => estimatedRowHeight,
    overscan: 5
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

  // 触发虚拟器更新
  function scheduleVirtualUpdate() {
    // 虚拟器会自动响应 count 变化，这里触发一次测量调度
    scheduleMeasure();
  }

  // 滚动防抖相关
  let scrollDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let lastScrollTop = 0;
  const SCROLL_THRESHOLD = 50; // 滚动至少50px才检查
  const SCROLL_DEBOUNCE_MS = 300; // 滚动停止后300ms再检查

  function handleScroll() {
    if (!browser || !parentEl) return;
    const el = parentEl;

    // 检查是否接近底部 - 使用估算高度避免读取scrollHeight（性能优化）
    if (total > 0) {
      const estimatedScrollHeight = total * estimatedRowHeight;
      if (el.scrollTop + el.clientHeight >= estimatedScrollHeight - 200 && end < total && !loading) {
        loadMore();
      }
    }

    // 检查滚动距离是否超过阈值
    const scrollDelta = Math.abs(el.scrollTop - lastScrollTop);
    if (scrollDelta < SCROLL_THRESHOLD) {
      return; // 滚动距离太小，跳过检查
    }
    lastScrollTop = el.scrollTop;

    // 使用防抖：清除之前的定时器，设置新的定时器
    if (scrollDebounceTimer) {
      clearTimeout(scrollDebounceTimer);
    }
    scrollDebounceTimer = setTimeout(() => {
      scrollDebounceTimer = null;
      checkVisibleChunks();
    }, SCROLL_DEBOUNCE_MS);
  }

  function checkVisibleChunks() {
    if (!browser || !total) return;

    // 限制检查频率，至少间隔200ms（增加间隔以减少调用频率）
    const now = Date.now();
    if (now - lastChunkCheckTime < 200) {
      return;
    }
    lastChunkCheckTime = now;

    try {
      // 使用 get() 获取虚拟器，避免响应式触发
      const virtualizer = get(rowVirtualizer);
      if (!virtualizer) return;

      const items = virtualizer.getVirtualItems();
      if (!items.length) return;

      // 收集虚拟items中涉及的所有块索引
      const neededChunks = new SvelteSet<number>();
      for (const item of items) {
        const lineNo = item.index + 1;
        const chunkIndex = getChunkIndex(lineNo);
        neededChunks.add(chunkIndex);
      }

      // 更新待加载块列表
      for (const chunkIndex of neededChunks) {
        if (!loadedChunks.has(chunkIndex) && !loadingChunks.has(chunkIndex) && !pendingChunkLoads.has(chunkIndex)) {
          pendingChunkLoads.add(chunkIndex);
        }
      }

      // 处理待加载块
      processPendingChunks();
    } catch {
      // 忽略错误
    }
  }

  function processPendingChunks() {
    if (!browser || !total) return;

    // 检查当前正在加载的块数量
    const currentLoadingCount = loadingChunks.size;
    if (currentLoadingCount >= MAX_CONCURRENT_CHUNKS) {
      return; // 已达到最大并发数
    }

    // 计算还可以加载多少个块
    const remainingSlots = MAX_CONCURRENT_CHUNKS - currentLoadingCount;
    if (remainingSlots <= 0) return;

    // 从待加载列表中取出最前面的几个块
    const chunksToLoad: number[] = [];
    for (const chunkIndex of pendingChunkLoads) {
      if (chunksToLoad.length >= remainingSlots) break;

      // 再次检查是否已经加载或正在加载（避免竞态条件）
      if (!loadedChunks.has(chunkIndex) && !loadingChunks.has(chunkIndex)) {
        chunksToLoad.push(chunkIndex);
      }
    }

    // 加载选中的块
    for (const chunkIndex of chunksToLoad) {
      pendingChunkLoads.delete(chunkIndex);
      loadChunk(chunkIndex);
    }
  }

  function getLineByIndex(idx0: number): { no: number; text: string } | null {
    const lineNo = idx0 + 1;
    const rec = lines[lineNo - 1];
    return rec ?? null;
  }

  // 延迟测量 action - 不立即测量，而是加入队列批量处理
  function measureVirtualRow(node: HTMLElement) {
    if (!browser) return {};

    // 加入测量队列，而不是立即测量
    queueMeasure(node);

    return {
      destroy: () => {
        // 从队列中移除（如果还在的话）
        const idx = measureQueue.indexOf(node);
        if (idx >= 0) {
          measureQueue.splice(idx, 1);
        }
      }
    };
  }

  async function fetchRange(s: number, e: number) {
    return await fetchViewCache(sid, currentFile, s, e);
  }

  // 计算行号所属的块索引
  function getChunkIndex(lineNo: number): number {
    return Math.floor((lineNo - 1) / CHUNK_SIZE);
  }

  // 计算块索引对应的行范围
  function getChunkRange(chunkIndex: number): { start: number; end: number } {
    const start = chunkIndex * CHUNK_SIZE + 1;
    const end = Math.min((chunkIndex + 1) * CHUNK_SIZE, total);
    return { start, end };
  }

  // 处理已加载的块数据，更新 lines 数组
  function processLoadedChunk(start: number, _dataEnd: number, chunkLines: { no: number; text: string }[]) {
    for (const line of chunkLines) {
      lines[line.no - 1] = line;
    }
    const chunkIndex = getChunkIndex(start);
    loadedChunks.add(chunkIndex);
    loadingChunks.delete(chunkIndex);
    pendingChunkLoads.delete(chunkIndex); // 确保从待加载列表中移除

    // 更新 end 为已加载的最大行号
    const maxLoadedLine = Math.max(...chunkLines.map((l) => l.no));
    if (maxLoadedLine > end) {
      end = maxLoadedLine;
    }
  }

  // 加载指定块索引
  async function loadChunk(chunkIndex: number) {
    if (loadedChunks.has(chunkIndex) || loadingChunks.has(chunkIndex)) {
      pendingChunkLoads.delete(chunkIndex); // 清理待加载列表
      return;
    }

    // 检查并发限制
    if (loadingChunks.size >= MAX_CONCURRENT_CHUNKS) {
      // 达到并发上限，加入待加载队列
      if (!pendingChunkLoads.has(chunkIndex)) {
        pendingChunkLoads.add(chunkIndex);
      }
      return;
    }

    loadingChunks.add(chunkIndex);
    pendingChunkLoads.delete(chunkIndex); // 开始加载时从待加载列表中移除

    try {
      const { start, end: chunkEnd } = getChunkRange(chunkIndex);
      const data = await fetchRange(start, chunkEnd);

      // 如果是第一个块，保存关键词
      if (chunkIndex === 0 && data.keywords && data.keywords.length > 0) {
        keywords = data.keywords;
      }

      processLoadedChunk(start, chunkEnd, data.lines || []);
      scheduleVirtualUpdate();

      // 加载完成后检查是否有待处理的块
      processPendingChunks();
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { message?: string }) : {};
      error = err.message || `加载块 ${chunkIndex} 失败`;
      loadingChunks.delete(chunkIndex);
      pendingChunkLoads.delete(chunkIndex); // 错误时也清理

      // 错误后也检查待处理块
      processPendingChunks();
    }
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
      loadedChunks.clear(); // Use .clear()
      loadingChunks.clear(); // Use .clear()
      pendingChunkLoads.clear(); // Use .clear()
      lastChunkCheckTime = 0;
      highlightCache.clear();
      cachedKeywordsHash = '';

      // 获取文件元数据
      const meta = await fetchRange(1, 1);
      total = meta.total;

      if (total <= 0) {
        end = 0;
        lines = [];
        return;
      }

      // 加载第一个块
      await loadChunk(0);

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

    const nextLine = end + 1;
    const chunkIndex = getChunkIndex(nextLine);

    // 调用 loadChunk，它会处理并发限制和队列
    loadChunk(chunkIndex);
  }

  function highlightKeywords(text: string): string {
    if (!keywords || keywords.length === 0 || !text) {
      return escapeHtml(text);
    }

    // 检查关键词是否变化，清空缓存
    const currentKeywordsHash = keywords.map((k) => `${k.type}:${k.text}`).join('|');
    if (currentKeywordsHash !== cachedKeywordsHash) {
      highlightCache.clear();
      cachedKeywordsHash = currentKeywordsHash;
    }

    // 检查缓存
    const cacheKey = text;
    if (highlightCache.has(cacheKey)) {
      return highlightCache.get(cacheKey)!;
    }

    const result = highlight(text, keywords);

    // 缓存结果
    highlightCache.set(cacheKey, result);
    return result;
  }

  function lineHasMatch(text: string): boolean {
    if (!keywords || keywords.length === 0 || !text) return false;
    return keywords.some((kwInfo) => {
      const kw = kwInfo.text;
      if (!kw || kw.length === 0) return false;
      if (kwInfo.type === 'literal') return text.toLowerCase().includes(kw.toLowerCase());
      if (kwInfo.type === 'phrase') return text.includes(kw);
      if (kwInfo.type === 'regex') {
        try {
          return new RegExp(kw).test(text);
        } catch {
          return false;
        }
      }
      return false;
    });
  }

  async function downloadCurrentFile() {
    try {
      if (!lines || lines.length === 0) return;

      // 使用后端下载端点获取完整文件
      const response = await fetchViewDownload(sid, currentFile);
      const blob = await response.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;

      // 使用 parseFileUrl 获取正确的文件名（支持 archive entry path）
      let fileName = 'log.txt';
      const parsed = parseFileUrl(currentFile);
      if (parsed) {
        fileName = parsed.displayName;
      } else {
        // 回退到 getDisplayName
        fileName = getDisplayName(currentFile);
      }
      // 清理文件名中的非法字符
      fileName = fileName.replace(/[\\/:*?"<>|]+/g, '_') || 'log.txt';

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
    // 清理滚动防抖计时器
    if (scrollDebounceTimer) {
      clearTimeout(scrollDebounceTimer);
      scrollDebounceTimer = null;
    }
    // 清理测量队列
    measureQueue = [];
    measureScheduled = false;
  });
</script>

<div class="flex h-screen flex-col bg-background text-foreground">
  <!-- 顶部导航栏 -->
  <header
    class="sticky top-0 z-50 w-full border-b border-border bg-background/95 text-lg backdrop-blur supports-backdrop-filter:bg-background/60"
  >
    <div class="flex h-16 w-full items-center gap-4 px-6">
      <!-- 左侧：Logo -->
      <a href={resolve('/')} class="flex items-center gap-2 transition-opacity hover:opacity-80">
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
            {fontSize}
            onFontSizeChange={handleFontSizeChange}
          />

          <!-- 虚拟滚动内容区域 -->
          <div
            class="relative min-h-0 flex-1 overflow-auto bg-background dark:bg-[#0d1117]"
            bind:this={parentEl}
            onscroll={handleScroll}
          >
            <div style="height: {$rowVirtualizer.getTotalSize()}px; width: 100%; position: relative;">
              {#if vItems.length === 0}
                {#if lines.length > 0}
                  {#each lines as ln (ln.no)}
                    {@const isMatch = lineHasMatch(ln.text)}
                    <div class="group/line flex font-mono leading-5 hover:bg-muted/10 {fontSizeClass}">
                      <div
                        class="w-[50px] shrink-0 px-3 py-0.5 text-right select-none {fontSizeClass} font-medium {isMatch
                          ? 'font-semibold text-foreground'
                          : 'text-muted-foreground/60'}"
                      >
                        {ln.no}
                      </div>
                      <div class="code-content flex-1 px-4 break-all text-foreground">
                        {@html highlightKeywords(ln.text)}
                      </div>
                    </div>
                  {/each}
                {:else}
                  <div class="flex h-full items-center justify-center p-10">
                    {#if loading}
                      <div class="flex flex-col items-center gap-2 text-muted-foreground">
                        <LoaderCircle class="h-8 w-8 animate-spin" />
                        <span class="text-sm">加载中...</span>
                      </div>
                    {:else}
                      <div class="text-sm text-muted-foreground">暂无内容</div>
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
                      <div class="group/line flex font-mono hover:bg-muted/10 {fontSizeClass}">
                        <div
                          class="w-[100px] shrink-0 px-3 text-right font-medium select-none {isMatch
                            ? 'font-semibold text-foreground'
                            : 'text-muted-foreground/60'}"
                        >
                          {ln.no}
                        </div>
                        <div class="code-content flex-1 px-4 break-all text-foreground">
                          {@html highlightKeywords(ln.text)}
                        </div>
                      </div>
                    {:else}
                      <div class="flex font-mono text-xs leading-relaxed opacity-60">
                        <div class="w-[50px] shrink-0 px-3 text-right font-medium text-muted-foreground select-none">
                          {item.index + 1}
                        </div>
                        <div class="code-content flex-1 px-4 text-muted-foreground">加载中…</div>
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
            <FileText class="mx-auto h-12 w-12 text-muted-foreground/50" />
            <p class="mt-4 text-sm text-muted-foreground">没有文件可查看</p>
          </div>
        </div>
      {/if}
    </main>
  </div>
</div>

<style>
  .code-content {
    font-family: 'Maple Mono NF CN', var(--font-ui), monospace;
    font-feature-settings:
      'liga' 0,
      'calt' 0;
    font-variant-ligatures: none;
    font-weight: 200;
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
