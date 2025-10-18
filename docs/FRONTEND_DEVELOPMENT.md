# 前端开发指南

本文档介绍如何使用 OpsBoard 前端的模块化架构进行开发。

## 📁 目录结构

```
ui/src/lib/modules/logseek/
├── types/           # TypeScript 类型定义
│   └── index.ts
├── api/             # API 客户端层
│   ├── config.ts    # API 配置
│   ├── search.ts    # 搜索 API
│   ├── settings.ts  # 设置 API
│   ├── nl2q.ts      # NL2Q API
│   ├── view.ts      # 文件查看 API
│   └── index.ts
├── utils/           # 工具函数
│   ├── highlight.ts # 文本处理
│   └── index.ts
├── composables/     # 可组合逻辑 (Svelte 5 Runes)
│   ├── useStreamReader.svelte.ts
│   ├── useSearch.svelte.ts
│   ├── useSettings.svelte.ts
│   └── index.ts
├── components/      # UI 组件（预留）
└── index.ts         # 模块统一入口
```

## 🎯 核心概念

### 1. 分层架构

- **types**: 类型定义，确保类型安全
- **api**: 封装所有后端 API 调用
- **utils**: 通用工具函数
- **composables**: Svelte 5 Runes 风格的状态管理
- **components**: 可复用的 UI 组件

### 2. 导入方式

统一从模块入口导入：

```typescript
import { 
  // Types
  type SearchJsonResult,
  type MinioSettingsPayload,
  
  // API Clients
  fetchMinioSettings,
  saveMinioSettings,
  startSearch,
  
  // Utils
  highlight,
  snippet,
  
  // Composables
  useSettings,
  useSearch
} from '$lib/modules/logseek';
```

## 📝 使用指南

### 1. 使用 API 客户端

API 客户端提供类型安全的后端调用。

#### 示例：获取和保存 MinIO 设置

```typescript
import { fetchMinioSettings, saveMinioSettings } from '$lib/modules/logseek';

// 获取设置
try {
  const settings = await fetchMinioSettings();
  console.log(settings.endpoint, settings.bucket);
} catch (error) {
  console.error('加载设置失败：', error);
}

// 保存设置
try {
  await saveMinioSettings({
    endpoint: 'http://localhost:9000',
    bucket: 'logs',
    access_key: 'minioadmin',
    secret_key: 'minioadmin'
  });
  console.log('保存成功');
} catch (error) {
  console.error('保存失败：', error.message);
}
```

#### 示例：开始搜索

```typescript
import { startSearch, extractSessionId } from '$lib/modules/logseek';

try {
  const response = await startSearch('error AND timeout');
  const sessionId = extractSessionId(response);
  const reader = response.body?.getReader();
  
  // 处理流式响应...
} catch (error) {
  console.error('搜索失败：', error);
}
```

#### 示例：自然语言转查询

```typescript
import { convertNaturalLanguage } from '$lib/modules/logseek';

try {
  const query = await convertNaturalLanguage('查找昨天的错误日志');
  console.log('生成的查询：', query);
} catch (error) {
  console.error('AI 生成失败：', error);
}
```

### 2. 使用 Composables

Composables 封装了状态管理和业务逻辑，使用 Svelte 5 Runes。

#### 示例：使用 `useSettings()`

```svelte
<script lang="ts">
  import { useSettings } from '$lib/modules/logseek';
  
  const settings = useSettings();
  
  // 初始化加载
  let init = $state(false);
  $effect(() => {
    if (init) return;
    init = true;
    settings.loadSettings();
  });
  
  async function handleSave() {
    await settings.save();
    if (settings.saveSuccess) {
      alert('保存成功！');
    }
  }
</script>

<form onsubmit={handleSave}>
  <input bind:value={settings.endpoint} placeholder="Endpoint" />
  <input bind:value={settings.bucket} placeholder="Bucket" />
  <input bind:value={settings.accessKey} placeholder="Access Key" />
  <input bind:value={settings.secretKey} type="password" placeholder="Secret Key" />
  
  <button type="submit" disabled={settings.saving}>
    {settings.saving ? '保存中...' : '保存'}
  </button>
  
  {#if settings.saveError}
    <p class="error">{settings.saveError}</p>
  {/if}
</form>
```

#### 示例：使用 `useSearch()`

```svelte
<script lang="ts">
  import { useSearch } from '$lib/modules/logseek';
  
  const search = useSearch();
  
  // 启动搜索
  async function handleSearch(query: string) {
    await search.search(query);
  }
  
  // 加载更多
  async function loadMore() {
    await search.loadMore();
  }
</script>

<input 
  type="text" 
  onchange={(e) => handleSearch(e.target.value)}
/>

{#if search.loading}
  <p>搜索中...</p>
{/if}

{#if search.error}
  <p class="error">{search.error}</p>
{/if}

{#each search.results as result}
  <div>
    <h3>{result.path}</h3>
    <!-- 显示结果 -->
  </div>
{/each}

{#if search.hasMore}
  <button onclick={loadMore} disabled={search.loading}>
    加载更多
  </button>
{/if}
```

### 3. 使用工具函数

工具函数提供文本处理功能。

#### 示例：高亮关键词

