<script lang="ts">
  /**
   * S3 Profile 管理组件
   * 支持添加、编辑、删除多个 S3 配置
   */
  import { useProfiles } from '$lib/modules/logseek';
  import type { S3ProfilePayload } from '$lib/modules/logseek';
  import Alert from '$lib/components/Alert.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from '$lib/components/ui/card';
  import { Plus, Trash2, Edit2, Database, Cloud } from 'lucide-svelte';
  import { Badge } from '$lib/components/ui/badge';

  const profileStore = useProfiles();

  // 编辑表单状态
  let editingProfile = $state<S3ProfilePayload | null>(null);
  let isEditing = $state(false);
  let profileName = $state('');
  let endpoint = $state('');
  let bucket = $state('');
  let accessKey = $state('');
  let secretKey = $state('');

  // 初始化加载
  let profilesInit = $state(false);
  $effect(() => {
    if (profilesInit) return;
    profilesInit = true;
    profileStore.loadProfiles();
  });

  // 打开新建 Profile 表单
  function startNewProfile() {
    isEditing = true;
    editingProfile = null;
    profileName = '';
    endpoint = '';
    bucket = '';
    accessKey = '';
    secretKey = '';
    profileStore.clearSaveState();
  }

  // 打开编辑 Profile 表单
  function startEditProfile(profile: S3ProfilePayload) {
    isEditing = true;
    editingProfile = profile;
    profileName = profile.profile_name;
    endpoint = profile.endpoint;
    bucket = profile.bucket;
    accessKey = profile.access_key;
    secretKey = profile.secret_key;
    profileStore.clearSaveState();
  }

  // 取消编辑
  function cancelEdit() {
    isEditing = false;
    editingProfile = null;
    profileName = '';
    endpoint = '';
    bucket = '';
    accessKey = '';
    secretKey = '';
    profileStore.clearSaveState();
  }

  // 保存 Profile
  async function handleSave(e: Event) {
    e.preventDefault();
    const success = await profileStore.save({
      profile_name: profileName.trim(),
      endpoint: endpoint.trim(),
      bucket: bucket.trim(),
      access_key: accessKey.trim(),
      secret_key: secretKey.trim()
    });

    if (success) {
      cancelEdit();
    }
  }

  // 删除 Profile
  async function handleDelete(name: string) {
    if (!confirm(`确认删除 Profile "${name}"？`)) return;

    profileStore.clearDeleteState();
    const success = await profileStore.remove(name);
    if (!success && profileStore.deleteError) {
      alert(profileStore.deleteError);
    }
  }
</script>

