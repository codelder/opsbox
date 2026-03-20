# OpsBox Frontend

SvelteKit 前端位于 `web/`，负责搜索、查看、Explorer 和设置页 UI。

## 当前结构

```text
web/src/lib/modules/
├── logseek/
├── agent/
└── explorer/
```

页面入口位于 `web/src/routes/`，当前主要页面有：

- `/`
- `/search`
- `/view`
- `/image-view`
- `/explorer`
- `/settings`
- `/prompt`

更完整说明见：

- [../docs/guides/frontend-development.md](../docs/guides/frontend-development.md)

## 开发

```bash
pnpm --dir web install
pnpm --dir web dev
```

## 构建

```bash
pnpm --dir web build
pnpm --dir web preview
```

构建产物会输出到 `backend/opsbox-server/static`，供 `opsbox-server` 嵌入。
