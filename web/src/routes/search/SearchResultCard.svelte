<script lang="ts">
  /**
   * 搜索结果卡片组件
   * 显示单个搜索结果的文件信息和匹配行
   */
  import type { SearchJsonResult, JsonLine, JsonChunk } from '$lib/modules/logseek';
  import { highlight, snippet } from '$lib/modules/logseek';
  import { parseFileUrl } from '$lib/modules/logseek/utils/fileUrl';

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

  // 行键生成函数
  const lineKey = (fileIdx: number, chunkIdx: number, lineIdx: number) => `${fileIdx}-${chunkIdx}-${lineIdx}`;

  // 扁平化为行数组
  function flattenLines(item: SearchJsonResult): Array<{ no: number; text: string; _ci: number; _li: number }> {
    const arr: Array<{ no: number; text: string; _ci: number; _li: number }> = [];
    (item?.chunks || []).forEach((chunk: JsonChunk, ci: number) => {
      (chunk?.lines || []).forEach((ln: JsonLine, li: number) =>
        arr.push({ no: ln.no, text: ln.text, _ci: ci, _li: li })
      );
    });
    return arr;
  }

  // 统计真正包含关键词的匹配行数（不包含上下文行）
  function totalMatches(item: SearchJsonResult): number {
    if (!item.keywords || item.keywords.length === 0) {
      return flattenLines(item).length;
    }

    const lines = flattenLines(item);
    let matchCount = 0;

    for (const ln of lines) {
      // 检查该行是否包含任何关键词
      const hasKeyword = item.keywords.some((keyword) => {
        if (!keyword || keyword.trim() === '') return false;
        return ln.text.includes(keyword);
      });

      if (hasKeyword) {
        matchCount++;
      }
    }

    return matchCount;
  }

  // 计算总行数（包括所有上下文行）
  function totalLines(item: SearchJsonResult): number {
    return flattenLines(item).length;
  }

  function visibleLines(item: SearchJsonResult): Array<{ no: number; text: string; _ci: number; _li: number }> {
    const flat = flattenLines(item);
    if (isShowAll) return flat;
    return flat.slice(0, Math.min(7, flat.length));
  }

  // 解析标题与来源标签
  function parseTitleAndSource(full: string): { title: string; source?: string } {
    const parsed = parseFileUrl(full);
    if (!parsed) return { title: full };
    if (parsed.type === 'tar-entry') {
      const title = parsed.entryPath || parsed.displayName || full;
      const source = `source archive+${parsed.baseUrl}`;
      return { title, source };
    }
    if (parsed.type === 'dir-entry') {
      const title = parsed.entryPath || parsed.displayName || full;
      const source = `source ${parsed.baseUrl}`; // 不显示 dir 字样
      return { title, source };
    }
    if (parsed.type === 'agent') {
      const title = parsed.path || parsed.displayName || full;
      // 构造目录路径（不包含文件名）
      let dir = '/';
      if (parsed.path) {
        const idx = parsed.path.lastIndexOf('/');
        if (idx > 0) {
          dir = parsed.path.substring(0, idx);
        }
      }
      const source = `source agent://${parsed.agentId}${dir}`;
      return { title, source };
    }
    return { title: full };
  }
</script>

<div
  class="group overflow-hidden rounded-2xl border border-white/60 bg-white/95 shadow-xl shadow-slate-300/40 backdrop-blur-sm transition-all duration-300 hover:shadow-2xl hover:shadow-slate-400/50 dark:border-gray-700/50 dark:bg-gray-800/80 dark:shadow-gray-900/20 dark:hover:shadow-gray-900/30"
  data-result-card={index}
