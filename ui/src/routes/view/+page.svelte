<script lang="ts">
  import { onMount } from 'svelte';
  import { env } from '$env/dynamic/public';

  const API_BASE = env.PUBLIC_API_BASE || '/api/v1/logsearch';

  let file = '';
  let sid = '';
  let total = 0;
  let start = 1;
  let end = 0;
  let keywords: string[] = [];
  let lines: { no: number; text: string }[] = [];
  let loading = false;
  let error: string | null = null;

  async function fetchRange(s: number, e: number) {
    const url = `${API_BASE}/view.cache.json?sid=${encodeURIComponent(sid)}&file=${encodeURIComponent(file)}&start=${s}&end=${e}`;
    const res = await fetch(url, { headers: { Accept: 'application/json' } });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    return data as {
      file: string;
      total: number;
      start: number;
      end: number;
      keywords: string[];
      lines: { no: number; text: string }[];
    };
  }

  async function loadInitial() {
    try {
      loading = true;
      const data = await fetchRange(1, 500);
      file = data.file;
      total = data.total;
      start = data.start;
      end = data.end;
      keywords = data.keywords || [];
      lines = data.lines || [];
    } catch (e: any) {
      error = e?.message || '加载失败';
    } finally {
      loading = false;
    }
  }

  async function loadMore() {
    if (end >= total) return;
    try {
      loading = true;
      const nextS = end + 1;
      const nextE = Math.min(nextS + 999, total);
      const data = await fetchRange(nextS, nextE);
      end = data.end;
      lines = [...lines, ...(data.lines || [])];
    } catch (e: any) {
      error = e?.message || '加载更多失败';
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    const params = new URL(window.location.href).searchParams;
    file = (params.get('file') || '').trim();
    sid = (params.get('sid') || '').trim();
    if (!file) {
      error = '缺少 file 参数';
      return;
    }
    if (!sid) {
      error = '缺少 sid 参数';
      return;
    }
    loadInitial();
  });
</script>

<div class="mx-auto max-w-[1560px] px-4 py-6">
  <h2 class="mb-2 font-mono text-sm text-gray-600 dark:text-gray-300">{file}</h2>
  {#if error}
    <div class="mb-3 rounded border border-red-300 bg-red-50 p-3 text-red-700 dark:border-red-800 dark:bg-red-950 dark:text-red-300">{error}</div>
  {/if}
  <div class="mb-2 text-xs text-gray-500 dark:text-gray-400">{total > 0 ? `共 ${total} 行` : ''}</div>

  <div class="rounded border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-800">
    {#each lines as ln (ln.no)}
      <div class="grid grid-cols-[72px_1fr] gap-0 font-mono text-[13px] leading-[20px]">
        <div class="border-r border-gray-100 bg-gray-50 px-3 py-0.5 text-right text-gray-400 select-none dark:border-gray-700 dark:bg-gray-900 dark:text-gray-500">{ln.no}</div>
        <div class="px-3 py-0.5 break-all whitespace-pre-wrap">{ln.text}</div>
      </div>
    {/each}
  </div>

  <div class="mt-4">
    {#if end < total}
      <button class="rounded bg-gray-700 px-3 py-2 text-sm text-white disabled:opacity-50" onclick={loadMore} disabled={loading}>{loading ? '加载中…' : '加载更多'}</button>
    {:else}
      <span class="text-sm text-gray-500 dark:text-gray-400">已到结尾。</span>
    {/if}
  </div>
</div>
