<script lang="ts">
  import { onMount } from 'svelte';
  import { marked } from 'marked';

  let content = $state('');
  let loading = $state(true);

  onMount(async () => {
    try {
      const res = await fetch('/query-syntax.md');
      if (res.ok) {
        const text = await res.text();
        content = await marked.parse(text);
      } else {
        content = '<p class="text-destructive">System prompt file not found.</p>';
      }
    } catch (e) {
      content = `<p class="text-destructive">Error loading system prompt: ${e}</p>`;
    } finally {
      loading = false;
    }
  });
</script>

<div class="container mx-auto max-w-4xl py-8 px-4">
  <h1 class="text-2xl font-bold mb-4">系统提示词 (System Prompt)</h1>

  {#if loading}
    <p>Loading...</p>
  {:else}
    <div class="prose dark:prose-invert max-w-none">
      {@html content}
    </div>
  {/if}
</div>