>
  <!-- 结果头：文件路径（可折叠）-->
  <button
    type="button"
    class="w-full border-b border-slate-200 bg-gradient-to-r from-slate-50 to-gray-100 px-6 py-6 text-left text-sm text-slate-700 transition-all duration-200 hover:from-slate-100 hover:to-slate-200 focus:ring-2 focus:ring-blue-500/20 focus:outline-none focus:ring-inset dark:border-gray-700/50 dark:from-gray-800/50 dark:to-gray-700/50 dark:text-gray-300 dark:hover:from-gray-700/50 dark:hover:to-gray-600/50"
    onclick={onToggleCollapse}
  >
    <span class="flex w-full flex-col gap-3 md:flex-row md:items-start md:justify-between">
      <!-- 左侧：标题 + 元信息 -->
      <span class="min-w-0 flex-1">
        <!-- 主标题 -->
        {#if parseTitleAndSource(item.path).title}
          <span class="mb-2 flex items-start gap-2">
            <span
              role="link"
              tabindex="0"
              class="group/link cursor-pointer font-mono text-base leading-tight font-semibold text-slate-900 transition-colors duration-200 hover:text-blue-700 md:text-lg lg:text-xl dark:text-gray-100 dark:hover:text-blue-300"
              title={parseTitleAndSource(item.path).title}
              onclick={(e) => {
                e.stopPropagation();
                const base = '/view';
                const url = `${base}?sid=${encodeURIComponent(sid)}&file=${encodeURIComponent(item.path)}`;
                window.open(url, '_blank', 'noopener');
              }}
              onkeydown={(e) => {
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
                >{parseTitleAndSource(item.path).title}</span
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
          <span class="mb-2">
            <span class="font-mono text-base leading-tight font-semibold text-slate-900 md:text-lg dark:text-gray-100">
              {item.path}
            </span>
          </span>
        {/if}

        <!-- 元信息行 -->
        <span class="flex flex-wrap items-center gap-2 text-xs text-slate-600 dark:text-gray-400">
          {#if parseTitleAndSource(item.path).source}
            <span
              class="inline-flex items-center rounded-md bg-gray-100 px-2.5 py-1 text-[11px] font-medium text-gray-700 ring-1 ring-gray-200 dark:bg-gray-700/50 dark:text-gray-300 dark:ring-gray-600"
            >
              <svg class="mr-1 h-3 w-3" fill="currentColor" viewBox="0 0 20 20">
                <path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z" />
              </svg>
              {parseTitleAndSource(item.path).source}
            </span>
          {/if}

          {#if item.keywords?.length}
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

      <!-- 右侧：折叠箭头 -->
      <span class="mt-2 flex shrink-0 items-center md:mt-0">
        <span
          class="flex h-8 w-8 items-center justify-center rounded-full bg-gray-200/60 transition-colors duration-200 group-hover:bg-gray-300/60 dark:bg-gray-600/50 dark:group-hover:bg-gray-500/50"
        >
          <svg
            class="h-4 w-4 text-gray-700 transition-transform duration-200 {isCollapsed
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

  {#if !isCollapsed}
    <!-- 代码块区域 -->
    <div
      class="overflow-hidden bg-gradient-to-r from-slate-50 to-white transition-all duration-500 ease-in-out dark:from-gray-900/50 dark:to-gray-800/50"
    >
      {#each visibleLines(item) as ln (index + '-' + ln._ci + '-' + ln._li)}
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
            {#if expandedLines.has(lineKey(index, ln._ci, ln._li))}
              <span class="code-content-text">{@html highlight(ln.text, item.keywords)}</span>
            {:else}
              {#key index + '-' + ln._ci + '-' + ln._li + '-snippet'}
                {@const sn = snippet(ln.text, item.keywords)}
                {#if sn.leftTrunc}
                  <button
                    type="button"
                    class="group/expand mx-0.5 inline-flex items-center rounded-md border border-blue-200 bg-blue-50 px-1.5 py-0.5 text-xs font-medium text-blue-700 transition-all duration-200 hover:border-blue-300 hover:bg-blue-100 hover:text-blue-800 focus:ring-2 focus:ring-blue-500/20 focus:outline-none dark:border-blue-700/50 dark:bg-blue-900/30 dark:text-blue-300 dark:hover:bg-blue-800/50 dark:hover:text-blue-200"
                    onclick={() => onExpandLine(lineKey(index, ln._ci, ln._li))}
                    title="展开显示完整内容"
                  >
                    <svg
                      class="mr-1 h-3 w-3 transition-transform duration-200 group-hover/expand:scale-105"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                    >
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
                    </svg>
                    <span>…</span>
                  </button>
                {/if}
                <span class="code-content-text">{@html sn.html}</span>
                {#if sn.rightTrunc}
                  <button
                    type="button"
                    class="group/expand mx-0.5 inline-flex items-center rounded-md border border-blue-200 bg-blue-50 px-1.5 py-0.5 text-xs font-medium text-blue-700 transition-all duration-200 hover:border-blue-300 hover:bg-blue-100 hover:text-blue-800 focus:ring-2 focus:ring-blue-500/20 focus:outline-none dark:border-blue-700/50 dark:bg-blue-900/30 dark:text-blue-300 dark:hover:bg-blue-800/50 dark:hover:text-blue-200"
                    onclick={() => onExpandLine(lineKey(index, ln._ci, ln._li))}
                    title="展开显示完整内容"
                  >
                    <span>…</span>
                    <svg
                      class="ml-1 h-3 w-3 transition-transform duration-200 group-hover/expand:scale-105"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                    >
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

    <!-- 展开更多按钮 -->
    {#if totalLines(item) > 7}
      <div
        class="border-t border-slate-200 bg-gradient-to-r from-slate-100 to-gray-100 px-6 py-4 dark:border-gray-700/50 dark:from-gray-800/80 dark:to-gray-700/80"
      >
        <button
          class="group inline-flex items-center rounded-lg px-3 py-2 text-sm font-medium text-blue-600 transition-all duration-200 hover:bg-blue-50 hover:text-blue-700 focus:ring-2 focus:ring-blue-500/20 focus:ring-offset-2 focus:ring-offset-gray-50 focus:outline-none dark:text-blue-400 dark:hover:bg-blue-900/20 dark:hover:text-blue-300 dark:focus:ring-offset-gray-800"
          onclick={onToggleShowAll}
        >
          {#if isShowAll}
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
              >{totalLines(item) - 7}</span
            > 行
          {/if}
        </button>
      </div>
    {/if}
  {/if}
</div>

<style>
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
</style>
