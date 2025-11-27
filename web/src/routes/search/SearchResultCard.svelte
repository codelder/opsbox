<script lang="ts">
  /**
   * 搜索结果卡片组件
   * 显示单个搜索结果的文件信息和匹配行
   */
  import type { SearchJsonResult, JsonLine, JsonChunk } from '$lib/modules/logseek';
  import { highlight, snippet } from '$lib/modules/logseek';
  import { parseFileUrl } from '$lib/modules/logseek/utils/fileUrl';

  import { Card } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Badge } from '$lib/components/ui/badge';
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
    Folder,
    Hash,
    FileCode,
    Database,
    Tag
  } from 'lucide-svelte';
  import { Separator } from '$lib/components/ui/separator';

  interface Props {
    item: SearchJsonResult;
    index: number;
    sid: string;
    isCollapsed: boolean;
    isShowAll: boolean;
    expandedLines: Set<string>;
    onToggleCollapse: () => void;
    onToggleShowAll: () => void;
    onExpandLine: (key: string) => void;
  }

  let {
    item,
    index,
    sid,
    isCollapsed,
    isShowAll,
    expandedLines,
    onToggleCollapse,
    onToggleShowAll,
    onExpandLine
  }: Props = $props();

  let viewUrl = $derived(`/view?${new URLSearchParams({ sid, file: item.path }).toString()}`);

  // 悬浮提示框状态
  let showTooltip = $state(false);
  let tooltipTimer: ReturnType<typeof setTimeout> | null = null;

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

  // 行键生成函数
  const lineKey = (fileIdx: number, chunkIdx: number, lineIdx: number) => `${fileIdx}-${chunkIdx}-${lineIdx}`;

  // 扁平化为行数组
  function flattenLines(
    item: SearchJsonResult
  ): Array<{ no: number; text: string; _ci: number; _li: number; isMatch: boolean }> {
    const arr: Array<{ no: number; text: string; _ci: number; _li: number; isMatch: boolean }> = [];
    (item?.chunks || []).forEach((chunk: JsonChunk, ci: number) => {
      (chunk?.lines || []).forEach((ln: JsonLine, li: number) => {
        const hasMatch = item.keywords.some((kw) => ln.text.includes(kw));
        arr.push({ no: ln.no, text: ln.text, _ci: ci, _li: li, isMatch: hasMatch });
      });
    });
    return arr;
  }

  // 计算总行数
  function totalLines(item: SearchJsonResult): number {
    return flattenLines(item).length;
  }

  type LineItem = { no: number; text: string; _ci: number; _li: number; isMatch: boolean };

  function visibleLines(item: SearchJsonResult): LineItem[] {
    const flat = flattenLines(item);
    if (isShowAll) return flat;
    return flat.slice(0, Math.min(7, flat.length));
  }

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
        onclick={onToggleCollapse}
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

          <!-- 悬浮提示框 - 精美设计 -->
          {#if showTooltip && parsedUrl}
            <div
              class="animate-in fade-in-0 zoom-in-95 pointer-events-auto absolute top-full left-0 z-50 mt-2 w-[420px] overflow-hidden rounded-xl border border-border bg-popover shadow-2xl duration-200"
            >
              <!-- 顶部彩色条纹 + 类型标识 -->
              <div class="relative">
                {#if parsedUrl.endpointType === 's3'}
                  <div class="h-1.5 bg-linear-to-r from-blue-500 via-cyan-500 to-blue-600"></div>
                {:else if parsedUrl.targetType === 'archive'}
                  <div class="h-1.5 bg-linear-to-r from-orange-500 via-amber-500 to-orange-600"></div>
                {:else if parsedUrl.endpointType === 'agent'}
                  <div class="h-1.5 bg-linear-to-r from-purple-500 via-violet-500 to-purple-600"></div>
                {:else if parsedUrl.targetType === 'dir'}
                  <div class="h-1.5 bg-linear-to-r from-emerald-500 via-green-500 to-emerald-600"></div>
                {:else}
                  <div class="h-1.5 bg-linear-to-r from-green-500 via-emerald-500 to-green-600"></div>
                {/if}
              </div>

              <div class="p-4">
                <!-- 头部：类型图标 + 文件名 + 编码 -->
                <div class="mb-4 flex items-start gap-3">
                  <div
                    class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg {parsedUrl.endpointType ===
                    's3'
                      ? 'bg-blue-500/10 text-blue-600 dark:text-blue-400'
                      : parsedUrl.targetType === 'archive'
                        ? 'bg-orange-500/10 text-orange-600 dark:text-orange-400'
                        : parsedUrl.endpointType === 'agent'
                          ? 'bg-purple-500/10 text-purple-600 dark:text-purple-400'
                          : parsedUrl.targetType === 'dir'
                            ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
                            : 'bg-green-500/10 text-green-600 dark:text-green-400'}"
                  >
                    {#if parsedUrl.endpointType === 's3'}
                      <Cloud class="h-5 w-5" />
                    {:else if parsedUrl.targetType === 'archive'}
                      <Archive class="h-5 w-5" />
                    {:else if parsedUrl.endpointType === 'agent'}
                      <Server class="h-5 w-5" />
                    {:else if parsedUrl.targetType === 'dir'}
                      <Folder class="h-5 w-5" />
                    {:else}
                      <HardDrive class="h-5 w-5" />
                    {/if}
                  </div>
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-2">
                      <span class="truncate font-semibold text-foreground">{parsedUrl.displayName}</span>
                    </div>
                    <div class="mt-1 flex items-center gap-2">
                      <span
                        class="inline-flex items-center rounded-md px-2 py-0.5 text-xs font-medium {parsedUrl.endpointType ===
                        's3'
                          ? 'bg-blue-500/10 text-blue-600 dark:text-blue-400'
                          : parsedUrl.targetType === 'archive'
                            ? 'bg-orange-500/10 text-orange-600 dark:text-orange-400'
                            : parsedUrl.endpointType === 'agent'
                              ? 'bg-purple-500/10 text-purple-600 dark:text-purple-400'
                              : parsedUrl.targetType === 'dir'
                                ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
                                : 'bg-green-500/10 text-green-600 dark:text-green-400'}"
                      >
                        {parsedUrl.endpointType === 's3'
                          ? 'S3 Object'
                          : parsedUrl.targetType === 'archive'
                            ? 'Archive Entry'
                            : parsedUrl.endpointType === 'agent'
                              ? 'Agent File'
                              : parsedUrl.targetType === 'dir'
                                ? 'Directory Entry'
                                : 'Local File'}
                      </span>
                      {#if item.encoding}
                        <span
                          class="inline-flex items-center gap-1 rounded-md bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground"
                        >
                          <FileCode class="h-3 w-3" />
                          {item.encoding}
                        </span>
                      {/if}
                    </div>
                  </div>
                </div>

                <!-- 元数据网格 -->
                <div class="space-y-3">
                  {#if parsedUrl.endpointType === 's3'}
                    {@const parts = parsedUrl.endpointId.split(':')}
                    {@const profile = parts.length > 1 ? parts[0] : ''}
                    {@const bucket = parts.length > 1 ? parts[1] : parts[0]}
                    <div class="grid grid-cols-2 gap-3">
                      {#if profile}
                        <div class="rounded-lg bg-muted/40 p-2.5">
                          <div class="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
                            <Database class="h-3 w-3" />
                            <span>Profile</span>
                          </div>
                          <div class="font-mono text-sm text-foreground">{profile}</div>
                        </div>
                      {/if}
                      <div class="rounded-lg bg-muted/40 p-2.5">
                        <div class="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
                          <Cloud class="h-3 w-3" />
                          <span>Bucket</span>
                        </div>
                        <div class="font-mono text-sm text-foreground">{bucket}</div>
                      </div>
                    </div>
                    <div class="rounded-lg bg-muted/40 p-2.5">
                      <div class="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
                        <Folder class="h-3 w-3" />
                        <span>Object Key</span>
                      </div>
                      <div class="font-mono text-xs leading-relaxed break-all text-foreground">{parsedUrl.path}</div>
                    </div>
                  {:else if parsedUrl.targetType === 'archive'}
                    <div class="rounded-lg border border-orange-500/20 bg-orange-500/5 p-2.5">
                      <div class="mb-1 flex items-center gap-1.5 text-xs text-orange-600 dark:text-orange-400">
                        <Archive class="h-3 w-3" />
                        <span>Archive</span>
                      </div>
                      <div class="font-mono text-xs leading-relaxed break-all text-foreground">{parsedUrl.path}</div>
                    </div>
                    {#if parsedUrl.entryPath}
                      <div class="rounded-lg bg-muted/40 p-2.5">
                        <div class="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
                          <FileText class="h-3 w-3" />
                          <span>Entry Path</span>
                        </div>
                        <div class="font-mono text-xs leading-relaxed break-all text-foreground">
                          {parsedUrl.entryPath}
                        </div>
                      </div>
                    {/if}
                  {:else if parsedUrl.endpointType === 'agent'}
                    <div class="rounded-lg border border-purple-500/20 bg-purple-500/5 p-2.5">
                      <div class="mb-1 flex items-center gap-1.5 text-xs text-purple-600 dark:text-purple-400">
                        <Server class="h-3 w-3" />
                        <span>Agent ID</span>
                      </div>
                      <div class="font-mono text-sm font-medium text-foreground">{parsedUrl.endpointId}</div>
                    </div>
                    <div class="rounded-lg bg-muted/40 p-2.5">
                      <div class="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
                        <Folder class="h-3 w-3" />
                        <span>File Path</span>
                      </div>
                      <div class="font-mono text-xs leading-relaxed break-all text-foreground">{parsedUrl.path}</div>
                    </div>
                  {:else if parsedUrl.targetType === 'dir'}
                    <div class="rounded-lg border border-emerald-500/20 bg-emerald-500/5 p-2.5">
                      <div class="mb-1 flex items-center gap-1.5 text-xs text-emerald-600 dark:text-emerald-400">
                        <Folder class="h-3 w-3" />
                        <span>Base Directory</span>
                      </div>
                      <div class="font-mono text-xs leading-relaxed break-all text-foreground">
                        {parsedUrl.path.split('/').slice(0, -1).join('/') || '/'}
                      </div>
                    </div>
                    <div class="rounded-lg bg-muted/40 p-2.5">
                      <div class="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
                        <FileText class="h-3 w-3" />
                        <span>Relative Path</span>
                      </div>
                      <div class="font-mono text-xs leading-relaxed break-all text-foreground">
                        {parsedUrl.displayName}
                      </div>
                    </div>
                  {:else if parsedUrl.endpointType === 'local'}
                    <div class="rounded-lg bg-muted/40 p-2.5">
                      <div class="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
                        <HardDrive class="h-3 w-3" />
                        <span>Local Path</span>
                      </div>
                      <div class="font-mono text-xs leading-relaxed break-all text-foreground">{parsedUrl.path}</div>
                    </div>
                  {/if}
                </div>

                <!-- 关键词 -->
                {#if item.keywords && item.keywords.length > 0}
                  <div class="mt-4 border-t border-border pt-3">
                    <div class="mb-2 flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
                      <Tag class="h-3 w-3" />
                      <span>Matched Keywords</span>
                      <span
                        class="ml-auto rounded-full bg-primary/10 px-1.5 py-0.5 text-[10px] font-semibold text-primary"
                        >{item.keywords.length}</span
                      >
                    </div>
                    <div class="flex flex-wrap gap-1.5">
                      {#each item.keywords as keyword, i}
                        <span
                          class="inline-flex items-center gap-1 rounded-md border border-amber-500/20 bg-linear-to-r from-amber-500/10 to-yellow-500/10 px-2 py-1 font-mono text-xs text-amber-700 dark:text-amber-400"
                        >
                          <Hash class="h-3 w-3 opacity-60" />
                          {keyword}
                        </span>
                      {/each}
                    </div>
                  </div>
                {/if}
              </div>
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
          {#each visibleLines(item) as ln, idx (index + '-' + ln._ci + '-' + ln._li)}
            <!-- 在不同 chunk 之间插入分隔行 -->
            {#if idx > 0 && visibleLines(item)[idx - 1]._ci !== ln._ci}
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
                        onclick={() => onExpandLine(lineKey(index, ln._ci, ln._li))}
                        title="展开">...</button
                      >{/if}<span class="code-content-text">{@html sn.html}</span>{#if sn.rightTrunc}<button
                        class="mx-0.5 text-muted-foreground hover:text-foreground hover:underline"
                        onclick={() => onExpandLine(lineKey(index, ln._ci, ln._li))}
                        title="展开">...</button
                      >{/if}{/key}{/if}</td
              >
            </tr>
          {/each}

          <!-- 展开更多行 -->
          {#if totalLines(item) > 7 && !isShowAll}
            <tr class="border-t border-border bg-muted/5 hover:bg-muted/10">
              <td colspan="2" class="p-0">
                <button
                  class="flex w-full items-center gap-2 px-4 py-2 text-xs text-muted-foreground transition-colors hover:text-foreground"
                  onclick={onToggleShowAll}
                >
                  <UnfoldVertical class="h-3.5 w-3.5" />
                  <span>显示其余 {totalLines(item) - 7} 行匹配项</span>
                </button>
              </td>
            </tr>
          {/if}
          {#if isShowAll && totalLines(item) > 7}
            <tr class="border-t border-border bg-muted/5 hover:bg-muted/10">
              <td colspan="2" class="p-0">
                <button
                  class="flex w-full items-center gap-2 px-4 py-2 text-xs text-muted-foreground transition-colors hover:text-foreground"
                  onclick={onToggleShowAll}
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
