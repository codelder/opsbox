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

  import { Tabs, TabsContent, TabsList, TabsTrigger } from '$lib/components/ui/tabs';
  import { Button } from '$lib/components/ui/button';
  import { ArrowLeft } from 'lucide-svelte';

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

<div class="mx-auto flex max-w-5xl flex-col gap-6 px-6 pb-16 text-foreground">
  <header class="pt-6">
    <div class="mb-4 flex items-center gap-3">
      <Button
        variant="ghost"
        size="sm"
        onclick={handleBack}
        class="gap-2 pl-2 text-muted-foreground hover:text-foreground"
        aria-label="返回上一页"
      >
        <ArrowLeft class="h-4 w-4" />
        <span>返回</span>
      </Button>
    </div>
    <p class="text-xs font-semibold tracking-[0.2em] text-muted-foreground uppercase">Settings</p>
    <h1 class="mt-2 text-2xl font-semibold tracking-tight">系统设置</h1>
    <p class="mt-2 text-sm text-muted-foreground">管理对象存储、Agent、大模型与规划脚本。</p>
  </header>

  <Tabs bind:value={activeTab} class="w-full">
    <TabsList class="grid w-full grid-cols-5 lg:w-[600px]">
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
