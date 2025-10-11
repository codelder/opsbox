<script lang="ts">
  /**
   * 文件查看页面 - 文件信息头部组件
   */
  import { parseFileUrl } from '$lib/modules/logseek/utils/fileUrl';

  interface Props {
    /**
     * 文件完整路径
     */
    filePath: string;
    /**
     * 总行数
     */
    total: number;
    /**
     * 已加载行数
     */
    loadedLines: number;
    /**
     * 关键词列表
     */
    keywords?: string[];
    /**
     * 是否正在加载
     */
    loading?: boolean;
    /**
     * 下载回调
     */
    onDownload?: () => void;
  }

  let { filePath, total, loadedLines, keywords = [], loading = false, onDownload }: Props = $props();

  // 统一解析标题与来源（与搜索结果卡片保持一致）
  function parseTitleAndSource(full: string): { title: string; source?: string } {
    if (!full) return { title: '未知文件' };
    const parsed = parseFileUrl(full);
    if (!parsed) return { title: full };

    switch (parsed.type) {
      case 'tar-entry': {
        const title = parsed.entryPath || parsed.displayName || full;
        const source = `${parsed.compression}+${parsed.baseUrl}`; // 例如 tar.gz+s3://...
        return { title, source };
      }
      case 'dir-entry': {
        const title = parsed.entryPath || parsed.displayName || full; // 相对路径
        const source = parsed.baseUrl; // 例如 file:///root
        return { title, source };
      }
      case 'agent': {
        const title = parsed.path || parsed.displayName || full; // 去掉前缀
        const source = `agent://${parsed.agentId}`;
        return { title, source };
      }
      case 'local': {
        // 保留原样（或可仅显示绝对路径部分）
        return { title: full };
      }
      case 's3': {
        return { title: full };
      }
      default:
        return { title: full };
    }
  }
</script>

<!-- 标题栏 -->
<div
  class="flex-shrink-0 border-b border-slate-200 bg-gradient-to-r from-slate-50 to-gray-100 px-6 py-4 dark:border-gray-700/50 dark:from-gray-800/50 dark:to-gray-700/50"
>
  <!-- 两列布局：左侧文件信息，右侧下载按钮 -->
  <div class="flex items-center justify-between gap-4">
    <!-- 左侧：文件信息 -->
    <div class="min-w-0">
      <h2
        class="font-mono text-sm leading-tight font-semibold break-all text-slate-900 md:text-base dark:text-gray-100"
      >
        {parseTitleAndSource(filePath).title}
      </h2>
      {#if parseTitleAndSource(filePath).source}
        <p class="mt-0.5 font-mono text-[11px] break-all text-slate-500 dark:text-gray-400">
          source {parseTitleAndSource(filePath).source}
        </p>
      {/if}
      {#if total > 0}
        <div class="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-slate-600 dark:text-gray-400">
          <span
            class="inline-flex items-center rounded-md bg-blue-50 px-2 py-0.5 text-[10px] font-medium text-blue-700 ring-1 ring-blue-200 dark:bg-blue-900/30 dark:text-blue-300 dark:ring-blue-800"
          >
            <svg class="mr-1 h-2.5 w-2.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
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
            <svg class="mr-1 h-2.5 w-2.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4" />
            </svg>
            已加载 {loadedLines} 行
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

    <!-- 右侧：下载按钮 -->
    <div class="flex justify-end">
      <button
        class="inline-flex items-center rounded-lg bg-gradient-to-r from-blue-600 to-blue-700 px-3 py-2 text-sm font-semibold text-white shadow-lg shadow-blue-500/25 transition-all duration-300 hover:from-blue-700 hover:to-blue-800 hover:shadow-xl hover:shadow-blue-500/30 focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:ring-offset-gray-50 focus:outline-none disabled:from-gray-400 disabled:to-gray-500 disabled:shadow-gray-400/25 dark:focus:ring-offset-gray-900 dark:disabled:from-gray-600 dark:disabled:to-gray-700"
        onclick={onDownload}
        disabled={loading || total <= 0}
        title="下载当前文件"
      >
        <svg class="mr-1.5 h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
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
