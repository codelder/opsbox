# 前端开发指南

**文档版本**: v1.1  
**最后更新**: 2026-03-20

本文档描述当前 `web/` 前端的实际结构和开发约定。

## 技术栈

- SvelteKit 2
- Svelte 5
- TypeScript
- Tailwind CSS 4
- Vite 7
- Vitest
- Playwright

## 目录结构

```text
web/
├── src/routes/                  # 页面路由
├── src/lib/components/          # 通用 UI 组件
├── src/lib/modules/
│   ├── logseek/
│   │   ├── api/
│   │   ├── composables/
│   │   ├── types/
│   │   └── utils/
│   ├── agent/
│   │   ├── api/
│   │   ├── composables/
│   │   └── types/
│   └── explorer/
│       ├── api.ts
│       ├── types.ts
│       └── utils.ts
├── static/
└── tests/
    └── e2e/
```

## 页面与功能对应

- `/`
  - 首页搜索输入
  - 支持直接查询和 AI 模式 NL2Q
- `/search`
  - 搜索结果展示
  - 结果按 ORL 解析为本地 / Agent / S3 资源树
- `/view`
  - 文本文件查看
- `/image-view`
  - 图片查看
- `/explorer`
  - ORL 驱动的资源浏览器
- `/settings`
  - 五个标签页：
    - 对象存储
    - Agent
    - 规划脚本
    - 大模型
    - Server 日志
- `/prompt`
  - Prompt 调试页

## 模块约定

### `logseek`

负责：

- 搜索
- 查看文件
- S3 默认配置和 Profiles
- LLM backends
- planners
- NL2Q

统一导出入口：

```ts
import { useSearch, fetchProfiles, convertNaturalLanguage } from '$lib/modules/logseek';
```

当前导出子模块：

- `api`
- `types`
- `utils`
- `composables`

当前 composables：

- `useSearch`
- `useSettings`
- `useProfiles`
- `useStreamReader`
- `useLlmBackends`

### `agent`

负责：

- Agent 列表
- 标签管理
- Agent 日志代理 API

当前 composable：

- `useAgents`

### `explorer`

负责：

- `POST /api/v1/explorer/list`
- `GET /api/v1/explorer/download`
- ORL 相关资源项渲染辅助

## API 基址

### LogSeek

`web/src/lib/modules/logseek/api/config.ts`

```ts
PUBLIC_API_BASE || '/api/v1/logseek'
```

### Agent

`web/src/lib/modules/agent/api/config.ts`

```ts
PUBLIC_AGENTS_API_BASE || '/api/v1/agents'
```

### Explorer

当前默认直接使用：

```ts
'/api/v1/explorer'
```

## 当前数据模型注意点

### S3 默认配置

`fetchS3Settings()` / `saveS3Settings()` 当前字段只有：

- `endpoint`
- `access_key`
- `secret_key`

不再包含 `bucket`。

### S3 Profile

当前 `S3ProfilePayload` 字段：

- `profile_name`
- `endpoint`
- `access_key`
- `secret_key`

`bucket` 不属于 profile 本身，而由 ORL 或搜索规划决定。

### Agent 状态

Agent 状态是 tagged union：

```ts
type AgentStatus =
  | { type: 'Online' }
  | { type: 'Busy'; tasks: number }
  | { type: 'Offline' };
```

## ORL 约定

前端已经以 ORL 为统一资源标识。

常见形式：

```text
orl://local/var/log/app.log
orl://agent/
orl://s3/
orl://web-01@agent/var/log/app.log
orl://prod:logs-bucket@s3/path/to/file.log
orl://local/var/log/archive.tar.gz?entry=inner/file.log
```

相关代码：

- `web/src/lib/utils/orl.ts`
- `/search` 页面资源树构建
- `/explorer` 页面导航与 URL 编码

## 新增功能时的推荐落点

### 新增 LogSeek API

1. 在 `web/src/lib/modules/logseek/api/` 中新增文件或补充现有文件
2. 在 `api/index.ts` 导出
3. 如需状态管理，在 `composables/` 增加对应封装
4. 补充对应 `*.test.ts`

### 新增 Agent 页面行为

1. 先在 `agent/api/` 写纯 API 封装
2. 再在 `agent/composables/` 聚合 UI 需要的状态与动作

### 新增页面

1. 将页面级逻辑保留在 `src/routes/`
2. 通用逻辑下沉到 `src/lib/modules/` 或 `src/lib/components/`

## 开发命令

```bash
pnpm --dir web install
pnpm --dir web dev
pnpm --dir web check
pnpm --dir web lint
pnpm --dir web test
pnpm --dir web test:e2e
```

## 测试约定

- API 封装优先配 `*.test.ts`
- UI 状态逻辑优先测 composable
- 用户主流程用 Playwright E2E 覆盖

当前 E2E 已覆盖搜索、Explorer、Agent、S3、设置页、可访问性等关键路径。
