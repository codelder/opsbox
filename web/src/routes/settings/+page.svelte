<script lang="ts">
  /**
   * S3 对象存储设置页面
   * 使用 Profile 管理统一管理多个 S3 配置
   * 支持 AWS S3、MinIO、阿里云 OSS 等 S3 兼容存储
   */
  import ProfileManagement from './ProfileManagement.svelte';
  import AgentManagement from './AgentManagement.svelte';
  import LlmManagement from './LlmManagement.svelte';

  // 选项卡状态（保留以便未来扩展）
  let activeTab = $state<'profiles' | 'agents' | 'llm' | 'alerts' | 'notifications' | 'team'>('profiles');
</script>

<svelte:head>
  <title>对象存储设置 · Opsboard</title>
</svelte:head>

<div class="mx-auto flex max-w-5xl flex-col gap-6 px-6 pb-16 text-slate-900 dark:text-slate-100">
  <header class="pt-6">
    <p class="text-xs font-semibold tracking-[0.2em] text-slate-500 uppercase dark:text-slate-400">Settings</p>
    <h1 class="mt-2 text-2xl font-semibold text-slate-900 dark:text-slate-50">系统设置</h1>
    <p class="mt-2 text-sm text-slate-500 dark:text-slate-400">管理 S3 对象存储配置、告警通知和团队设置。</p>
  </header>

  <nav
    class="flex items-center gap-6 border-b border-slate-200 pb-3 text-sm font-medium text-slate-500 dark:border-slate-800 dark:text-slate-400"
  >
    <button
      type="button"
      class="rounded-full px-3 py-1 transition {activeTab === 'profiles'
        ? 'bg-white text-slate-900 shadow-sm dark:bg-slate-900 dark:text-slate-100'
        : 'hover:text-slate-700 dark:hover:text-slate-300'}"
      onclick={() => (activeTab = 'profiles')}
    >
      对象存储
    </button>
    <button
      type="button"
      class="rounded-full px-3 py-1 transition {activeTab === 'agents'
        ? 'bg-white text-slate-900 shadow-sm dark:bg-slate-900 dark:text-slate-100'
        : 'hover:text-slate-700 dark:hover:text-slate-300'}"
      onclick={() => (activeTab = 'agents')}
    >
      Agent
    </button>
    <button
      type="button"
      class="rounded-full px-3 py-1 transition {activeTab === 'llm'
        ? 'bg-white text-slate-900 shadow-sm dark:bg-slate-900 dark:text-slate-100'
        : 'hover:text-slate-700 dark:hover:text-slate-300'}"
      onclick={() => (activeTab = 'llm')}
    >
      大模型
    </button>
    <button
      type="button"
      class="rounded-full px-3 py-1 transition {activeTab === 'alerts'
        ? 'bg-white text-slate-900 shadow-sm dark:bg-slate-900 dark:text-slate-100'
        : 'hover:text-slate-700 dark:hover:text-slate-300'}"
      onclick={() => (activeTab = 'alerts')}
      disabled
    >
      告警
    </button>
    <button
      type="button"
      class="rounded-full px-3 py-1 transition {activeTab === 'notifications'
        ? 'bg-white text-slate-900 shadow-sm dark:bg-slate-900 dark:text-slate-100'
        : 'hover:text-slate-700 dark:hover:text-slate-300'}"
      onclick={() => (activeTab = 'notifications')}
      disabled
    >
      通知
    </button>
    <button
      type="button"
      class="rounded-full px-3 py-1 transition {activeTab === 'team'
        ? 'bg-white text-slate-900 shadow-sm dark:bg-slate-900 dark:text-slate-100'
        : 'hover:text-slate-700 dark:hover:text-slate-300'}"
      onclick={() => (activeTab = 'team')}
      disabled
    >
      团队
    </button>
  </nav>

  {#if activeTab === 'profiles'}
    <ProfileManagement />
  {:else if activeTab === 'agents'}
    <AgentManagement />
  {:else if activeTab === 'llm'}
    <LlmManagement />
  {:else if activeTab === 'alerts'}
    <div
      class="rounded-3xl border border-dashed border-slate-200 bg-white/60 px-8 py-16 text-center dark:border-slate-700 dark:bg-slate-900/30"
    >
      <svg class="mx-auto h-16 w-16 text-slate-400" viewBox="0 0 24 24" stroke="currentColor">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"
        />
      </svg>
      <p class="mt-4 text-sm text-slate-600 dark:text-slate-400">告警功能即将推出</p>
    </div>
  {:else if activeTab === 'notifications'}
    <div
      class="rounded-3xl border border-dashed border-slate-200 bg-white/60 px-8 py-16 text-center dark:border-slate-700 dark:bg-slate-900/30"
    >
      <svg class="mx-auto h-16 w-16 text-slate-400" viewBox="0 0 24 24" stroke="currentColor">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M3 8l7.89 5.26a2 2 0 002.22 0L21 8M5 19h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"
        />
      </svg>
      <p class="mt-4 text-sm text-slate-600 dark:text-slate-400">通知功能即将推出</p>
    </div>
  {:else if activeTab === 'team'}
    <div
      class="rounded-3xl border border-dashed border-slate-200 bg-white/60 px-8 py-16 text-center dark:border-slate-700 dark:bg-slate-900/30"
    >
      <svg class="mx-auto h-16 w-16 text-slate-400" viewBox="0 0 24 24" stroke="currentColor">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"
        />
      </svg>
      <p class="mt-4 text-sm text-slate-600 dark:text-slate-400">团队管理功能即将推出</p>
    </div>
  {/if}
</div>
