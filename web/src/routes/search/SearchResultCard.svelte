<script lang="ts">
  /**
   * 搜索结果卡片组件
   * 显示单个搜索结果的文件信息和匹配行
   */
  import type { SearchJsonResult, JsonChunk } from '$lib/modules/logseek';
  import { highlight, snippet } from '$lib/modules/logseek';
  import { parseFileUrl } from '$lib/modules/logseek/utils/fileUrl';

  import { Card } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import {
    ChevronRight,
    ChevronDown,
    ExternalLink,
    FileText,
    Copy,
    UnfoldVertical,
    Cloud,
    Server,
    HardDrive,
    Archive,
    Folder
  } from 'lucide-svelte';

  interface Props {
    item: SearchJsonResult;
    index: number;
    sid: string;
  }

  let { item, index, sid }: Props = $props();

  // Svelte 5 类型导出
  export type { Props };

  let viewUrl = $derived(`/view?${new URLSearchParams({ sid, file: item.path }).toString()}`);

  // 悬浮提示框状态
  let showTooltip = $state(false);
  let tooltipTimer: ReturnType<typeof setTimeout> | null = null;

  // 组件内部状态
  let isCollapsed = $state(false);
  let isShowAll = $state(false);
  let expandedLines = $state(new Set<string>());

  // 解析文件URL获取详细元数据
  let parsedUrl = $derived(parseFileUrl(item.path));

  // 延迟显示tooltip
  function handleMouseEnter() {
    tooltipTimer = setTimeout(() => {
      showTooltip = true;
    }, 300);
  }

  function handleMouseLeave() {
    if (tooltipTimer) {
      clearTimeout(tooltipTimer);
      tooltipTimer = null;
    }
    showTooltip = false;
  }

  // 组件内部交互函数
  function toggleCollapse() {
    isCollapsed = !isCollapsed;
  }

  function toggleShowAll() {
    const wasExpanded = isShowAll;
    isShowAll = !wasExpanded;
    // 收起时，延迟滚动以等待DOM更新
    if (wasExpanded) {
      setTimeout(() => {
        const cardElement = document.querySelector(`[data-result-card="${index}"]`);
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
    }
  }

  function expandLine(key: string) {
    expandedLines = new Set([...expandedLines, key]);
  }

  // 行键生成函数
  const lineKey = (fileIdx: number, chunkIdx: number, lineIdx: number) => `${fileIdx}-${chunkIdx}-${lineIdx}`;

  type LineItem = { no: number; text: string; _ci: number; _li: number; isMatch: boolean };

  // 使用 $derived 缓存扁平化结果，避免重复计算
  // 关键优化：只在需要显示的行上计算，而不是所有行
  let allFlattenedLines = $derived.by(() => {
    const arr: LineItem[] = [];
    const chunks = item?.chunks || [];
    const keywords = item?.keywords || [];

    // 如果不需要显示所有行，只处理前7行（加上一些缓冲，避免边界问题）
    const maxLinesToProcess = isShowAll ? Infinity : 10;
    let processedCount = 0;

    for (let ci = 0; ci < chunks.length && processedCount < maxLinesToProcess; ci++) {
      const chunk = chunks[ci];
      const lines = chunk?.lines || [];

      for (let li = 0; li < lines.length && processedCount < maxLinesToProcess; li++) {
        const ln = lines[li];
        const hasMatch = keywords.some((kw) => {
          const kwText = kw?.text ?? '';
          if (!kwText) return false;
          if (kw.type === 'literal') return ln.text.toLowerCase().includes(kwText.toLowerCase());
          if (kw.type === 'phrase') return ln.text.includes(kwText);
          if (kw.type === 'regex') {
            try {
              return new RegExp(kwText).test(ln.text);
            } catch {
              return false;
            }
          }
          return false;
        });
        arr.push({ no: ln.no, text: ln.text, _ci: ci, _li: li, isMatch: hasMatch });
        processedCount++;
      }
    }

    return arr;
  });

  // 计算总行数（轻量级计算，只计数不处理内容）
  // 使用 $derived 缓存，只计算一次
  let totalLinesCount = $derived.by(() => {
    let count = 0;
    (item?.chunks || []).forEach((chunk: JsonChunk) => {
      count += (chunk?.lines || []).length;
    });
    return count;
  });

  // 使用 $derived 缓存可见行，避免重复计算
  let visibleLinesList = $derived.by(() => {
    if (isShowAll) {
      // 如果显示所有行，使用 allFlattenedLines（它已经处理了所有行）
      return allFlattenedLines;
    } else {
      // 只显示前7行，使用已处理的行（最多7行）
      return allFlattenedLines.slice(0, Math.min(7, allFlattenedLines.length));
    }
  });

  function copyPath() {
    navigator.clipboard.writeText(item.path);
  }
</script>

<Card
  class="group overflow-visible rounded-md border-border bg-card transition-all hover:border-primary/50 dark:border-gray-700 dark:bg-[#0d1117]"
  data-result-card={index}
>
  <!-- 结果头：仿 GitHub 风格 -->
  <div
    class="flex items-center justify-between border-b border-border bg-muted/30 px-4 py-2 text-sm dark:border-gray-700 dark:bg-[#161b22]"
  >
    <div class="flex items-center gap-2 overflow-visible">
      <button
        class="text-muted-foreground hover:text-foreground"
        onclick={toggleCollapse}
        title={isCollapsed ? '展开' : '折叠'}
      >
        {#if isCollapsed}
          <ChevronRight class="h-4 w-4" />
        {:else}
          <ChevronDown class="h-4 w-4" />
        {/if}
      </button>

      <!-- 文件路径 - 只显示文件名 -->
      <div class="flex items-center gap-1 font-mono text-sm">
        <div
          class="relative flex items-center"
          role="presentation"
          onmouseenter={handleMouseEnter}
          onmouseleave={handleMouseLeave}
        >
          <a
            href={viewUrl}
            target="_blank"
            rel="noopener"
            class="flex items-center font-bold text-foreground hover:underline"
          >
            <FileText class="mr-1.5 inline-block h-4 w-4 align-text-bottom text-muted-foreground" />
            <span class="truncate">{parsedUrl ? parsedUrl.displayName : item.path}</span>
          </a>

          <!-- 悬浮提示框 - 简约设计 -->
          {#if showTooltip && parsedUrl}
            <div
              class="animate-in fade-in-0 pointer-events-auto absolute top-full left-0 z-50 mt-2 w-[380px] overflow-hidden rounded-lg border border-border bg-popover p-4 shadow-xl"
            >
              <!-- 头部：类型图标 + 文件名 -->
              <div class="mb-4 flex items-start gap-3">
                <div
                  class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground"
                >
                  {#if parsedUrl.endpointType === 's3'}
                    <Cloud class="h-4 w-4" />
                  {:else if parsedUrl.targetType === 'archive'}
                    <Archive class="h-4 w-4" />
                  {:else if parsedUrl.endpointType === 'agent'}
                    <Server class="h-4 w-4" />
                  {:else if parsedUrl.targetType === 'dir'}
                    <Folder class="h-4 w-4" />
                  {:else}
                    <HardDrive class="h-4 w-4" />
                  {/if}
                </div>
                <div class="min-w-0 flex-1">
                  <div class="font-medium break-all text-foreground">{parsedUrl.displayName}</div>
                  <div class="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
                    <span class="capitalize">{parsedUrl.endpointType}</span>
                    <span>•</span>
                    <span>{item.encoding || 'UTF-8'}</span>
                  </div>
                </div>
              </div>

              <!-- 元数据列表 -->
              <div class="space-y-2 text-xs">
                {#if parsedUrl.endpointType === 's3'}
                  {@const parts = parsedUrl.endpointId.split(':')}
                  {@const profile = parts.length > 1 ? parts[0] : ''}
                  {@const bucket = parts.length > 1 ? parts[1] : parts[0]}

                  {#if profile}
                    <div class="grid grid-cols-[80px_1fr] gap-2">
                      <span class="text-muted-foreground">Profile</span>
                      <span class="font-mono text-foreground">{profile}</span>
                    </div>
                  {/if}
                  <div class="grid grid-cols-[80px_1fr] gap-2">
                    <span class="text-muted-foreground">Bucket</span>
                    <span class="font-mono text-foreground">{bucket}</span>
                  </div>
                  <div class="grid grid-cols-[80px_1fr] gap-2">
                    <span class="text-muted-foreground">Object Key</span>
                    <span class="font-mono break-all text-foreground">{parsedUrl.path}</span>
                  </div>
                  {#if parsedUrl.targetType === 'archive' && parsedUrl.entryPath}
                    <div class="grid grid-cols-[80px_1fr] gap-2">
                      <span class="text-muted-foreground">Entry Path</span>
                      <span class="font-mono break-all text-foreground">{parsedUrl.entryPath}</span>
                    </div>
                  {/if}
                {:else if parsedUrl.targetType === 'archive'}
                  <div class="grid grid-cols-[80px_1fr] gap-2">
                    <span class="text-muted-foreground">Archive</span>
                    <span class="font-mono break-all text-foreground">{parsedUrl.path}</span>
                  </div>
                  {#if parsedUrl.entryPath}
                    <div class="grid grid-cols-[80px_1fr] gap-2">
                      <span class="text-muted-foreground">Entry Path</span>
                      <span class="font-mono break-all text-foreground">{parsedUrl.entryPath}</span>
                    </div>
                  {/if}
                {:else if parsedUrl.endpointType === 'agent'}
                  <div class="grid grid-cols-[80px_1fr] gap-2">
                    <span class="text-muted-foreground">Agent ID</span>
                    <span class="font-mono text-foreground">{parsedUrl.endpointId}</span>
                  </div>
                  <div class="grid grid-cols-[80px_1fr] gap-2">
                    <span class="text-muted-foreground">Path</span>
                    <span class="font-mono break-all text-foreground">{parsedUrl.path}</span>
                  </div>
                {:else if parsedUrl.targetType === 'dir'}
                  <div class="grid grid-cols-[80px_1fr] gap-2">
                    <span class="text-muted-foreground">Base Dir</span>
                    <span class="font-mono break-all text-foreground"
                      >{parsedUrl.path.split('/').slice(0, -1).join('/') || '/'}</span
                    >
                  </div>
                  <div class="grid grid-cols-[80px_1fr] gap-2">
                    <span class="text-muted-foreground">Relative</span>
                    <span class="font-mono break-all text-foreground">{parsedUrl.displayName}</span>
                  </div>
                {:else if parsedUrl.endpointType === 'local'}
                  <div class="grid grid-cols-[80px_1fr] gap-2">
                    <span class="text-muted-foreground">Path</span>
                    <span class="font-mono break-all text-foreground">{parsedUrl.path}</span>
                  </div>
                {/if}
              </div>

              <!-- 关键词 -->
              {#if (item.keywords || []).filter((k) => !k.text.includes(':')).length > 0}
                {@const displayKeywords = (item.keywords || []).filter((k) => !k.text.includes(':'))}
                <div class="mt-4 border-t border-border pt-3">
                  <div class="mb-2 text-xs font-medium text-muted-foreground">Matched Keywords</div>
                  <div class="flex flex-wrap gap-1.5">
                    {#each displayKeywords as keyword}
                      <span
                        class="inline-flex items-center rounded-md bg-muted px-2 py-1 font-mono text-xs text-foreground"
                      >
                        {keyword.text}
                      </span>
                    {/each}
                  </div>
                </div>
              {/if}
            </div>
          {/if}
        </div>
      </div>
    </div>

    <!-- 右侧元数据和操作 -->
    <div class="flex items-center gap-4">
      <!-- 语言/类型标记 -->
      <div class="hidden items-center gap-2 text-xs text-muted-foreground sm:flex">
        <span class="flex items-center gap-1">
          <span class="h-2 w-2 rounded-full bg-yellow-400"></span>
          {item.encoding || 'UTF-8'}
        </span>
      </div>

      <!-- 操作按钮组 -->
      <div class="flex items-center gap-1">
        <Button
          variant="ghost"
          size="icon"
          class="h-7 w-7 text-muted-foreground hover:text-foreground"
          onclick={copyPath}
          title="复制路径"
        >
          <Copy class="h-3.5 w-3.5" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          class="h-7 w-7 text-muted-foreground hover:text-foreground"
          href={viewUrl}
          target="_blank"
          title="在新窗口打开"
        >
          <ExternalLink class="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  </div>

  {#if !isCollapsed}
    <!-- 代码块区域：仿 GitHub Blob 视图 -->
    <div class="overflow-x-auto bg-background py-0 text-sm dark:bg-[#0d1117]">
      <table class="w-full border-collapse">
        <tbody>
          {#each visibleLinesList as ln, idx (index + '-' + ln._ci + '-' + ln._li)}
            <!-- 在不同 chunk 之间插入分隔行 -->
            {#if idx > 0 && visibleLinesList[idx - 1]._ci !== ln._ci}
              <tr class="chunk-separator">
                <td colspan="2" class="h-5 bg-muted/50 dark:bg-muted/60">
                  <div class="flex h-full items-center justify-center">
                    <div class="h-px w-full bg-border/60 dark:bg-border/80"></div>
                    <span class="mx-3 shrink-0 text-[9px] text-muted-foreground/70">⋮</span>
                    <div class="h-px w-full bg-border/60 dark:bg-border/80"></div>
                  </div>
                </td>
              </tr>
            {/if}
            <tr class="group/line hover:bg-muted/10">
              <!-- 行号 -->
              <td
                class="w-[1%] min-w-[50px] px-3 py-0.5 text-right align-top font-mono text-xs select-none {ln.isMatch
                  ? 'font-semibold text-foreground'
                  : 'text-muted-foreground/60'}"
              >
                {ln.no}
              </td>
              <!-- 代码内容 -->
              <td class="px-4 py-0.5 font-mono text-xs leading-relaxed break-all whitespace-pre-wrap text-foreground"
                >{#if expandedLines.has(lineKey(index, ln._ci, ln._li))}<span class="code-content-text"
                    >{@html highlight(ln.text, item.keywords)}</span
                  >{:else}{#key index + '-' + ln._ci + '-' + ln._li + '-snippet'}{@const sn = snippet(
                      ln.text,
                      item.keywords
                    )}{#if sn.leftTrunc}<button
                        class="mx-0.5 text-muted-foreground hover:text-foreground hover:underline"
                        onclick={() => expandLine(lineKey(index, ln._ci, ln._li))}
                        title="展开">...</button
                      >{/if}<span class="code-content-text">{@html sn.html}</span>{#if sn.rightTrunc}<button
                        class="mx-0.5 text-muted-foreground hover:text-foreground hover:underline"
                        onclick={() => expandLine(lineKey(index, ln._ci, ln._li))}
                        title="展开">...</button
                      >{/if}{/key}{/if}</td
              >
            </tr>
          {/each}

          <!-- 展开更多行 -->
          {#if totalLinesCount > 7 && !isShowAll}
            <tr class="border-t border-border bg-muted/5 hover:bg-muted/10">
              <td colspan="2" class="p-0">
                <button
                  class="flex w-full items-center gap-2 px-4 py-2 text-xs text-muted-foreground transition-colors hover:text-foreground"
                  onclick={toggleShowAll}
                >
                  <UnfoldVertical class="h-3.5 w-3.5" />
                  <span>显示其余 {totalLinesCount - 7} 行匹配项</span>
                </button>
              </td>
            </tr>
          {/if}
          {#if isShowAll && totalLinesCount > 7}
            <tr class="border-t border-border bg-muted/5 hover:bg-muted/10">
              <td colspan="2" class="p-0">
                <button
                  class="flex w-full items-center gap-2 px-4 py-2 text-xs text-muted-foreground transition-colors hover:text-foreground"
                  onclick={toggleShowAll}
                >
                  <ChevronDown class="h-3.5 w-3.5 rotate-180" />
                  <span>收起</span>
                </button>
              </td>
            </tr>
          {/if}
        </tbody>
      </table>
    </div>
  {/if}
</Card>

<style>
  .code-content-text {
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
