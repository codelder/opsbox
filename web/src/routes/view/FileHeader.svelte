<script lang="ts">
  /**
   * 文件查看页面 - 文件信息头部组件
   */
  import { parseOrl } from '$lib/utils/orl';
  import type { OrlInfo } from '$lib/utils/orl';
  import { Button } from '$lib/components/ui/button';
  import { Badge } from '$lib/components/ui/badge';
  import type { KeywordInfo } from '$lib/modules/logseek/types';
  import { Download, FileText, Database, Server, HardDrive, Archive, Folder, Minus, Plus } from 'lucide-svelte';

  // Lucide 图标类型
  type LucideIcon = typeof FileText;

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
    keywords?: KeywordInfo[];
    /**
     * 是否正在加载
     */
    loading?: boolean;
    /**
     * 下载回调
     */
    onDownload?: () => void;
    /**
     * 当前字体大小
     */
    fontSize?: string;
    /**
     * 字体大小改变回调
     */
    onFontSizeChange?: (size: string) => void;
    /**
     * 文件编码
     */
    encoding?: string;
  }

  let {
    filePath,
    total,
    loadedLines,
    keywords = [],
    loading = false,
    onDownload,
    fontSize = 'base',
    onFontSizeChange,
    encoding
  }: Props = $props();

  // Svelte 5 类型导出
  export type { Props };

  // 字体大小选项
  const fontSizeOptions = [
    { value: 'xs', label: 'XS', size: 'text-xs' },
    { value: 'sm', label: 'SM', size: 'text-sm' },
    { value: 'base', label: 'MD', size: 'text-base' },
    { value: 'lg', label: 'LG', size: 'text-lg' },
    { value: 'xl', label: 'XL', size: 'text-xl' }
  ];

  // 切换字体大小
  function handleFontSizeChange(newSize: string) {
    if (onFontSizeChange) {
      onFontSizeChange(newSize);
    }
  }

  // 减小字体大小
  function decreaseFontSize() {
    const currentIndex = fontSizeOptions.findIndex((opt) => opt.value === fontSize);
    if (currentIndex > 0) {
      const newSize = fontSizeOptions[currentIndex - 1].value;
      handleFontSizeChange(newSize);
    }
  }

  // 增大字体大小
  function increaseFontSize() {
    const currentIndex = fontSizeOptions.findIndex((opt) => opt.value === fontSize);
    if (currentIndex < fontSizeOptions.length - 1) {
      const newSize = fontSizeOptions[currentIndex + 1].value;
      handleFontSizeChange(newSize);
    }
  }

  // 获取当前字体大小显示标签
  const currentFontSizeLabel = $derived(fontSizeOptions.find((opt) => opt.value === fontSize)?.label || 'MD');

  // 统一解析标题与来源
  function parseFileInfo(full: string) {
    if (!full) return { title: '未知文件', icon: FileText, metadata: [] };

    const parsed: OrlInfo | null = parseOrl(full);
    if (!parsed) {
      const parts = full.split('/');
      return {
        title: parts[parts.length - 1] || full,
        icon: FileText,
        metadata: [{ label: full, icon: FileText, type: 'path' as const }]
      };
    }

    const title = parsed.displayName;
    const metadata: {
      label: string;
      icon: LucideIcon;
      title?: string;
      type: 's3' | 'agent' | 'local' | 'archive' | 'path';
    }[] = [];

    // 1. Endpoint Part
    if (parsed.endpointType === 's3') {
      const s3Label = parsed.endpointId.replace(':', ' / ');
      metadata.push({ label: `S3: ${s3Label}`, icon: Database, title: parsed.endpointId, type: 's3' });
    } else if (parsed.endpointType === 'agent') {
      metadata.push({ label: `Agent: ${parsed.endpointId}`, icon: Server, type: 'agent' });
    } else if (parsed.endpointType === 'local') {
      metadata.push({ label: 'Local Host', icon: HardDrive, title: parsed.endpointId, type: 'local' });
    }

    // 2. Path/Archive Part
    if (parsed.targetType === 'archive') {
      // Archive File - 显示完整路径
      if (parsed.path) {
        metadata.push({ label: parsed.path, icon: Archive, title: parsed.path, type: 'archive' });
      }

      // Entry Path - 显示完整路径（包括文件名）
      if (parsed.entryPath) {
        metadata.push({ label: parsed.entryPath, icon: Folder, title: parsed.entryPath, type: 'path' });
      }
    } else {
      // Standard Directory - 显示完整路径
      if (parsed.path) {
        metadata.push({ label: parsed.path, icon: Folder, title: parsed.path, type: 'path' });
      }
    }

    return { title, icon: FileText, metadata };
  }

  let fileInfo = $derived(parseFileInfo(filePath));
</script>

<!-- 标题栏 - 两行布局 -->
<div
  class="sticky top-0 z-10 flex items-center gap-4 border-b border-border bg-background/95 px-6 py-3 shadow-sm backdrop-blur-sm"
