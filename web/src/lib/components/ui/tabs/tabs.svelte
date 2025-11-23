<script lang="ts">
	import { Tabs as TabsPrimitive } from "bits-ui";

	let {
		value = $bindable(),
		ref = $bindable(null),
		children,
		...restProps
	}: TabsPrimitive.RootProps = $props();
</script>

<TabsPrimitive.Root bind:value bind:ref {...restProps}>
	{@render children?.()}
</TabsPrimitive.Root>
