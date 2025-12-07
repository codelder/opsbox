<script lang="ts">
  /**
   * 首页（重构版）
   * 使用 LogSeek 模块的 API 客户端
   */
  import { goto } from '$app/navigation';
  import { convertNaturalLanguage } from '$lib/modules/logseek';
  import LogSeekLogo from '$lib/components/LogSeekLogo.svelte';
  import SyntaxHints from '$lib/components/SyntaxHints.svelte';
  import AiModeIcon from '$lib/components/AiModeIcon.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import { Search } from 'lucide-svelte';

  // 工具函数：将片段插入到输入框光标位置
  let inputEl: HTMLInputElement | null = null;

  // AI 加载状态（点击 AI 按钮时使用）；不再持久切换模式
  let aiLoading = $state(false);
  // 按压态，用于提供“按下”视觉反馈
  let pressing = $state(false);

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

  // 提交：始终按“普通模式”直接检索
  async function handleHomeSubmit(e: Event) {
    e.preventDefault();
    const text = (inputEl?.value || '').trim();
    if (!text || aiLoading) return;
    // 表达式模式：直接跳到 /search?q=
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    goto(`/search?q=${encodeURIComponent(text)}`);
  }

  // 点击右侧“AI 模式”按钮：临时使用 AI 把自然语言转查询后再检索
  async function handleAiClick() {
    const text = (inputEl?.value || '').trim();
    if (!text || aiLoading) return;
    aiLoading = true;
    try {
      const query = await convertNaturalLanguage(text);
      // eslint-disable-next-line svelte/no-navigation-without-resolve
      goto(`/search?q=${encodeURIComponent(query)}`);
    } catch (err) {
      console.error('AI 生成失败：', err);
    } finally {
      aiLoading = false;
    }
  }
</script>

<main class="flex min-h-full justify-center bg-background text-foreground">
  <!-- 固定位置的设置和主题切换按钮 -->
  <div class="fixed top-3 left-3 z-50"><Settings /></div>
  <div class="fixed top-3 right-3 z-50"><ThemeToggle /></div>

  <div class="w-full max-w-6xl px-6 pt-28 sm:pt-36 md:pt-44">
    <div class="mx-auto w-full text-center">
      <div class="mb-8 block md:mb-12" id="logo-label">
        <LogSeekLogo size="large" asLabel htmlFor="search" />
      </div>

      <!-- 输入框容器 -->
      <form role="search" onsubmit={handleHomeSubmit}>
        <div class="relative flex items-center">
          <!-- 搜索图标 -->
          <span aria-hidden="true" class="pointer-events-none absolute left-4 z-10 text-muted-foreground">
            <Search class="h-5 w-5" />
          </span>

          <input
            aria-labelledby="logo-label"
            bind:this={inputEl}
            class="flex h-14 w-full rounded-full border border-input bg-background px-12 py-2 text-foreground shadow-sm ring-offset-background placeholder:text-placeholder focus:ring-2 focus:ring-ring focus:ring-offset-2 focus:outline-none disabled:cursor-not-allowed disabled:opacity-50"
            id="search"
            name="q"
            placeholder="试一下: (taxResult OR taxWarn) /&quot;9111[0-9A-Z]{14}&quot;/ dt:20250818 path:ptcr -path:system.log"
            type="text"
          />

          <!-- 右侧"AI 模式"按钮 -->
          <div class="absolute right-2 z-20">
            <button
              type="button"
              class="group/ai inline-flex items-center rounded-full bg-secondary px-3 py-1.5 text-xs font-medium text-secondary-foreground transition-all hover:bg-secondary/80 focus:outline-none"
              title="按下使用 AI 模式；直接回车为普通模式"
              aria-label="AI 模式按钮"
              aria-pressed={aiLoading || pressing}
              onmousedown={() => (pressing = true)}
              onmouseup={() => (pressing = false)}
              onmouseleave={() => (pressing = false)}
              onclick={handleAiClick}
              disabled={aiLoading}
            >
              <!-- 彩虹流动边框（悬停或 loading 时显示） -->
              <span
                class="pointer-events-none absolute z-0 rounded-full transition-opacity duration-300
                {aiLoading || pressing ? 'opacity-100' : 'opacity-0 group-hover/ai:opacity-100'}"
                style="inset:-2px;padding:2px;-webkit-mask:linear-gradient(#fff 0 0) content-box,linear-gradient(#fff 0 0);-webkit-mask-composite:xor;mask-composite:exclude;"
                aria-hidden="true"
              >
                <span
                  class="absolute inset-0 rounded-full {aiLoading || pressing ? 'ai-rainbow-ring' : ''}"
                  style="background:conic-gradient(from 0deg, #60a5fa 0deg, #a78bfa 72deg, #f472b6 144deg, #f59e0b 216deg, #34d399 288deg, #60a5fa 360deg);"
                ></span>
              </span>

              <span class="relative z-10 inline-flex items-center gap-1.5">
                <AiModeIcon size={16} />
                <span>AI 模式</span>
              </span>
            </button>
          </div>
        </div>
      </form>

      <SyntaxHints onInsert={insertSnippet} />
    </div>
  </div>
</main>

<style>
  /* AI 按钮：彩虹流动边框动画（仅 loading 时旋转渐变背景） */
  .ai-rainbow-ring {
    animation: ai-ring-rotate 5s linear infinite;
    will-change: transform;
  }

  @keyframes ai-ring-rotate {
    to {
      transform: rotate(360deg);
    }
  }
</style>
