<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements';
  import { cn } from '$lib/utils.js';

  let {
    class: className,
    children,
    ref = $bindable(null),
    ...restProps
  }: HTMLAttributes<HTMLHeadingElement> & {
    ref?: HTMLHeadingElement | null;
  } = $props();
</script>

<h5 bind:this={ref} class={cn('mb-1 leading-none font-medium tracking-tight', className)} {...restProps}>
  {@render children?.()}
</h5>