```typescript
import { highlight } from '$lib/modules/logseek';

const line = 'Error: Connection timeout after 30 seconds';
const keywords = ['Error', 'timeout'];
const html = highlight(line, keywords);

// 结果: "⟨mark⟩Error⟨/mark⟩: Connection ⟨mark⟩timeout⟨/mark⟩ after 30 seconds"
```

#### 示例：智能截断长行

```typescript
import { snippet } from '$lib/modules/logseek';

const longLine = '很长的日志行...';
const keywords = ['error'];
const result = snippet(longLine, keywords, { max: 540, context: 230 });

console.log(result.html);        // 带高亮的 HTML
console.log(result.leftTrunc);   // 是否左侧截断
console.log(result.rightTrunc);  // 是否右侧截断
```

### 4. 使用类型定义

所有类型都集中在 `types/index.ts`。

#### 示例：定义组件 props

```typescript
import type { SearchJsonResult, JsonLine } from '$lib/modules/logseek';

interface ResultCardProps {
  result: SearchJsonResult;
  onViewFile: (path: string) => void;
}

function processLines(lines: JsonLine[]) {
  return lines.map(line => ({
    no: line.no,
    text: line.text
  }));
}
```

## 🔧 开发工作流

### 1. 添加新的 API

1. 在 `api/` 目录创建新文件（如 `myapi.ts`）
2. 定义 API 函数，使用统一的错误处理
3. 在 `api/index.ts` 中导出
4. 在 `types/index.ts` 中添加相关类型

```typescript
// api/myapi.ts
import { getApiBase, commonHeaders } from './config';

export async function myApiCall(param: string): Promise<MyResponse> {
  const API_BASE = getApiBase();
  const response = await fetch(`${API_BASE}/my-endpoint`, {
    method: 'POST',
    headers: commonHeaders,
    body: JSON.stringify({ param })
  });
  
  if (!response.ok) {
    throw new Error(`请求失败：HTTP ${response.status}`);
  }
  
  return await response.json();
}
```

### 2. 添加新的 Composable

1. 在 `composables/` 目录创建 `.svelte.ts` 文件
2. 使用 Svelte 5 Runes (`$state`, `$derived`, `$effect`)
3. 返回 getter/setter 对象
4. 在 `composables/index.ts` 中导出

```typescript
// composables/useMyFeature.svelte.ts
export function useMyFeature() {
  let data = $state<string[]>([]);
  let loading = $state(false);
  
  async function load() {
    loading = true;
    try {
      // API 调用...
      data = result;
    } finally {
      loading = false;
    }
  }
  
  return {
    get data() { return data; },
    get loading() { return loading; },
    load
  };
}
```

### 3. 添加新的工具函数

1. 在 `utils/` 目录添加或修改文件
2. 确保函数是纯函数（无副作用）
3. 添加 JSDoc 注释
4. 在 `utils/index.ts` 中导出

```typescript
// utils/myutil.ts
/**
 * 格式化日志时间戳
 * @param timestamp ISO 8601 时间戳
 * @returns 格式化的时间字符串
 */
export function formatTimestamp(timestamp: string): string {
  return new Date(timestamp).toLocaleString('zh-CN');
}
```

## ✅ 最佳实践

### 1. 类型安全

- 始终使用 TypeScript 类型
- 不要使用 `any`，使用 `unknown` 并做类型检查
- 为 API 响应定义明确的接口

### 2. 错误处理

- API 调用始终使用 try-catch
- 显示用户友好的中文错误消息
- 在 composable 中集中处理错误

### 3. 状态管理

- 使用 Svelte 5 Runes (`$state`, `$derived`, `$effect`)
- 避免在组件间传递过多状态
- 使用 composables 封装复杂的状态逻辑

### 4. 代码复用

- 提取重复的逻辑到 composables 或工具函数
- 创建可复用的 UI 组件
- 避免在多个地方重复 API 调用逻辑

### 5. 性能优化

- 使用 `$derived` 缓存计算结果
- 避免在 `$effect` 中创建无限循环
- 对大列表使用虚拟滚动

## 📚 参考资源

- [Svelte 5 文档](https://svelte.dev/docs/svelte/overview)
- [SvelteKit 文档](https://kit.svelte.dev/docs)
- [TypeScript 文档](https://www.typescriptlang.org/docs/)
- [RFC 7807 Problem Details](https://www.rfc-editor.org/rfc/rfc7807)

## 🔍 调试技巧

### 1. API 调试

在浏览器控制台检查网络请求：

```javascript
// 检查 API 响应
fetch('/api/v1/logseek/settings/minio')
  .then(r => r.json())
  .then(console.log);
```

### 2. Composable 调试

在组件中打印状态：

```svelte
<script>
  import { useSettings } from '$lib/modules/logseek';
  const settings = useSettings();
  
  $effect(() => {
    console.log('Settings state:', {
      endpoint: settings.endpoint,
      loading: settings.loadingSettings,
      error: settings.loadError
    });
  });
</script>
```

### 3. 类型检查

运行类型检查命令：

```bash
pnpm --dir ui run check
```

## 🚀 下一步

- 查看 `docs/PHASE4_SUMMARY.md` 了解重构详情
- 查看具体页面代码了解实际使用示例
- 尝试创建自己的 API 客户端或 composable
- 为模块编写单元测试
