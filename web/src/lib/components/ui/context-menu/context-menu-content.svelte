<script lang="ts">
  import { ContextMenu as ContextMenuPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils.js';
  import { fade } from 'svelte/transition';

  let { ref = $bindable(null), class: className, ...restProps }: ContextMenuPrimitive.ContentProps = $props();
</script>

<ContextMenuPrimitive.Content
  bind:ref
  class={cn(
    'bg-popover text-popover-foreground z-50 min-w-[8rem] overflow-hidden rounded-md border p-1 shadow-md transition-all',
    className
  )}
  {...restProps}
>
  {@render restProps.children?.()}
</ContextMenuPrimitive.Content>
