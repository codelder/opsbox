#!/usr/bin/env bash
set -euo pipefail
# 中文注释：一键构建前端（SvelteKit），并将产物写入后端 backend/api-gateway/static
# 注意：构建过程会清空 backend/api-gateway/static 目录
# 在 Windows 环境下请使用 Node 脚本 scripts/build-frontend.mjs（跨平台），此 bash 脚本仅供类 Unix 系统使用。

# 切换到仓库根目录的 web 子项目进行构建
pnpm --dir "$(dirname "$0")/../web" build

echo "前端构建完成，产物已写入 backend/api-gateway/static"
