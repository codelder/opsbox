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
<div class="flex-shrink-0 border-b border-[var(--border)] bg-[var(--surface-2)] px-6 py-4">
  <!-- 响应式布局：在小屏幕上垂直排列，大屏幕上横向排列 -->
  <div class="flex flex-col gap-4 md:flex-row md:items-start md:justify-between">
    <!-- 左侧：文件信息 -->
    <div class="min-w-0 flex-1">
      <div class="mb-2">
        <h2 class="font-mono leading-snug font-semibold break-all text-[var(--text)] text-base">
          {parseTitleAndSource(filePath).title}
        </h2>
        {#if parseTitleAndSource(filePath).source}
          <p class="mt-1 font-mono text-xs break-all text-[var(--muted)]">
            source: {parseTitleAndSource(filePath).source}
          </p>
        {/if}
      </div>
      {#if total > 0}
        <div class="flex flex-wrap items-center gap-2">
          <span
            class="inline-flex items-center gap-1 rounded-lg bg-blue-50 px-2.5 py-1 text-xs font-medium text-blue-700 ring-1 ring-blue-200 ring-inset dark:bg-blue-900/30 dark:text-blue-300 dark:ring-blue-800/50"
          >
            <svg class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
              />
            </svg>
            {total.toLocaleString()} 行
          </span>
          <span
            class="inline-flex items-center gap-1 rounded-lg bg-green-50 px-2.5 py-1 text-xs font-medium text-green-700 ring-1 ring-green-200 ring-inset dark:bg-green-900/30 dark:text-green-300 dark:ring-green-800/50"
          >
            <svg class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
            </svg>
            已加载 {loadedLines.toLocaleString()}
          </span>

          {#if keywords?.length}
            <!-- 关键词显示 -->
            <div class="flex flex-wrap items-center gap-1.5">
              {#each keywords.slice(0, 3) as keyword (keyword)}
                <span
                  class="inline-flex items-center rounded-md bg-yellow-50 px-2 py-0.5 text-xs font-medium text-yellow-800 ring-1 ring-yellow-600/20 ring-inset dark:bg-yellow-900/30 dark:text-yellow-300 dark:ring-yellow-600/30"
                  >{keyword}</span
                >
              {/each}
              {#if keywords.length > 3}
                <span class="text-xs text-[var(--muted)]">+{keywords.length - 3}</span>
              {/if}
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <!-- 右侧：下载按钮 -->
    <div class="flex shrink-0 items-start">
      <button
        class="inline-flex items-center gap-2 rounded-xl bg-[var(--primary)] px-4 py-2 text-sm font-semibold text-[var(--primary-foreground)] shadow-lg shadow-black/10 transition-all duration-300 hover:opacity-90 hover:shadow-xl hover:shadow-black/15 focus:ring-4 focus:ring-[var(--ring)] focus:outline-none disabled:opacity-50"
        onclick={onDownload}
        disabled={loading || total <= 0}
        title="下载当前文件"
      >
        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
          />
        </svg>
        <span>下载</span>
      </button>
    </div>
  </div>
</div>
