<script lang="ts">
  /**
   * 通用提示组件
   * 支持错误、成功、警告、信息等多种类型
   */
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

  // 类型样式映射
  const typeStyles = {
    error: {
      container: 'border-red-300 bg-red-50 text-red-700 dark:border-red-800 dark:bg-red-950 dark:text-red-200',
      icon: 'text-red-600 dark:text-red-400',
      button: 'bg-red-600 hover:bg-red-700 focus:ring-red-500'
    },
    success: {
      container: 'border-emerald-300 bg-emerald-50 text-emerald-700 dark:border-emerald-800 dark:bg-emerald-950 dark:text-emerald-200',
      icon: 'text-emerald-600 dark:text-emerald-400',
      button: 'bg-emerald-600 hover:bg-emerald-700 focus:ring-emerald-500'
    },
    warning: {
      container: 'border-amber-300 bg-amber-50 text-amber-700 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200',
      icon: 'text-amber-600 dark:text-amber-400',
      button: 'bg-amber-600 hover:bg-amber-700 focus:ring-amber-500'
    },
    info: {
      container: 'border-blue-300 bg-blue-50 text-blue-700 dark:border-blue-800 dark:bg-blue-950 dark:text-blue-200',
      icon: 'text-blue-600 dark:text-blue-400',
      button: 'bg-blue-600 hover:bg-blue-700 focus:ring-blue-500'
    }
  };

  const styles = typeStyles[type];
</script>

<div class="flex items-start gap-3 rounded-xl border px-4 py-3 text-sm shadow-sm {styles.container} {className}">
  {#if showIcon}
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      stroke="currentColor"
      stroke-width="1.5"
      class="mt-0.5 h-5 w-5 shrink-0 {styles.icon}"
      fill="none"
    >
      {#if type === 'error' || type === 'warning'}
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M12 9v4m0 4h.01m-.01-14a9 9 0 1 1 0 18 9 9 0 0 1 0-18z"
        />
      {:else if type === 'success'}
        <path stroke-linecap="round" stroke-linejoin="round" d="m5 13 4 4L19 7" />
      {:else}
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
        />
      {/if}
    </svg>
  {/if}

  <div class="flex-1 min-w-0">
    {#if title}
      <h3 class="font-semibold mb-1">{title}</h3>
    {/if}
    <p class="leading-relaxed">{message}</p>

    {#if actionLabel && onAction}
      <button
        class="mt-3 inline-flex items-center rounded-lg px-4 py-2 text-sm font-medium text-white transition-colors duration-200 focus:ring-2 focus:ring-offset-2 focus:outline-none {styles.button}"
        onclick={onAction}
      >
        {actionLabel}
      </button>
    {/if}
  </div>

  {#if onClose}
    <button
      type="button"
      class="shrink-0 rounded-lg p-1 transition-colors hover:bg-black/10 dark:hover:bg-white/10"
      onclick={onClose}
      aria-label="关闭"
    >
      <svg class="h-4 w-4" viewBox="0 0 24 24" stroke="currentColor" fill="none">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
      </svg>
    </button>
  {/if}
</div>

