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
      // 解析 baseUrl 以获取更友好的显示
      const baseUrlParsed = parseFileUrl(parsed.baseUrl);
      let source = `source ${parsed.baseUrl}`;
      if (baseUrlParsed?.type === 'local') {
        source = `source local:${baseUrlParsed.path}`;
      } else if (baseUrlParsed?.type === 'agent') {
        source = `source agent://${baseUrlParsed.agentId}${baseUrlParsed.path}`;
      }
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
    if (parsed.type === 's3') {
      const title = parsed.displayName || full;
      const profilePart = parsed.profile ? `${parsed.profile}:` : '';
      const source = `source s3://${profilePart}${parsed.bucket}`;
      return { title, source };
    }
    return { title: full };
  }
</script>

<div class="group min-w-0 overflow-hidden" data-result-card={index}>
  <!-- 结果头：文件路径（可折叠）-->
  <div class="flex h-10 items-center rounded-t-md border border-gray-300 bg-gray-50 pr-4 pl-2 text-sm text-gray-900">
    <button
      class="flex h-6 w-6 items-center justify-center rounded-md text-gray-600"
      aria-label="Collapse"
      onclick={onToggleCollapse}
    >
      <svg
        aria-hidden="true"
        focusable="false"
        viewBox="0 0 16 16"
        width="16"
        height="16"
        fill="currentColor"
        display="inline-block"
        overflow="visible"
        style="vertical-align:text-bottom"
      >
        <path
          d="M12.78 5.22a.749.749 0 0 1 0 1.06l-4.25 4.25a.749.749 0 0 1-1.06 0L3.22 6.28a.749.749 0 1 1 1.06-1.06L8 8.939l3.72-3.719a.749.749 0 0 1 1.06 0Z"
        ></path>
      </svg>
    </button>
    <div class="ml-1 flex min-w-0 flex-1 items-center">
      <div class="flex items-center truncate text-sm font-medium">
        <div class="block truncate">
          <a
            href={`/view?sid=${encodeURIComponent(sid)}&file=${encodeURIComponent(item.path)}`}
            target="_blank"
            rel="noopener"
            class="font-mono font-bold hover:underline"
          >
            {parseTitleAndSource(item.path).title}
          </a>
        </div>
      </div>
      <div class="flex items-center truncate text-sm font-medium">
        {parseTitleAndSource(item.path).source}
      </div>
    </div>
  </div>

  {#if !isCollapsed}
    <!-- 代码块区域 -->
    <div
      class="overflow-hidden bg-linear-to-r from-slate-50 to-white transition-all duration-500 ease-in-out dark:from-gray-900/50 dark:to-gray-800/50"
    >
      {#each visibleLines(item) as ln (index + '-' + ln._ci + '-' + ln._li)}
        <div
          class="group/line grid grid-cols-[80px_1fr] gap-0 font-mono text-sm leading-[24px] transition-colors duration-150 hover:bg-blue-50/40 dark:hover:bg-blue-900/10"
        >
          <div
            class="border-r border-slate-300 bg-linear-to-r from-slate-100 to-slate-50 px-4 py-2 text-right font-medium text-slate-600 transition-all duration-150 select-none group-hover/line:from-blue-100 group-hover/line:to-blue-50 group-hover/line:text-blue-700 dark:border-gray-700/60 dark:from-gray-800/80 dark:to-gray-900/80 dark:text-gray-400 dark:group-hover/line:from-blue-900/20 dark:group-hover/line:to-blue-800/20 dark:group-hover/line:text-blue-400"
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
        class="border-t border-slate-200 bg-linear-to-r from-slate-100 to-gray-100 px-6 py-4 dark:border-gray-700/50 dark:from-gray-800/80 dark:to-gray-700/80"
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
