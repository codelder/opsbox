<script lang="ts">
  /**
   * S3 Profile 管理组件
   * 支持添加、编辑、删除多个 S3 配置
   */
  import { useProfiles } from '$lib/modules/logseek';
  import type { S3ProfilePayload } from '$lib/modules/logseek';
  import Alert from '$lib/components/Alert.svelte';

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
    <section
      class="rounded-3xl border border-slate-200 bg-white shadow-lg shadow-slate-200/40 dark:border-slate-800 dark:bg-slate-900 dark:shadow-black/30"
    >
      <div class="flex items-center justify-between border-b border-slate-200 p-6 dark:border-slate-800">
        <div>
          <h2 class="text-lg font-semibold text-slate-900 dark:text-slate-100">S3 Profile 配置</h2>
          <p class="mt-1 text-sm text-slate-500 dark:text-slate-400">管理多个 S3 对象存储连接配置</p>
        </div>
        <button
          type="button"
          class="inline-flex items-center rounded-xl bg-indigo-600 px-4 py-2 text-sm font-semibold text-white shadow-sm transition hover:bg-indigo-500 focus:ring-4 focus:ring-indigo-200 focus:outline-none dark:bg-indigo-500 dark:hover:bg-indigo-400 dark:focus:ring-indigo-500/40"
          onclick={startNewProfile}
        >
          <svg class="mr-2 h-4 w-4" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
          </svg>
          新建 Profile
        </button>
      </div>

      <div class="p-6">
        {#if profileStore.loading}
          <div class="text-center text-sm text-slate-500 dark:text-slate-400">加载中…</div>
        {:else if profileStore.profiles.length === 0}
          <div
            class="rounded-xl border border-dashed border-slate-200 bg-slate-50 px-4 py-8 text-center dark:border-slate-700 dark:bg-slate-900/50"
          >
            <svg class="mx-auto h-12 w-12 text-slate-400" viewBox="0 0 24 24" stroke="currentColor">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4"
              />
            </svg>
            <p class="mt-4 text-sm text-slate-600 dark:text-slate-400">暂无配置，点击"新建 Profile"添加</p>
          </div>
        {:else}
          <div class="space-y-3">
            {#each profileStore.profiles as profile (profile.profile_name)}
              <div
                class="flex items-center justify-between rounded-xl border border-slate-200 bg-slate-50 p-4 transition hover:bg-slate-100 dark:border-slate-700 dark:bg-slate-800/50 dark:hover:bg-slate-800"
              >
                <div class="flex-1">
                  <div class="flex items-center gap-2">
                    <h3 class="font-semibold text-slate-900 dark:text-slate-100">{profile.profile_name}</h3>
                    {#if profile.profile_name === 'default'}
                      <span
                        class="inline-flex items-center rounded-full bg-blue-100 px-2 py-0.5 text-xs font-medium text-blue-800 dark:bg-blue-900/30 dark:text-blue-300"
                      >
                        默认
                      </span>
                    {/if}
                  </div>
                  <p class="mt-1 text-sm text-slate-600 dark:text-slate-400">{profile.endpoint} / {profile.bucket}</p>
                </div>
                <div class="flex items-center gap-2">
                  <button
                    type="button"
                    class="rounded-lg px-3 py-1.5 text-sm font-medium text-slate-600 transition hover:bg-slate-200 hover:text-slate-900 dark:text-slate-300 dark:hover:bg-slate-700 dark:hover:text-slate-100"
                    onclick={() => startEditProfile(profile)}
                  >
                    编辑
                  </button>
                  {#if profile.profile_name !== 'default'}
                    <button
                      type="button"
                      class="rounded-lg px-3 py-1.5 text-sm font-medium text-red-600 transition hover:bg-red-100 hover:text-red-700 dark:text-red-400 dark:hover:bg-red-900/30 dark:hover:text-red-300"
                      onclick={() => handleDelete(profile.profile_name)}
                      disabled={profileStore.deleting}
                    >
                      删除
                    </button>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </section>
  {:else}
    <!-- 编辑表单 -->
    <section
      class="rounded-3xl border border-slate-200 bg-white shadow-lg shadow-slate-200/40 dark:border-slate-800 dark:bg-slate-900 dark:shadow-black/30"
    >
      <div class="border-b border-slate-200 p-6 dark:border-slate-800">
        <h2 class="text-lg font-semibold text-slate-900 dark:text-slate-100">
          {editingProfile ? `编辑 Profile: ${editingProfile.profile_name}` : '新建 Profile'}
        </h2>
      </div>

      <form class="space-y-6 p-6" onsubmit={handleSave}>
        <div>
          <label for="profile-name" class="block text-sm font-medium text-slate-700 dark:text-slate-300">
            Profile 名称
          </label>
          <input
            id="profile-name"
            type="text"
            class="mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm placeholder-slate-400 shadow-sm transition focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 focus:outline-none dark:border-slate-600 dark:bg-slate-800 dark:text-white dark:placeholder-slate-500"
            bind:value={profileName}
            placeholder="例如：production"
            disabled={!!editingProfile || profileStore.saving}
            required
          />
          <p class="mt-1 text-xs text-slate-500 dark:text-slate-400">
            {editingProfile ? 'Profile 名称不可修改' : '用于标识此配置，例如 production、staging'}
          </p>
        </div>

        <div>
          <label for="profile-endpoint" class="block text-sm font-medium text-slate-700 dark:text-slate-300">
            Endpoint
          </label>
          <input
            id="profile-endpoint"
            type="text"
            class="mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm placeholder-slate-400 shadow-sm transition focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 focus:outline-none dark:border-slate-600 dark:bg-slate-800 dark:text-white dark:placeholder-slate-500"
            bind:value={endpoint}
            placeholder="http://host:9000"
            disabled={profileStore.saving}
            required
          />
        </div>

        <div>
          <label for="profile-bucket" class="block text-sm font-medium text-slate-700 dark:text-slate-300">
            Bucket
          </label>
          <input
            id="profile-bucket"
            type="text"
            class="mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm placeholder-slate-400 shadow-sm transition focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 focus:outline-none dark:border-slate-600 dark:bg-slate-800 dark:text-white dark:placeholder-slate-500"
            bind:value={bucket}
            placeholder="bucket"
            disabled={profileStore.saving}
            required
          />
          <p class="mt-1 text-xs text-slate-500 dark:text-slate-400">指定要访问的 S3 存储桶名称</p>
        </div>

        <div>
          <label for="profile-access-key" class="block text-sm font-medium text-slate-700 dark:text-slate-300">
            Access Key
          </label>
          <input
            id="profile-access-key"
            type="text"
            class="mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm placeholder-slate-400 shadow-sm transition focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 focus:outline-none dark:border-slate-600 dark:bg-slate-800 dark:text-white dark:placeholder-slate-500"
            bind:value={accessKey}
            placeholder="access key"
            autocomplete="off"
            disabled={profileStore.saving}
            required
          />
        </div>

        <div>
          <label for="profile-secret-key" class="block text-sm font-medium text-slate-700 dark:text-slate-300">
            Secret Key
          </label>
          <input
            id="profile-secret-key"
            type="password"
            class="mt-1 block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm placeholder-slate-400 shadow-sm transition focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 focus:outline-none dark:border-slate-600 dark:bg-slate-800 dark:text-white dark:placeholder-slate-500"
            bind:value={secretKey}
            placeholder="secret key"
            autocomplete="off"
            disabled={profileStore.saving}
            required
          />
        </div>

        <div class="flex justify-end gap-3 border-t border-slate-200 pt-6 dark:border-slate-800">
          <button
            type="button"
            class="inline-flex items-center rounded-xl border border-transparent px-4 py-2 text-sm font-medium text-slate-500 transition hover:text-slate-700 focus:ring-2 focus:ring-slate-300 focus:outline-none dark:text-slate-300 dark:hover:text-slate-100"
            onclick={cancelEdit}
            disabled={profileStore.saving}
          >
            取消
          </button>

          <button
            type="submit"
            class="inline-flex items-center justify-center rounded-xl bg-indigo-600 px-5 py-2 text-sm font-semibold text-white shadow-sm transition hover:bg-indigo-500 focus:ring-4 focus:ring-indigo-200 focus:outline-none disabled:cursor-not-allowed disabled:bg-indigo-300 dark:bg-indigo-500 dark:hover:bg-indigo-400 dark:focus:ring-indigo-500/40"
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
          </button>
        </div>
      </form>
    </section>
  {/if}
</div>
