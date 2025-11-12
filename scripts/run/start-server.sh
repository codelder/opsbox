#!/bin/bash

# 获取项目根目录（相对于脚本位置）
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "🚀 启动 OpsBox Server..."
cd "$PROJECT_ROOT/backend"
cargo run --release -p opsbox-server
