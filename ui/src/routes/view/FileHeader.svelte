<script lang="ts">
  /**
   * 文件查看页面 - 文件信息头部组件
   */
  import LogSeekLogo from '$lib/components/LogSeekLogo.svelte';

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

  // 提取文件名
  function extractFileName(fullPath: string): string {
    if (!fullPath) return '未知文件';
    const colonIndex = fullPath.indexOf(':');
    if (colonIndex >= 0) {
      const innerPath = fullPath.slice(colonIndex + 1);
      return innerPath.split('/').pop() || innerPath || '未知文件';
    }
    return fullPath.split('/').pop() || fullPath || '未知文件';
  }

  // 提取tar包名称
  function extractTarName(fullPath: string): string | null {
    if (!fullPath) return null;
    const colonIndex = fullPath.indexOf(':');
    if (colonIndex >= 0) {
      const tarPath = fullPath.slice(0, colonIndex);
      return tarPath.split('/').pop() || tarPath || null;
    }
    return null;
  }
</script>

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
        {extractFileName(filePath)}
      </h2>
      {#if extractTarName(filePath)}
        <p class="mt-0.5 font-mono text-[11px] break-all text-slate-500 dark:text-gray-400">
          来自: {extractTarName(filePath)}
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

    <!-- 中间：LogSeek标志 -->
    <div class="flex items-center justify-center">
      <LogSeekLogo size="small" hoverable />
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
