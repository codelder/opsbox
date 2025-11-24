<script lang="ts">
  import { Tabs as TabsPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils.js';

  let { class: className, value, children, ref = $bindable(null), ...restProps }: TabsPrimitive.ContentProps = $props();
</script>

<TabsPrimitive.Content
  bind:ref
  {value}
  class={cn(
    'mt-2 ring-offset-background focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:outline-none',
    className
  )}
  {...restProps}
>
  {@render children?.()}
</TabsPrimitive.Content>
