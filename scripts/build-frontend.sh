#!/usr/bin/env bash
set -euo pipefail
# 中文注释：一键构建前端（SvelteKit），并将产物写入后端 server/api-gateway/static
# 注意：构建过程会清空 server/api-gateway/static 目录

# 切换到仓库根目录的 ui 子项目进行构建
pnpm --dir "$(dirname "$0")/../ui" build

echo "前端构建完成，产物已写入 server/api-gateway/static"

