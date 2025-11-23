<script lang="ts">
	import type { HTMLAttributes } from "svelte/elements";
	import { cn } from "$lib/utils.js";

	let {
		class: className,
		children,
		ref = $bindable(null),
		...restProps
	}: HTMLAttributes<HTMLParagraphElement> & {
		ref?: HTMLParagraphElement | null;
	} = $props();
</script>

<div bind:this={ref} class={cn("text-sm [&_p]:leading-relaxed", className)} {...restProps}>
	{@render children?.()}
</div>
