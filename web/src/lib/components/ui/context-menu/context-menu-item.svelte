<script lang="ts">
  import { ContextMenu as ContextMenuPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    class: className,
    inset,
    children,
    ...restProps
  }: ContextMenuPrimitive.ItemProps & { inset?: boolean } = $props();
</script>

<ContextMenuPrimitive.Item
  bind:ref
  class={cn(
    'relative flex cursor-default items-center rounded-md px-2 py-1.5 text-sm transition-colors outline-none select-none data-disabled:pointer-events-none data-disabled:opacity-50',
    inset && 'pl-8',
    className
  )}
  {...restProps}
>
  <div class="pointer-events-auto contents">
    {@render children?.()}
  </div>
</ContextMenuPrimitive.Item>
