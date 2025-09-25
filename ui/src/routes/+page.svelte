<script lang="ts">
  import { env } from '$env/dynamic/public';
  import { IconRobot, IconFunction } from '@tabler/icons-svelte';
  // 工具函数：将片段插入到输入框光标位置
  let inputEl: HTMLInputElement | null = null;

  // 首页搜索框模式：true=AI（自然语言），false=表达式（查询串）
  let aiMode = $state(false);
  let aiLoading = $state(false);

  /**
   * 在当前光标位置插入文本片段
   * @param snippet 要插入的文本
   * @param caretOffsetFromEnd 光标相对片段末尾的偏移量（用于把光标放到引号/斜杠中间）
   */
  function insertSnippet(snippet: string, caretOffsetFromEnd: number = 0) {
    if (!inputEl) return;
    const el = inputEl;
    el.focus();
    const start = el.selectionStart ?? el.value.length;
    const end = el.selectionEnd ?? el.value.length;
    const before = el.value.slice(0, start);
    const after = el.value.slice(end);
    el.value = before + snippet + after;
    const caret = before.length + snippet.length - caretOffsetFromEnd;
    el.setSelectionRange(caret, caret);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  }

  // 中文注释：处理首页搜索框提交逻辑
  async function handleHomeSubmit(e: Event) {
    e.preventDefault();
    const text = (inputEl?.value || '').trim();
    if (!text || aiLoading) return;
    if (!aiMode) {
      // 表达式模式：直接跳到 /search?q=
      window.location.href = `/search?q=${encodeURIComponent(text)}`;
      return;
    }
    // AI 模式：先调用 nl2q，再跳转
    aiLoading = true;
    try {
      const API_BASE = env.PUBLIC_API_BASE || '/api/v1/logsearch';
      const res = await fetch(`${API_BASE}/nl2q`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
        body: JSON.stringify({ nl: text })
      });

      if (!res.ok) {
        console.error('AI 生成失败：AI 服务异常', res.status);
        return;
      }

      const data = (await res.json()) as { q?: string };
      const q = (data?.q || '').trim();

      if (!q) {
        console.error('AI 生成失败：AI 返回空结果');
        return;
      }

      window.location.href = `/search?q=${encodeURIComponent(q)}`;
    } catch (err) {
      // 网络或其他异常
      console.error('AI 生成失败：', err);
    } finally {
      aiLoading = false;
    }
  }
</script>