>
  <!-- 左侧：两行内容 -->
  <div class="flex min-w-0 flex-1 flex-col gap-2">
    <!-- 第一行：文件名 -->
    <div class="flex items-center gap-3">
      <div class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
        {#if fileInfo.icon}
          {@const Icon = fileInfo.icon}
          <Icon class="h-4 w-4" />
        {/if}
      </div>
      <h1
        class="min-w-0 flex-1 text-base font-semibold break-all"
        style="color: hsl(var(--foreground));"
        title={filePath}
      >
        {fileInfo.title}
      </h1>
    </div>

    <!-- 第二行：路径、统计信息和关键词 -->
    <div class="flex flex-wrap items-center gap-3 text-xs">
      <!-- 路径信息 -->
      {#if fileInfo.metadata.length > 0}
        <div class="flex flex-wrap items-center gap-1.5 text-muted-foreground">
          {#each fileInfo.metadata as meta, i (meta)}
            {@const commonClass = 'bg-muted/40 text-muted-foreground border-border/40 hover:bg-muted/60'}
            {@const colorClasses = {
              s3: commonClass,
              agent: commonClass,
              local: commonClass,
              archive: commonClass,
              path: commonClass
            }}
            <div
              class="flex items-center gap-1 rounded-md border px-1.5 py-0.5 {colorClasses[meta.type] ||
                'border-border/50 bg-muted/50 text-muted-foreground'}"
              title={meta.title || meta.label}
            >
              {#if meta.icon}
                {@const MetaIcon = meta.icon}
                <MetaIcon class="h-3 w-3 shrink-0" />
              {/if}
              <span class="font-mono text-[11px] break-all">{meta.label}</span>
            </div>
            {#if i < fileInfo.metadata.length - 1}
              <span class="text-muted-foreground/40">/</span>
            {/if}
          {/each}
        </div>
      {/if}

      <!-- 统计信息 -->
      {#if total > 0}
        <div class="flex items-center gap-1.5">
          <div class="h-1.5 w-1.5 rounded-full bg-blue-500"></div>
          <span class="font-medium text-foreground">{total.toLocaleString()}</span>
          <span class="text-muted-foreground">行</span>
          {#if loadedLines < total}
            <span class="text-muted-foreground/60">·</span>
            <span class="text-muted-foreground">已加载 {loadedLines.toLocaleString()}</span>
          {/if}
        </div>
      {/if}

      <!-- 关键词 -->
      {#if keywords?.length}
        <div class="flex items-center gap-1.5">
          <div class="flex flex-wrap items-center gap-1">
            {#each keywords.slice(0, 3) as keyword (`${keyword.type}:${keyword.text}`)}
              <Badge
                variant="secondary"
                class="h-5 border border-amber-200/50 bg-amber-50/80 px-2 text-[10px] font-medium text-amber-700 dark:border-amber-800/50 dark:bg-amber-900/30 dark:text-amber-400"
              >
                {keyword.text}
              </Badge>
            {/each}
            {#if keywords.length > 3}
              <span class="text-xs text-muted-foreground">+{keywords.length - 3}</span>
            {/if}
          </div>
        </div>
      {/if}

      <!-- 编码 -->
      {#if encoding}
        <Badge
          variant="outline"
          class="h-5 border-blue-200/50 bg-blue-50/50 px-2 text-[10px] font-medium text-blue-700 dark:border-blue-800/30 dark:bg-blue-900/20 dark:text-blue-400/80"
        >
          {encoding}
        </Badge>
      {/if}
    </div>
  </div>

  <!-- 右侧：字体大小控制和下载按钮 -->
  <div class="flex shrink-0 items-center gap-2">
    <!-- 字体大小控制 -->
    <div class="flex items-center rounded-md border border-border bg-background">
      <!-- 减小按钮 -->
      <Button
        variant="ghost"
        size="sm"
        class="h-7 w-7 rounded-none rounded-l-md p-0 hover:bg-muted"
        onclick={decreaseFontSize}
        disabled={fontSize === fontSizeOptions[0].value}
        title="减小字体"
      >
        <Minus class="h-3.5 w-3.5" />
      </Button>

      <!-- 当前字体大小显示 -->
      <div class="flex h-7 w-8 items-center justify-center border-x border-border px-2">
        <span class="text-xs font-medium">{currentFontSizeLabel}</span>
      </div>

      <!-- 增大按钮 -->
      <Button
        variant="ghost"
        size="sm"
        class="h-7 w-7 rounded-none rounded-r-md p-0 hover:bg-muted"
        onclick={increaseFontSize}
        disabled={fontSize === fontSizeOptions[fontSizeOptions.length - 1].value}
        title="增大字体"
      >
        <Plus class="h-3.5 w-3.5" />
      </Button>
    </div>

    <!-- 分隔线 -->
    <div class="h-5 w-px bg-border"></div>

    <!-- 下载按钮 -->
    <Button
      variant="outline"
      size="sm"
      class="h-8 gap-2"
      onclick={onDownload}
      disabled={loading || total <= 0}
      title="下载当前文件"
    >
      <Download class="h-4 w-4" />
      <span class="hidden sm:inline">下载</span>
    </Button>
  </div>
</div>
