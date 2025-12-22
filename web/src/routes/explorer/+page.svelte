<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { type ResourceItem, type Odfi, listResources } from '$lib/modules/explorer';
  import {
    Folder,
    File,
    ArrowLeft,
    RefreshCw,
    Home,
    Download,
    Server,
    HardDrive,
    Cloud,
    ChevronRight,
    ChevronDown,
    Monitor
  } from 'lucide-svelte';

  // State
  let currentOdfiStr = $state('');
  let items: ResourceItem[] = $state([]);
  let loading = $state(false);
  let error: string | null = $state(null);
  let urlOdfi = page.url.searchParams.get('odfi');

  // Sidebar State
  let expandedSections = $state({
    s3: false,
    agent: false
  });
  let sidebarData = $state({
    s3: [] as ResourceItem[],
    agent: [] as ResourceItem[]
  });
  let sidebarLoading = $state({
    s3: false,
    agent: false
  });

  // Derived active section (simple heuristic)
  let activeSection = $derived.by(() => {
    if (currentOdfiStr.startsWith('odfi://local')) return 'local';
    if (currentOdfiStr.startsWith('odfi://s3')) return 's3';
    if (currentOdfiStr.includes('@agent') || currentOdfiStr.startsWith('odfi://agent')) return 'agent';
    return null;
  });

  // Initial load
  onMount(() => {
    if (urlOdfi) {
      currentOdfiStr = urlOdfi;
      loadResources(urlOdfi);
    } else {
      // Default to local root
      currentOdfiStr = 'odfi://local/';
      loadResources(currentOdfiStr);
    }
  });

  async function loadResources(odfi: string) {
    loading = true;
    error = null;
    try {
      items = await listResources(odfi);
    } catch (e: any) {
      error = e.message;
      items = [];
    } finally {
      loading = false;
    }
  }

  async function toggleSection(section: 's3' | 'agent') {
    expandedSections[section] = !expandedSections[section];

    // Load if expanding and empty
    if (expandedSections[section] && sidebarData[section].length === 0) {
      sidebarLoading[section] = true;
      try {
        const rootOdfi = section === 's3' ? 'odfi://s3/' : 'odfi://agent/';
        sidebarData[section] = await listResources(rootOdfi);
      } catch (e) {
        console.error(`Failed to load sidebar ${section}`, e);
      } finally {
        sidebarLoading[section] = false;
      }
    }
  }

  function handleNavigate(newOdfi: string) {
    currentOdfiStr = newOdfi;
    // Update URL without reload
    const url = new URL(window.location.href);
    url.searchParams.set('odfi', newOdfi);
    // Remove title query if present to clean up
    goto(url.toString(), { keepFocus: true, noScroll: true });
    loadResources(newOdfi);
  }

  function handleRowClick(item: ResourceItem) {
    if (item.type === 'dir') {
      handleNavigate(item.path);
    } else {
      // File action? Download or Preview?
      console.log('File clicked:', item.path);
    }
  }

  function goUp() {
    try {
      let urlStr = currentOdfiStr;
      const url = new URL(urlStr);

      // Check if we are inside an archive
      const entry = url.searchParams.get('entry');
      if (entry) {
        const entryParts = entry.split('/').filter((p) => p);
        if (entryParts.length > 1) {
          entryParts.pop();
          url.searchParams.set('entry', entryParts.join('/'));
        } else {
          url.searchParams.delete('entry');
        }
        handleNavigate(url.toString());
        return;
      }

      // Normal path navigation
      const pathParts = url.pathname.split('/').filter((p) => p);
      if (pathParts.length > 0) {
        pathParts.pop();
        url.pathname = '/' + pathParts.join('/');
        handleNavigate(url.toString());
      }
    } catch (e) {
      console.error('Failed to parse ODFI for parent navigation', e);
      if (currentOdfiStr.includes('/')) {
        const parts = currentOdfiStr.split('/');
        parts.pop();
        handleNavigate(parts.join('/') || 'odfi://local/');
      }
    }
  }

  function formatSize(bytes: number | null | undefined): string {
    if (bytes === null || bytes === undefined) return '';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let i = 0;
    while (bytes >= 1024 && i < units.length - 1) {
      bytes /= 1024;
      i++;
    }
    return `${bytes.toFixed(1)} ${units[i]}`;
  }
