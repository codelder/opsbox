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

<main class="min-h-[100svh] flex justify-center bg-white">
  <div class="w-200 px-6 pt-28 sm:pt-36 md:pt-44">
    <div class="mx-auto w-full text-center">
      <label for="search" id="logo-label" class="mb-4 md:mb-10 block select-none text-6xl md:text-8xl tracking-[-0.25em] font-extrabold italic">
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
      <div class="relative">
        <!-- 搜索图标（仅装饰，不可交互） -->
        <span class="pointer-events-none absolute inset-y-0 left-4 flex items-center text-neutral-400" aria-hidden="true">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="h-5 w-5">
            <circle cx="11" cy="11" r="8"></circle>
            <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
          </svg>
        </span>

        <input
          bind:this={inputEl}
          id="search"
          type="text"
          placeholder='Try: "connection reset" OR /JUMPERR\d+/ -repeater path:**/trace/*.log'
          aria-labelledby="logo-label"
          autofocus
          class="w-full rounded-3xl border border-neutral-200 bg-white pl-12 pr-6 py-4 shadow-sm outline-none transition
                 focus:ring-4 focus:ring-blue-200 focus:border-blue-400 placeholder:text-neutral-400"
        />
      </div>

      <div class="mt-6 flex flex-wrap items-center justify-center gap-1.5 text-xs text-neutral-500">
        <span class="mr-1 select-none">语法提示：</span>
        <button
          type="button"
          class="rounded-full border border-neutral-200 bg-neutral-50 px-2 py-0.5 hover:bg-neutral-100"
          title="逻辑或（必须大写）"
          on:click={() => insertSnippet(' OR ')}
        >OR</button>
        <button
          type="button"
          class="rounded-full border border-neutral-200 bg-neutral-50 px-2 py-0.5 hover:bg-neutral-100"
          title="逻辑与（必须大写）；相邻词默认 AND"
          on:click={() => insertSnippet(' AND ')}
        >AND</button>
        <button
          type="button"
          class="rounded-full border border-neutral-200 bg-neutral-50 px-2 py-0.5 hover:bg-neutral-100"
          title="排除词，例如 -debug"
          on:click={() => insertSnippet('-')}
        >-exclude</button>
        <button
          type="button"
          class="rounded-full border border-neutral-200 bg-neutral-50 px-2 py-0.5 hover:bg-neutral-100"
          title="短语匹配：插入一对引号"
          on:click={() => insertSnippet('""', 1)}
        >"phrase"</button>
        <button
          type="button"
          class="rounded-full border border-neutral-200 bg-neutral-50 px-2 py-0.5 hover:bg-neutral-100"
          title="正则匹配：插入 /.../"
          on:click={() => insertSnippet('//', 1)}
        >/regex/</button>
        <button
          type="button"
          class="rounded-full border border-neutral-200 bg-neutral-50 px-2 py-0.5 hover:bg-neutral-100"
          title="路径限定（glob），示例 path:logs/*.log"
          on:click={() => insertSnippet('path:logs/*.log ')}
        >path:glob</button>
        <button
          type="button"
          class="ml-2 text-neutral-600 underline underline-offset-2 hover:text-neutral-800"
          title="插入完整示例"
          on:click={() => insertSnippet('\"connection reset\" OR /ERR\\d+/ -debug path:logs/*.log')}
        >示例</button>
      </div>
    </div>
  </div>
</main>