<main class="flex min-h-[100svh] justify-center">
  <div class="w-210 px-6 pt-28 sm:pt-36 md:pt-44">
    <div class="mx-auto w-full text-center">
      <label
        class="mb-4 block text-6xl font-extrabold tracking-[-0.25em] italic antialiased select-none md:mb-10 md:text-8xl"
        for="search"
        id="logo-label"
      >
        <span class="text-blue-600">L</span>
        <span class="text-red-600">o</span>
        <span class="text-yellow-500">g</span>
        <span class="text-green-600">s</span>
        <span class="text-blue-600">e</span>
        <span class="text-red-600">e</span>
        <span class="text-yellow-500">k</span>
        <!--        <span class="text-green-600">e</span>-->
      </label>

      <!-- 输入框容器（相对定位），在左侧放置搜索图标；右侧是模式切换（AI/表达式） -->
      <form role="search" onsubmit={handleHomeSubmit}>
        <div class="relative">
          <!-- 搜索图标（仅装饰，不可交互） -->
          <span
            aria-hidden="true"
            class="pointer-events-none absolute inset-y-0 left-4 flex items-center text-gray-400"
          >
            <svg
              class="h-5 w-5"
              stroke="currentColor"
              stroke-linecap="round"
              stroke-linejoin="round"
              fill="none"
              stroke-width="2"
              viewBox="0 0 24 24"
              xmlns="http://www.w3.org/2000/svg"
            >
              <circle cx="11" cy="11" r="8"></circle>
              <line x1="21" x2="16.65" y1="21" y2="16.65"></line>
            </svg>
          </span>

          <input
            aria-labelledby="logo-label"
            bind:this={inputEl}
            class="w-full rounded-3xl border border-gray-200 bg-white py-4 pr-14 pl-12 text-sm shadow-sm transition outline-none placeholder:text-gray-500 focus:border-blue-200
                   focus:ring-4 focus:ring-blue-200 dark:border-gray-600 dark:bg-gray-800 dark:shadow-gray-600 dark:focus:border-gray-400 dark:focus:ring-gray-400"
            id="search"
            name="q"
            placeholder="Try: (taxResult OR taxWarn) /&quot;9111[0-9A-Z]{14}&quot;/ dt:20250818 path:ptcr -path:system.log"
            type="text"
          />

          <!-- 右侧模式切换按钮：默认 表达式；切换为“AI”时，回车将按自然语言生成查询串 -->
          <button
            type="button"
            class="absolute top-1/2 right-2 inline-flex h-8 w-8 -translate-y-1/2 items-center justify-center text-gray-600 hover:text-gray-800 active:scale-95 disabled:opacity-60 dark:text-gray-300 hover:dark:text-gray-100"
            title={aiMode ? 'AI 模式：回车将按自然语言生成查询串' : '表达式模式：回车将按查询串直接检索'}
            aria-label={aiMode ? 'AI 模式' : '表达式模式'}
            onclick={() => (aiMode = !aiMode)}
            disabled={aiLoading}
          >
            {#if aiLoading}
              <!-- 中文注释：简易加载指示器 -->
              <svg class="h-4 w-4 animate-spin text-gray-500" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v4a4 4 0 00-4 4H4z" />
              </svg>
            {:else if aiMode}
              <IconRobot size={24} stroke={2} aria-hidden="true" />
            {:else}
              <IconFunction size={24} stroke={2} aria-hidden="true" />
            {/if}
          </button>
        </div>
      </form>

      <div class="mt-2 flex flex-wrap items-center justify-center gap-1.5 text-xs text-gray-500 dark:text-gray-400">
        <span class="mr-1 select-none">语法提示：</span>
        <button
          class="rounded-full border border-gray-200 bg-gray-50 px-2 py-0.5 hover:bg-gray-100 dark:border-gray-800 dark:bg-gray-950 hover:dark:bg-gray-600"
          onclick={() => insertSnippet(' OR ')}
          title="逻辑或（必须大写）"
          type="button"
        >
          OR
        </button>
        <button
          class="rounded-full border border-gray-200 bg-gray-50 px-2 py-0.5 hover:bg-gray-100 dark:border-gray-800 dark:bg-gray-950 hover:dark:bg-gray-600"
          onclick={() => insertSnippet(' AND ')}
          title="逻辑与（必须大写）；相邻词默认 AND"
          type="button"
        >
          AND
        </button>
        <button
          class="rounded-full border border-gray-200 bg-gray-50 px-2 py-0.5 hover:bg-gray-100 dark:border-gray-800 dark:bg-gray-950 hover:dark:bg-gray-600"
          onclick={() => insertSnippet('-')}
          title="排除词，例如 -debug"
          type="button"
        >
          -exclude
        </button>
        <button
          class="rounded-full border border-gray-200 bg-gray-50 px-2 py-0.5 hover:bg-gray-100 dark:border-gray-800 dark:bg-gray-950 hover:dark:bg-gray-600"
          onclick={() => insertSnippet('""', 1)}
          title="短语匹配：插入一对引号"
          type="button"
        >
          "phrase"
        </button>
        <button
          class="rounded-full border border-gray-200 bg-gray-50 px-2 py-0.5 hover:bg-gray-100 dark:border-gray-800 dark:bg-gray-950 hover:dark:bg-gray-600"
          onclick={() => insertSnippet('//', 1)}
          title="正则匹配：插入 /.../"
          type="button"
        >
          /regex/
        </button>
        <button
          class="rounded-full border border-gray-200 bg-gray-50 px-2 py-0.5 hover:bg-gray-100 dark:border-gray-800 dark:bg-gray-950 hover:dark:bg-gray-600"
          onclick={() => insertSnippet('path:logs/*.log ')}
          title="路径限定（glob），示例 path:logs/*.log"
          type="button">path:glob</button
        >
        <button
          class="ml-2 underline underline-offset-2 hover:text-gray-200"
          onclick={() =>
            insertSnippet('(taxResult OR taxWarn) /"9111[0-9A-Z]{14}"/ dt:20250818 path:ptcr -path:system.log')}
          title="插入完整示例"
          type="button"
        >
          示例
        </button>
      </div>
    </div>
  </div>
</main>