</script>

<div class="flex h-[calc(100vh-4rem)] overflow-hidden">
  <!-- Sidebar -->
  <div class="flex w-64 flex-col border-r border-border/40 bg-card/30 dark:border-gray-700/50">
    <div class="border-b border-border/40 p-4 text-lg font-semibold dark:border-gray-700/50">Explorer</div>
    <div class="flex-1 space-y-1 overflow-y-auto p-2">
      <!-- Local Node -->
      <button
        class="flex w-full items-center rounded-md px-2 py-1.5 text-sm font-medium hover:bg-accent/50 {activeSection ===
        'local'
          ? 'bg-accent text-accent-foreground'
          : 'text-muted-foreground'}"
        onclick={() => handleNavigate('odfi://local/')}
      >
        <Monitor class="mr-2 h-4 w-4" />
        Local Machine
      </button>

      <!-- Agent Node -->
      <div>
        <button
          class="flex w-full items-center rounded-md px-2 py-1.5 text-sm font-medium hover:bg-accent/50 {activeSection ===
          'agent'
            ? 'text-foreground'
            : 'text-muted-foreground'}"
          onclick={() => toggleSection('agent')}
        >
          {#if expandedSections.agent}
            <ChevronDown class="mr-2 h-4 w-4" />
          {:else}
            <ChevronRight class="mr-2 h-4 w-4" />
          {/if}
          <Server class="mr-2 h-4 w-4" />
          Agents
        </button>
        {#if expandedSections.agent}
          <div class="mt-1 space-y-1 pl-6">
            {#if sidebarLoading.agent}
              <div class="px-2 py-1 text-xs text-muted-foreground">Loading agents...</div>
            {:else if sidebarData.agent.length === 0}
              <div class="px-2 py-1 text-xs text-muted-foreground">No online agents</div>
            {:else}
              {#each sidebarData.agent as agent}
                <button
                  class="flex w-full items-center truncate rounded-md px-2 py-1 text-sm hover:bg-accent/50 {currentOdfiStr.includes(
                    agent.name
                  )
                    ? 'bg-accent text-accent-foreground'
                    : 'text-muted-foreground'}"
                  onclick={() => handleNavigate(agent.path)}
                  title={agent.name}
                >
                  <div class="mr-2 h-2 w-2 flex-shrink-0 rounded-full bg-green-500"></div>
                  <span class="truncate">{agent.name}</span>
                </button>
              {/each}
            {/if}
          </div>
        {/if}
      </div>

      <!-- S3 Node -->
      <div>
        <button
          class="flex w-full items-center rounded-md px-2 py-1.5 text-sm font-medium hover:bg-accent/50 {activeSection ===
          's3'
            ? 'text-foreground'
            : 'text-muted-foreground'}"
          onclick={() => toggleSection('s3')}
        >
          {#if expandedSections.s3}
            <ChevronDown class="mr-2 h-4 w-4" />
          {:else}
            <ChevronRight class="mr-2 h-4 w-4" />
          {/if}
          <Cloud class="mr-2 h-4 w-4" />
          S3 Storage
        </button>
        {#if expandedSections.s3}
          <div class="mt-1 space-y-1 pl-6">
            {#if sidebarLoading.s3}
              <div class="px-2 py-1 text-xs text-muted-foreground">Loading profiles...</div>
            {:else if sidebarData.s3.length === 0}
              <div class="px-2 py-1 text-xs text-muted-foreground">No profiles found</div>
            {:else}
              {#each sidebarData.s3 as profile}
                <button
                  class="flex w-full items-center truncate rounded-md px-2 py-1 text-sm hover:bg-accent/50 {currentOdfiStr.includes(
                    profile.name
                  )
                    ? 'bg-accent text-accent-foreground'
                    : 'text-muted-foreground'}"
                  onclick={() => handleNavigate(profile.path)}
                >
                  <div class="mr-2 h-2 w-2 flex-shrink-0 rounded-full bg-blue-500"></div>
                  <span class="truncate">{profile.name}</span>
                </button>
              {/each}
            {/if}
          </div>
        {/if}
      </div>
    </div>
  </div>

  <!-- Main Content -->
  <div class="flex flex-1 flex-col overflow-hidden bg-background">
    <!-- Toolbar -->
    <div class="flex items-center space-x-2 border-b border-border/40 p-4 dark:border-gray-700/50">
      <Button variant="ghost" size="icon" onclick={goUp} disabled={loading}>
        <ArrowLeft class="h-4 w-4" />
      </Button>
      <Button variant="ghost" size="icon" onclick={() => loadResources(currentOdfiStr)} disabled={loading}>
        <RefreshCw class="h-4 w-4 {loading ? 'animate-spin' : ''}" />
      </Button>
      <div
        class="flex flex-1 items-center rounded-md border border-border/40 bg-muted/50 px-3 py-1.5 focus-within:ring-1 focus-within:ring-ring dark:border-gray-700/50"
      >
        <span class="mr-2 font-mono text-xs text-muted-foreground select-none">ODFI://</span>
        <input
          class="w-full flex-1 border-none bg-transparent font-mono text-sm outline-none"
          bind:value={currentOdfiStr}
          onkeydown={(e) => e.key === 'Enter' && handleNavigate(currentOdfiStr)}
        />
      </div>
    </div>

    <!-- Content Area -->
    <div class="flex-1 overflow-auto p-4">
      {#if error}
        <div class="rounded-md border border-destructive/20 bg-destructive/5 p-8 text-center text-destructive">
          <div class="mb-2 text-lg font-semibold">Error loading resources</div>
          <div class="text-sm opacity-90">{error}</div>
        </div>
      {:else}
        <div class="rounded-md border border-border/40 dark:border-gray-700/50">
          <table class="w-full text-sm">
            <thead class="block w-full bg-muted/40">
              <tr class="flex w-full">
                <th
                  class="flex h-10 w-12 shrink-0 items-center justify-center px-4 text-left align-middle font-medium text-muted-foreground"
                ></th>
                <th class="flex h-10 flex-1 items-center px-4 text-left align-middle font-medium text-muted-foreground"
                  >Name</th
                >
                <th
                  class="flex h-10 w-24 shrink-0 items-center justify-end px-4 text-right align-middle font-medium text-muted-foreground"
                  >Size</th
                >
                <th
                  class="flex h-10 w-40 shrink-0 items-center justify-end px-4 text-right align-middle font-medium text-muted-foreground"
                  >Modified</th
                >
              </tr>
            </thead>
            <tbody class="block max-h-[calc(100vh-16rem)] w-full overflow-y-auto">
              {#if items.length === 0 && !loading}
                <tr class="flex w-full border-t border-border/40 dark:border-gray-700/50">
                  <td class="w-full p-8 text-center text-muted-foreground"> This directory is empty. </td>
                </tr>
              {/if}
              {#each items as item}
                <tr
                  class="flex w-full cursor-pointer border-t border-border/40 hover:bg-muted/50 dark:border-gray-700/50"
                  onclick={() => handleRowClick(item)}
                >
                  <td class="flex w-12 flex-shrink-0 items-center justify-center p-2">
                    {#if item.type === 'dir'}
                      <Folder class="h-4 w-4 fill-current text-blue-500" />
                    {:else}
                      <File class="h-4 w-4 text-muted-foreground" />
                    {/if}
                  </td>
                  <td class="flex flex-1 items-center truncate p-2 font-medium">
                    {item.name}
                  </td>
                  <td
                    class="flex w-24 flex-shrink-0 items-center justify-end p-2 font-mono text-xs text-muted-foreground"
                  >
                    {formatSize(item.size)}
                  </td>
                  <td
                    class="flex w-40 flex-shrink-0 items-center justify-end p-2 font-mono text-xs text-muted-foreground"
                  >
                    {#if item.modified}
                      {new Date(item.modified * 1000).toLocaleString()}
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  </div>
</div>
