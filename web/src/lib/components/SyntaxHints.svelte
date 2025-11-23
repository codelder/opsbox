<script lang="ts">
  /**
   * 语法提示按钮组组件
   * 提供快捷插入查询语法的按钮
   */
  import { Button } from "$lib/components/ui/button";

  interface Props {
    /**
     * 插入文本的回调函数
     * @param snippet 要插入的文本
     * @param caretOffsetFromEnd 光标相对片段末尾的偏移量
     */
    onInsert: (snippet: string, caretOffsetFromEnd?: number) => void;
  }

  let { onInsert }: Props = $props();

  const hints = [
    { label: 'OR', snippet: ' OR ', title: '逻辑或（必须大写）' },
    { label: 'AND', snippet: ' AND ', title: '逻辑与（必须大写）；相邻词默认 AND' },
    { label: '-exclude', snippet: '-', title: '排除词，例如 -debug' },
    { label: '"phrase"', snippet: '""', caretOffset: 1, title: '短语匹配：插入一对引号' },
    { label: '/regex/', snippet: '//', caretOffset: 1, title: '正则匹配：插入 /.../' },
    { label: 'path:glob', snippet: 'path:logs/*.log ', title: '路径限定（glob），示例 path:logs/*.log' }
  ];

  const exampleSnippet = '(taxResult OR taxWarn) /"9111[0-9A-Z]{14}"/ dt:20250818 path:ptcr -path:system.log';
</script>

<div class="mt-4 flex flex-wrap items-center justify-center gap-2 text-xs text-muted-foreground">
  <span class="mr-1 select-none">语法提示：</span>
  {#each hints as hint (hint.label)}
    <Button
      variant="outline"
      size="sm"
      class="h-6 rounded-full px-2.5 text-xs font-normal hover:bg-secondary hover:text-secondary-foreground"
      onclick={() => onInsert(hint.snippet, hint.caretOffset)}
      title={hint.title}
      type="button"
    >
      {hint.label}
    </Button>
  {/each}
  <Button
    variant="link"
    size="sm"
    class="h-6 px-1 text-xs text-muted-foreground underline-offset-2 hover:text-foreground"
    onclick={() => onInsert(exampleSnippet)}
    title="插入完整示例"
    type="button"
  >
    示例
  </Button>
</div>
