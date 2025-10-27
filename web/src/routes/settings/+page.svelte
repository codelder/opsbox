<script lang="ts">
  /**
   * S3 对象存储设置页面
   * 使用 Profile 管理统一管理多个 S3 配置
   * 支持 AWS S3、MinIO、阿里云 OSS 等 S3 兼容存储
   */
  import ProfileManagement from './ProfileManagement.svelte';
  import AgentManagement from './AgentManagement.svelte';
  import LlmManagement from './LlmManagement.svelte';
  import PlannerManagement from './PlannerManagement.svelte';

  // 选项卡状态
  let activeTab = $state<'profiles' | 'agents' | 'planners' | 'llm'>('profiles');
</script>

<svelte:head>
  <title>对象存储设置 · Opsboard</title>
</svelte:head>

<div class="mx-auto flex max-w-5xl flex-col gap-6 px-6 pb-16 text-slate-900 dark:text-slate-100">
  <header class="pt-6">
    <p class="text-xs font-semibold tracking-[0.2em] text-slate-500 uppercase dark:text-slate-400">Settings</p>
    <h1 class="mt-2 text-2xl font-semibold text-slate-900 dark:text-slate-50">系统设置</h1>
    <p class="mt-2 text-sm text-slate-500 dark:text-slate-400">管理对象存储、Agent、大模型与规划脚本。</p>
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
      class="rounded-full px-3 py-1 transition {activeTab === 'planners'
        ? 'bg-white text-slate-900 shadow-sm dark:bg-slate-900 dark:text-slate-100'
        : 'hover:text-slate-700 dark:hover:text-slate-300'}"
      onclick={() => (activeTab = 'planners')}
    >
      规划脚本
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
  </nav>

  {#if activeTab === 'profiles'}
    <ProfileManagement />
  {:else if activeTab === 'agents'}
    <AgentManagement />
  {:else if activeTab === 'planners'}
    <PlannerManagement />
  {:else if activeTab === 'llm'}
    <LlmManagement />
  {/if}
</div>
