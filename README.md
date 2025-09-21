Opsbox Monorepo

目录结构
- server/api-gateway: 后端入口，监听 127.0.0.1:4000，挂载 /api/v1/logsearch/*
- server/logsearch: 日志检索工具库，导出 router()，提供 /stream
- ui/: Next.js 前端（开发端口 3001）

启动
- 后端：
  cd server/api-gateway && cargo run
- 前端：
  cd ui && npm i && npm run dev

配置
- MinIO 目前硬编码在 server/api-gateway/src/main.rs，后续改为环境变量
- 前端代理默认指向 http://127.0.0.1:4000/api/v1/logsearch/stream

AI 查询串生成（本地 Ollama）
- 新增后端接口：POST /api/v1/logsearch/nl2q
  - 请求体：{ "nl": "自然语言需求" }
  - 响应体：{ "q": "生成的查询字符串" }
- 依赖本地 Ollama（默认 http://127.0.0.1:11434）和模型 qwen3:8b
- 可通过环境变量覆盖：
  - OLLAMA_BASE_URL（默认 http://127.0.0.1:11434）
  - OLLAMA_MODEL（默认 qwen3:8b）
- 前端：搜索输入框右侧新增 AI 按钮；输入自然语言后点击即可自动生成 q 并执行搜索（中文错误提示）