<div class="space-y-6">
  {#if profileStore.error}
    <Alert type="error" message={profileStore.error} />
  {/if}

  {#if profileStore.saveError}
    <Alert type="error" message={profileStore.saveError} />
  {/if}

  {#if profileStore.saveSuccess}
    <Alert type="success" message="Profile 已保存" />
  {/if}

  <!-- Profile 列表 -->
  {#if !isEditing}
    <Card>
      <CardHeader class="flex flex-row items-center justify-between space-y-0 pb-2">
        <div class="space-y-1">
          <CardTitle>S3 Profile 配置</CardTitle>
          <CardDescription>管理多个 S3 对象存储连接配置</CardDescription>
        </div>
        <Button onclick={startNewProfile} size="sm">
          <Plus class="mr-2 h-4 w-4" />
          新建 Profile
        </Button>
      </CardHeader>
      <CardContent>
        {#if profileStore.loading}
          <div class="py-8 text-center text-sm text-muted-foreground">加载中…</div>
        {:else if profileStore.profiles.length === 0}
          <div class="flex flex-col items-center justify-center rounded-lg border border-dashed py-12 text-center">
            <Database class="h-10 w-10 text-muted-foreground/50" />
            <p class="mt-4 text-sm text-muted-foreground">暂无配置，点击"新建 Profile"添加</p>
          </div>
        {:else}
          <div class="grid gap-4">
            {#each profileStore.profiles as profile (profile.profile_name)}
              <div class="flex items-center justify-between rounded-lg border p-4 transition-colors hover:bg-muted/50">
                <div class="grid gap-1">
                  <div class="flex items-center gap-2">
                    <span class="font-semibold">{profile.profile_name}</span>
                    {#if profile.profile_name === 'default'}
                      <Badge variant="secondary" class="text-xs">默认</Badge>
                    {/if}
                  </div>
                  <div class="flex items-center text-sm text-muted-foreground">
                    <Cloud class="mr-1 h-3 w-3" />
                    {profile.endpoint} / {profile.bucket}
                  </div>
                </div>
                <div class="flex items-center gap-2">
                  <Button variant="ghost" size="icon" onclick={() => startEditProfile(profile)}>
                    <Edit2 class="h-4 w-4" />
                    <span class="sr-only">编辑</span>
                  </Button>
                  {#if profile.profile_name !== 'default'}
                    <Button
                      variant="ghost"
                      size="icon"
                      class="text-destructive hover:bg-destructive/10 hover:text-destructive"
                      onclick={() => handleDelete(profile.profile_name)}
                      disabled={profileStore.deleting}
                    >
                      <Trash2 class="h-4 w-4" />
                      <span class="sr-only">删除</span>
                    </Button>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </CardContent>
    </Card>
  {:else}
    <!-- 编辑表单 -->
    <Card>
      <CardHeader>
        <CardTitle>{editingProfile ? `编辑 Profile: ${editingProfile.profile_name}` : '新建 Profile'}</CardTitle>
      </CardHeader>
      <form onsubmit={handleSave}>
        <CardContent class="space-y-4">
          <div class="grid gap-2">
            <Label for="profile-name">Profile 名称</Label>
            <Input
              id="profile-name"
              type="text"
              bind:value={profileName}
              placeholder="例如：production"
              disabled={!!editingProfile || profileStore.saving}
              required
            />
            <p class="text-xs text-muted-foreground">
              {editingProfile ? 'Profile 名称不可修改' : '用于标识此配置，例如 production、staging'}
            </p>
          </div>

          <div class="grid gap-2">
            <Label for="profile-endpoint">Endpoint</Label>
            <Input
              id="profile-endpoint"
              type="text"
              bind:value={endpoint}
              placeholder="http://host:9000"
              disabled={profileStore.saving}
              required
            />
          </div>

          <div class="grid gap-2">
            <Label for="profile-bucket">Bucket</Label>
            <Input
              id="profile-bucket"
              type="text"
              bind:value={bucket}
              placeholder="bucket"
              disabled={profileStore.saving}
              required
            />
            <p class="text-xs text-muted-foreground">指定要访问的 S3 存储桶名称</p>
          </div>

          <div class="grid gap-2">
            <Label for="profile-access-key">Access Key</Label>
            <Input
              id="profile-access-key"
              type="text"
              bind:value={accessKey}
              placeholder="access key"
              autocomplete="off"
              disabled={profileStore.saving}
              required
            />
          </div>

          <div class="grid gap-2">
            <Label for="profile-secret-key">Secret Key</Label>
            <Input
              id="profile-secret-key"
              type="password"
              bind:value={secretKey}
              placeholder="secret key"
              autocomplete="off"
              disabled={profileStore.saving}
              required
            />
          </div>
        </CardContent>
        <CardFooter class="flex justify-end gap-2">
          <Button variant="outline" type="button" onclick={cancelEdit} disabled={profileStore.saving}>取消</Button>
          <Button
            type="submit"
            disabled={profileStore.saving ||
              !profileName.trim() ||
              !endpoint.trim() ||
              !bucket.trim() ||
              !accessKey.trim() ||
              !secretKey.trim()}
          >
            {#if profileStore.saving}
              保存中…
            {:else}
              保存 Profile
            {/if}
          </Button>
        </CardFooter>
      </form>
    </Card>
  {/if}
</div>
