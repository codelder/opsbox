<script lang="ts">
  import { toggleMode } from 'mode-watcher';
  import { Button } from '$lib/components/ui/button';
  import { Sun, Moon } from 'lucide-svelte';
</script>

<Button
  variant="ghost"
  size="icon"
  onclick={toggleMode}
  aria-label="Toggle theme"
  class="rounded-full bg-secondary/50 hover:bg-secondary"
>
  <Sun class="h-[1.2rem] w-[1.2rem] scale-100 rotate-0 transition-all dark:scale-0 dark:-rotate-90" />
  <Moon class="absolute h-[1.2rem] w-[1.2rem] scale-0 rotate-90 transition-all dark:scale-100 dark:rotate-0" />
  <span class="sr-only">Toggle theme</span>
</Button>
