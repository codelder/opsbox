<script lang="ts">
  /**
   * 搜索结果卡片组件
   * 显示单个搜索结果的文件信息和匹配行
   */
  import type { SearchJsonResult, JsonLine, JsonChunk } from '$lib/modules/logseek';
  import { highlight, snippet } from '$lib/modules/logseek';
  import { parseFileUrl } from '$lib/modules/logseek/utils/fileUrl';
  
  import { Card } from "$lib/components/ui/card";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import { ChevronRight, ChevronDown, ExternalLink, MoreHorizontal, FileText, Copy, UnfoldVertical } from "lucide-svelte";
  import { Separator } from "$lib/components/ui/separator";

  interface Props {
    /**
     * 搜索结果项
     */
    item: SearchJsonResult;
    /**
     * 结果索引
     */
    index: number;
    /**
     * 会话 ID
     */
    sid: string;
    /**
     * 是否折叠
     */
    isCollapsed: boolean;
    /**
     * 是否显示全部匹配
     */
    isShowAll: boolean;
    /**
     * 已展开的行键集合
     */
    expandedLines: Set<string>;
    /**
     * 切换折叠状态
     */
    onToggleCollapse: () => void;
    /**
     * 切换显示全部匹配
     */
    onToggleShowAll: () => void;
    /**
     * 展开单行
     */
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

  let viewUrl = $derived(
    `/view?${new URLSearchParams({ sid, file: item.path }).toString()}`
  );

  // 行键生成函数
  const lineKey = (fileIdx: number, chunkIdx: number, lineIdx: number) => `${fileIdx}-${chunkIdx}-${lineIdx}`;

  // 扁平化为行数组
  function flattenLines(item: SearchJsonResult): Array<{ no: number; text: string; _ci: number; _li: number; isMatch: boolean }> {
    const arr: Array<{ no: number; text: string; _ci: number; _li: number; isMatch: boolean }> = [];
    (item?.chunks || []).forEach((chunk: JsonChunk, ci: number) => {
      (chunk?.lines || []).forEach((ln: JsonLine, li: number) => {
        // Check if this line contains any of the keywords
        const hasMatch = item.keywords.some(kw => ln.text.includes(kw));
        arr.push({ no: ln.no, text: ln.text, _ci: ci, _li: li, isMatch: hasMatch })
      });
    });
    return arr;
  }

  // 计算总行数（包括所有上下文行）
  function totalLines(item: SearchJsonResult): number {
    return flattenLines(item).length;
  }

  type LineItem = { no: number; text: string; _ci: number; _li: number; isMatch: boolean };
  
  function visibleLines(item: SearchJsonResult): LineItem[] {
    const flat = flattenLines(item);
    if (isShowAll) return flat;
    return flat.slice(0, Math.min(7, flat.length));
  }

  // 解析标题与来源标签
  function parseTitleAndSource(full: string): { title: string; source?: string } {
    const parsed = parseFileUrl(full);
    if (!parsed) return { title: full };
    return { title: full };
  }

  const { title } = $derived(parseTitleAndSource(item.path));

  function copyPath() {
    navigator.clipboard.writeText(item.path);
  }
</script>

<Card class="group overflow-hidden rounded-md border-border transition-all hover:border-primary/50" data-result-card={index}>
  <!-- 结果头：仿 GitHub 风格 -->
  <div class="flex items-center justify-between bg-muted/10 px-4 py-2 text-sm">
    <div class="flex items-center gap-2 overflow-hidden">
      <!-- 文件图标 -->
      <FileText class="h-4 w-4 text-muted-foreground" />
      
      <!-- 文件路径 -->
      <a
        href={viewUrl}
        target="_blank"
        rel="noopener"
        class="truncate font-mono text-sm font-medium text-foreground hover:text-primary hover:underline"
        title={item.path}
      >
        {item.path}
      </a>

      <!-- 匹配关键词 Badge -->
      {#if item.keywords?.length}
        <div class="hidden items-center gap-1.5 sm:flex">
          {#each item.keywords.slice(0, 3) as keyword (keyword)}
            <Badge variant="secondary" class="h-5 rounded-full px-2 text-[10px] font-normal text-muted-foreground">
              {keyword}
            </Badge>
          {/each}
        </div>
      {/if}
    </div>

    <!-- 操作按钮组 -->
    <div class="flex items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100">
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
      <Button
        variant="ghost"
        size="icon"
        class="h-7 w-7 text-muted-foreground hover:text-foreground"
        onclick={onToggleCollapse}
        title={isCollapsed ? "展开" : "折叠"}
      >
        {#if isCollapsed}
          <ChevronDown class="h-4 w-4" />
        {:else}
          <ChevronRight class="h-4 w-4 rotate-90" />
        {/if}
      </Button>
    </div>
  </div>

  <Separator />

  {#if !isCollapsed}
    <!-- 代码块区域：仿 GitHub Blob 视图 -->
    <div class="overflow-x-auto bg-background py-1 text-sm">
      <table class="w-full border-collapse">
        <tbody>
          {#each visibleLines(item) as ln (index + '-' + ln._ci + '-' + ln._li)}
            <tr class="group/line hover:bg-muted/30">
              <!-- 行号 -->
              <td
                class={`w-[1%] min-w-[3rem] select-none px-3 py-0.5 text-right font-mono text-xs align-top ${ln.isMatch ? 'text-foreground' : 'text-muted-foreground/50'}`}
              >
                {ln.no}
              </td>
              <!-- 代码内容 -->
              <td class="px-4 py-0.5 font-mono text-xs leading-relaxed break-all whitespace-pre-wrap">
                {#if expandedLines.has(lineKey(index, ln._ci, ln._li))}
                  <span class="code-content-text">{@html highlight(ln.text, item.keywords)}</span>
                {:else}
                  {#key index + '-' + ln._ci + '-' + ln._li + '-snippet'}
                    {@const sn = snippet(ln.text, item.keywords)}
                    {#if sn.leftTrunc}
                      <button
                        class="mx-1 inline-flex h-4 w-4 items-center justify-center rounded bg-muted/50 text-muted-foreground hover:bg-muted hover:text-foreground"
                        onclick={() => onExpandLine(lineKey(index, ln._ci, ln._li))}
                        title="展开"
                      >
                        <MoreHorizontal class="h-3 w-3" />
                      </button>
                    {/if}
                    <span class="code-content-text">{@html sn.html}</span>
                    {#if sn.rightTrunc}
                      <button
                        class="mx-1 inline-flex h-4 w-4 items-center justify-center rounded bg-muted/50 text-muted-foreground hover:bg-muted hover:text-foreground"
                        onclick={() => onExpandLine(lineKey(index, ln._ci, ln._li))}
                        title="展开"
                      >
                        <MoreHorizontal class="h-3 w-3" />
                      </button>
                    {/if}
                  {/key}
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <!-- 展开更多按钮 -->
    {#if totalLines(item) > 7}
      <div
        class="flex cursor-pointer items-center justify-start border-t border-border/50 bg-muted/5 px-4 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted/10 hover:text-foreground"
        onclick={onToggleShowAll}
        role="button"
        tabindex="0"
        onkeydown={(e) => e.key === 'Enter' && onToggleShowAll()}
      >
        {#if isShowAll}
          <ChevronDown class="mr-2 h-3.5 w-3.5 rotate-180" />
          收起
        {:else}
          <UnfoldVertical class="mr-2 h-3.5 w-3.5" />
          显示更多 ({totalLines(item) - 7} 行)
        {/if}
      </div>
    {/if}
  {/if}
</Card>

<style>
  .code-content-text {
    font-family: var(--font-ui), monospace;
    font-feature-settings: 'liga' 0, 'calt' 0;
    font-variant-ligatures: none;
  }
  
  /* 关键词高亮样式（通常由 highlight 函数生成 span.highlight） */
  :global(.highlight) {
    background-color: rgba(253, 224, 71, 0.3); /* yellow-300 with opacity */
    color: inherit;
    border-radius: 0.125rem;
    padding: 0 0.125rem;
  }
  :global(.dark .highlight) {
    background-color: rgba(234, 179, 8, 0.3); /* yellow-500 with opacity */
  }
</style>
