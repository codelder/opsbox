<script lang="ts">
  // 工具函数：将片段插入到输入框光标位置
  let inputEl: HTMLInputElement | null = null;

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
</script>

<main class="flex min-h-[100svh] justify-center">
  <div class="w-200 px-6 pt-28 sm:pt-36 md:pt-44">
    <div class="mx-auto w-full text-center">
      <label
        class="mb-4 block text-6xl font-extrabold tracking-[-0.25em] italic antialiased select-none md:mb-10 md:text-8xl"
        for="search"
        id="logo-label"
      >
        <span class="text-blue-600">L</span>
        <span class="text-red-600">o</span>
        <span class="text-yellow-500">G</span>
        <span class="text-green-600">o</span>
        <span class="text-blue-600">o</span>
        <span class="text-red-600">g</span>
        <span class="text-yellow-500">l</span>
        <span class="text-green-600">e</span>
      </label>

      <!-- 输入框容器（相对定位），在左侧放置搜索图标 -->
      <form action="/search" method="GET" role="search">
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
              stroke-width="2"
              viewBox="0 0 24 24"
              xmlns="http://www.w3.org/2000/svg"
            >
              <circle cx="11" cy="11" fill="none" r="8"></circle>
              <line x1="21" x2="16.65" y1="21" y2="16.65"></line>
            </svg>
          </span>

          <input
            aria-labelledby="logo-label"
            bind:this={inputEl}
            class="w-full rounded-3xl border border-gray-200 bg-white py-4 pr-6 pl-12 shadow-sm transition outline-none placeholder:text-gray-500 focus:border-blue-200 focus:ring-4
                   focus:ring-blue-200 dark:border-gray-600 dark:bg-gray-800 dark:shadow-gray-600 dark:focus:border-gray-400 dark:focus:ring-gray-400"
            id="search"
            name="q"
            placeholder="Try: &quot;connection reset&quot; OR /JUMPERR\\d+/ -repeater path:**/trace/*.log"
            type="text"
          />
        </div>
      </form>

      <div class="mt-2 flex flex-wrap items-center justify-center gap-1.5 text-xs text-gray-500 dark:text-gray-400">
        <span class="mr-1 select-none">语法提示：</span>
        <button
          class="rounded-full border border-gray-200 bg-gray-50 px-2 py-0.5 hover:bg-gray-100 dark:border-gray-800 dark:bg-gray-950 hover:dark:bg-gray-600"
          on:click={() => insertSnippet(' OR ')}
          title="逻辑或（必须大写）"
          type="button"
        >
          OR
        </button>
        <button
          class="rounded-full border border-gray-200 bg-gray-50 px-2 py-0.5 hover:bg-gray-100 dark:border-gray-800 dark:bg-gray-950 hover:dark:bg-gray-600"
          on:click={() => insertSnippet(' AND ')}
          title="逻辑与（必须大写）；相邻词默认 AND"
          type="button"
        >
          AND
        </button>
        <button
          class="rounded-full border border-gray-200 bg-gray-50 px-2 py-0.5 hover:bg-gray-100 dark:border-gray-800 dark:bg-gray-950 hover:dark:bg-gray-600"
          on:click={() => insertSnippet('-')}
          title="排除词，例如 -debug"
          type="button"
        >
          -exclude
        </button>
        <button
          class="rounded-full border border-gray-200 bg-gray-50 px-2 py-0.5 hover:bg-gray-100 dark:border-gray-800 dark:bg-gray-950 hover:dark:bg-gray-600"
          on:click={() => insertSnippet('""', 1)}
          title="短语匹配：插入一对引号"
          type="button"
        >
          "phrase"
        </button>
        <button
          class="rounded-full border border-gray-200 bg-gray-50 px-2 py-0.5 hover:bg-gray-100 dark:border-gray-800 dark:bg-gray-950 hover:dark:bg-gray-600"
          on:click={() => insertSnippet('//', 1)}
          title="正则匹配：插入 /.../"
          type="button"
        >
          /regex/
        </button>
        <button
          class="rounded-full border border-gray-200 bg-gray-50 px-2 py-0.5 hover:bg-gray-100 dark:border-gray-800 dark:bg-gray-950 hover:dark:bg-gray-600"
          on:click={() => insertSnippet('path:logs/*.log ')}
          title="路径限定（glob），示例 path:logs/*.log"
          type="button">path:glob</button
        >
        <button
          class="ml-2 underline underline-offset-2 hover:text-gray-200"
          on:click={() => insertSnippet('"connection reset" OR /ERR\\d+/ -debug path:logs/*.log')}
          title="插入完整示例"
          type="button"
        >
          示例
        </button>
      </div>
    </div>
  </div>
</main>
