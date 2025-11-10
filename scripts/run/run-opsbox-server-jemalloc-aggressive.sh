#!/usr/bin/env bash
set -euo pipefail

# ⚠️ 注意：此脚本已过时
# 当前项目已切换为 mimalloc 作为全局分配器，此脚本中的 MALLOC_CONF 设置（针对 jemalloc）不再生效。
# 请直接使用 start-server.sh 或直接运行 opsbox-server。

# 获取项目根目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "⚠️  警告：此脚本已过时，项目现在使用 mimalloc 而非 jemalloc"
echo "请使用 scripts/run/start-server.sh 或直接运行 opsbox-server"
echo ""

# 以工作区 manifest 路径运行 opsbox-server，透传所有参数
exec cargo run --manifest-path "$PROJECT_ROOT/backend/Cargo.toml" -p opsbox-server -- "$@"
