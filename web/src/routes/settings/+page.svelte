<script lang="ts">
  /**
   * S3 对象存储设置页面
   * 使用 Profile 管理统一管理多个 S3 配置
   * 支持 AWS S3、MinIO、阿里云 OSS 等 S3 兼容存储
   */
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import ProfileManagement from './ProfileManagement.svelte';
  import AgentManagement from './AgentManagement.svelte';
  import LlmManagement from './LlmManagement.svelte';
  import PlannerManagement from './PlannerManagement.svelte';
  import ServerLogSettings from './ServerLogSettings.svelte';

  // 选项卡状态
  let activeTab = $state<'profiles' | 'agents' | 'planners' | 'llm' | 'server-log'>('profiles');

  // 返回上一页，如果没有历史记录则返回首页
  function handleBack() {
    if (browser && window.history.length > 1) {
      // 有历史记录，返回上一页
      window.history.back();
    } else {
      // 没有历史记录，返回首页
      goto('/');
    }
  }
</script>

<svelte:head>
  <title>对象存储设置 · Opsboard</title>
</svelte:head>

<div class="mx-auto flex max-w-5xl flex-col gap-6 px-6 pb-16 text-[var(--text)]">
  <header class="pt-6">
    <div class="mb-4 flex items-center gap-3">
      <button
        type="button"
        onclick={handleBack}
        class="inline-flex items-center gap-2 rounded-lg px-3 py-2 text-sm font-medium text-[var(--muted)] transition-colors hover:bg-[var(--surface)] hover:text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 dark:focus:ring-offset-gray-900"
        aria-label="返回上一页"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          class="h-4 w-4"
        >
          <path d="M19 12H5M12 19l-7-7 7-7" />
        </svg>
        <span>返回</span>
      </button>
    </div>
    <p class="text-xs font-semibold tracking-[0.2em] text-[var(--muted)] uppercase">Settings</p>
    <h1 class="mt-2 text-2xl font-semibold text-[var(--text)]">系统设置</h1>
    <p class="mt-2 text-sm text-[var(--muted)]">管理对象存储、Agent、大模型与规划脚本。</p>
  </header>

  <nav class="flex items-center gap-6 border-b border-[var(--border)] pb-3 text-sm font-medium text-[var(--muted)]">
    <button
      type="button"
      class="rounded-full px-3 py-1 transition {activeTab === 'profiles'
        ? 'bg-[var(--surface)] text-[var(--text)] shadow-sm'
        : 'text-[var(--muted)] hover:text-[var(--text)]'}"
      onclick={() => (activeTab = 'profiles')}
    >
      对象存储
    </button>
    <button
      type="button"
      class="rounded-full px-3 py-1 transition {activeTab === 'agents'
        ? 'bg-[var(--surface)] text-[var(--text)] shadow-sm'
        : 'text-[var(--muted)] hover:text-[var(--text)]'}"
      onclick={() => (activeTab = 'agents')}
    >
      Agent
    </button>
    <button
      type="button"
      class="rounded-full px-3 py-1 transition {activeTab === 'planners'
        ? 'bg-[var(--surface)] text-[var(--text)] shadow-sm'
        : 'text-[var(--muted)] hover:text-[var(--text)]'}"
      onclick={() => (activeTab = 'planners')}
    >
      规划脚本
    </button>
    <button
      type="button"
      class="rounded-full px-3 py-1 transition {activeTab === 'llm'
        ? 'bg-[var(--surface)] text-[var(--text)] shadow-sm'
        : 'text-[var(--muted)] hover:text-[var(--text)]'}"
      onclick={() => (activeTab = 'llm')}
    >
      大模型
    </button>
    <button
      type="button"
      class="rounded-full px-3 py-1 transition {activeTab === 'server-log'
        ? 'bg-[var(--surface)] text-[var(--text)] shadow-sm'
        : 'text-[var(--muted)] hover:text-[var(--text)]'}"
      onclick={() => (activeTab = 'server-log')}
    >
      Server 日志
    </button>
  </nav>

  {#if activeTab === 'profiles'}
    <ProfileManagement />
  {:else if activeTab === 'agents'}
    <AgentManagement />
  {:else if activeTab === 'planners'}
    <PlannerManagement />
  {:else if activeTab === 'llm'}
    <LlmManagement />
  {:else if activeTab === 'server-log'}
    <ServerLogSettings />
  {/if}
</div>
