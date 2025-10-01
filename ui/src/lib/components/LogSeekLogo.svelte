<script lang="ts">
  interface Props {
    /**
     * Logo 尺寸：'small' | 'medium' | 'large' | 'xlarge'
     */
    size?: 'small' | 'medium' | 'large' | 'xlarge';
    /**
     * 是否显示为标签（label），将 cursor 设为 pointer
     */
    asLabel?: boolean;
    /**
     * 是否启用 hover 缩放效果
     */
    hoverable?: boolean;
    /**
     * 自定义 class
     */
    class?: string;
    /**
     * HTML for 属性（当 asLabel 为 true 时使用）
     */
    htmlFor?: string;
  }

  let { size = 'large', asLabel = false, hoverable = false, class: className = '', htmlFor }: Props = $props();

  // 尺寸映射
  const sizeClasses = {
    small: 'text-3xl md:text-4xl',
    medium: 'text-4xl md:text-6xl',
    large: 'text-6xl md:text-8xl',
    xlarge: 'text-8xl md:text-10xl'
  };

  const baseClass = `inline-block font-extrabold tracking-[-0.25em] italic antialiased select-none ${sizeClasses[size]}`;
  const hoverClass = hoverable ? 'transform transition-transform duration-300 hover:scale-105' : '';
  const cursorClass = asLabel ? 'cursor-pointer' : '';
</script>

{#if asLabel && htmlFor}
  <label for={htmlFor} class="{baseClass} {hoverClass} {cursorClass} {className}">
    <span class="text-blue-600 drop-shadow-sm">L</span>
    <span class="text-red-600 drop-shadow-sm">o</span>
    <span class="text-yellow-500 drop-shadow-sm">g</span>
    <span class="text-green-600 drop-shadow-sm">s</span>
    <span class="text-blue-600 drop-shadow-sm">e</span>
    <span class="text-red-600 drop-shadow-sm">e</span>
    <span class="text-yellow-500 drop-shadow-sm">k</span>
  </label>
{:else}
  <div class="{baseClass} {hoverClass} {className}">
    <span class="text-blue-600 drop-shadow-sm">L</span>
    <span class="text-red-600 drop-shadow-sm">o</span>
    <span class="text-yellow-500 drop-shadow-sm">g</span>
    <span class="text-green-600 drop-shadow-sm">s</span>
    <span class="text-blue-600 drop-shadow-sm">e</span>
    <span class="text-red-600 drop-shadow-sm">e</span>
    <span class="text-yellow-500 drop-shadow-sm">k</span>
  </div>
{/if}
