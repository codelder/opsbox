<script lang="ts">
  /**
   * 通用提示组件
   * 支持错误、成功、警告、信息等多种类型
   */
  import { Alert, AlertTitle, AlertDescription } from '$lib/components/ui/alert';
  import { Button } from '$lib/components/ui/button';
  import { X, AlertCircle, CheckCircle2, Info, AlertTriangle } from 'lucide-svelte';

  interface Props {
    /**
     * 提示类型
     */
    type?: 'error' | 'success' | 'warning' | 'info';
    /**
     * 提示标题
     */
    title?: string;
    /**
     * 提示内容
     */
    message: string;
    /**
     * 是否显示图标
     */
    showIcon?: boolean;
    /**
     * 自定义 class
     */
    class?: string;
    /**
     * 关闭按钮点击回调
     */
    onClose?: () => void;
    /**
     * 操作按钮
     */
    actionLabel?: string;
    /**
     * 操作按钮点击回调
     */
    onAction?: () => void;
  }

  let {
    type = 'info',
    title,
    message,
    showIcon = true,
    class: className = '',
    onClose,
    actionLabel,
    onAction
  }: Props = $props();

  // 映射到 shadcn variant
  const variantMap = {
    error: 'destructive',
    success: 'default', // success needs custom styling or a new variant, using default with custom class for now
    warning: 'default',
    info: 'default'
  } as const;

  const iconMap = {
    error: AlertCircle,
    success: CheckCircle2,
    warning: AlertTriangle,
    info: Info
  };

  let Icon = $derived(iconMap[type]);

  // Custom styles for non-destructive variants to match original intent
  const typeClasses = {
    success:
      'border-green-200 bg-green-50 text-green-800 dark:border-green-900 dark:bg-green-950 dark:text-green-300 [&>svg]:text-green-600 dark:[&>svg]:text-green-400',
    warning:
      'border-amber-200 bg-amber-50 text-amber-800 dark:border-amber-900 dark:bg-amber-950 dark:text-amber-300 [&>svg]:text-amber-600 dark:[&>svg]:text-amber-400',
    info: 'border-blue-200 bg-blue-50 text-blue-800 dark:border-blue-900 dark:bg-blue-950 dark:text-blue-300 [&>svg]:text-blue-600 dark:[&>svg]:text-blue-400',
    error: '' // handled by destructive variant
  };
</script>

<Alert
  variant={variantMap[type]}
  class="{type !== 'error' ? typeClasses[type] : ''} {className} {onClose ? 'pr-12' : ''}"
  data-testid="alert"
>
  {#if showIcon}
    <Icon class="h-4 w-4" />
  {/if}

  <div class="flex flex-col gap-1">
    {#if title}
      <AlertTitle>{title}</AlertTitle>
    {/if}
    <AlertDescription>
      {message}
      {#if actionLabel && onAction}
        <div class="mt-2">
          <Button
            variant="outline"
            size="sm"
            onclick={onAction}
            class="h-7 border-current/30 bg-transparent text-xs hover:bg-current/10 hover:text-current"
          >
            {actionLabel}
          </Button>
        </div>
      {/if}
    </AlertDescription>
  </div>

  {#if onClose}
    <Button
      variant="ghost"
      size="icon"
      class="absolute top-2 right-2 h-6 w-6 text-current/60 hover:bg-current/10 hover:text-current"
      onclick={onClose}
      aria-label="关闭"
    >
      <X class="h-4 w-4" />
    </Button>
  {/if}
</Alert>
