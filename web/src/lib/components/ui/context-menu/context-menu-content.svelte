<script lang="ts">
  import { ContextMenu as ContextMenuPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils.js';
  import { fade } from 'svelte/transition';

  let { ref = $bindable(null), class: className, ...restProps }: ContextMenuPrimitive.ContentProps = $props();
</script>

<ContextMenuPrimitive.Content
  bind:ref
  class={cn(
    'z-50 min-w-[8rem] overflow-hidden rounded-md border border-black/10 bg-white/55 p-1 text-neutral-900 shadow-md backdrop-blur-xl transition-all dark:border-white/10 dark:bg-[#1c1c1e]/65 dark:text-white/90',
    className
  )}
  {...restProps}
>
  {@render restProps.children?.()}
</ContextMenuPrimitive.Content>
