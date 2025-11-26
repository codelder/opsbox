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

<!-- 标题栏 - 仿 GitHub 风格 -->
<div
  class="flex items-center justify-between border-b border-border bg-muted/30 px-4 py-2 text-sm dark:border-gray-700 dark:bg-[#161b22]"
>
  <!-- 左侧：文件信息 -->
  <div class="flex min-w-0 flex-1 items-center gap-3">
    <!-- 文件图标和名称 -->
    <div class="flex items-center gap-1.5 font-mono text-sm">
      <svg
        class="h-4 w-4 shrink-0 text-muted-foreground"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        stroke-width="2"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
        />
      </svg>
      <span class="truncate font-bold text-foreground">{parseTitleAndSource(filePath).title}</span>
    </div>

    {#if parseTitleAndSource(filePath).source}
      <span class="hidden text-xs text-muted-foreground sm:inline">
        · {parseTitleAndSource(filePath).source}
      </span>
    {/if}

    <!-- 统计信息 -->
    {#if total > 0}
      <div class="hidden items-center gap-2 text-xs text-muted-foreground sm:flex">
        <span class="flex items-center gap-1">
          <span class="h-2 w-2 rounded-full bg-blue-400"></span>
          {total.toLocaleString()} 行
        </span>
        <span class="flex items-center gap-1">
          <span class="h-2 w-2 rounded-full bg-green-400"></span>
          已加载 {loadedLines.toLocaleString()}
        </span>
      </div>
    {/if}

    <!-- 关键词 -->
    {#if keywords?.length}
      <div class="hidden items-center gap-1.5 md:flex">
        {#each keywords.slice(0, 3) as keyword (keyword)}
          <span
            class="inline-flex items-center rounded-full bg-amber-500/10 px-2 py-0.5 text-xs font-medium text-amber-700 dark:text-amber-400"
            >{keyword}</span
          >
        {/each}
        {#if keywords.length > 3}
          <span class="text-xs text-muted-foreground">+{keywords.length - 3}</span>
        {/if}
      </div>
    {/if}
  </div>

  <!-- 右侧：下载按钮 -->
  <div class="flex shrink-0 items-center gap-1">
    <button
      class="inline-flex h-7 w-7 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground disabled:opacity-50"
      onclick={onDownload}
      disabled={loading || total <= 0}
      title="下载当前文件"
    >
      <svg class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
        />
      </svg>
    </button>
  </div>
</div>
