Opsbox Monorepo

目录结构
- backend/api-gateway: 后端入口，监听 127.0.0.1:4000，挂载 /api/v1/logsearch/*
- backend/logsearch: 日志检索工具库，导出 router()，提供 /stream
- frontend/: Next.js 前端（开发端口 3001）

启动
- 后端：
  cd backend/api-gateway && cargo run
- 前端：
  cd frontend && npm i && npm run dev

配置
- MinIO 目前硬编码在 backend/api-gateway/src/main.rs，后续改为环境变量
- 前端代理默认指向 http://127.0.0.1:4000/api/v1/logsearch/stream


