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
  import ServerLogSettings from './ServerLogSettings.svelte';

  import { Tabs, TabsContent, TabsList, TabsTrigger } from '$lib/components/ui/tabs';
  import { resolve } from '$app/paths';
  import LogSeekLogo from '$lib/components/LogSeekLogo.svelte';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';

  // 选项卡状态
  let activeTab = $state<'profiles' | 'agents' | 'planners' | 'llm' | 'server-log'>('profiles');
</script>

<svelte:head>
  <title>对象存储设置 · Opsboard</title>
</svelte:head>

<div class="flex min-h-screen flex-col bg-background text-foreground">
  <!-- 顶部导航栏 -->
  <header
    class="sticky top-0 z-50 w-full border-b border-border bg-background/95 backdrop-blur supports-backdrop-filter:bg-background/60"
  >
    <div class="flex h-16 w-full items-center gap-4 px-6">
      <!-- 左侧：Logo -->
      <a href={resolve('/')} class="flex items-center gap-2 transition-opacity hover:opacity-80">
        <LogSeekLogo size="small" />
      </a>

      <!-- 中间：标题 -->
      <div class="flex-1 px-4">
        <h1 class="text-lg font-semibold tracking-tight">系统设置</h1>
      </div>

      <!-- 右侧：操作区 -->
      <div class="flex items-center gap-2">
        <ThemeToggle />
      </div>
    </div>
  </header>

  <div class="mx-auto flex w-full max-w-5xl flex-col gap-6 px-6 py-8">
    <Tabs bind:value={activeTab} class="w-full">
      <TabsList class="mx-auto grid w-full grid-cols-5 lg:w-[600px]">
        <TabsTrigger value="profiles">对象存储</TabsTrigger>
        <TabsTrigger value="agents">Agent</TabsTrigger>
        <TabsTrigger value="planners">规划脚本</TabsTrigger>
        <TabsTrigger value="llm">大模型</TabsTrigger>
        <TabsTrigger value="server-log">Server 日志</TabsTrigger>
      </TabsList>

      <div class="mt-6">
        <TabsContent value="profiles">
          <ProfileManagement />
        </TabsContent>
        <TabsContent value="agents">
          <AgentManagement />
        </TabsContent>
        <TabsContent value="planners">
          <PlannerManagement />
        </TabsContent>
        <TabsContent value="llm">
          <LlmManagement />
        </TabsContent>
        <TabsContent value="server-log">
          <ServerLogSettings />
        </TabsContent>
      </div>
    </Tabs>
  </div>
</div>
