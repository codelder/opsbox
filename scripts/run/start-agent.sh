#!/bin/bash

# 获取项目根目录（相对于脚本位置）
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# 配置参数
SERVER_ENDPOINT="${SERVER_ENDPOINT:-http://localhost:4000}"
AGENT_ID="${AGENT_ID:-agent-$(hostname)}"
AGENT_NAME="${AGENT_NAME:-Test Agent @ $(hostname)}"
SEARCH_ROOTS="${SEARCH_ROOTS:-/var/log,/tmp}"
LISTEN_PORT="${AGENT_PORT:-8090}"
ENABLE_HEARTBEAT="${ENABLE_HEARTBEAT:-true}"
HEARTBEAT_INTERVAL="${HEARTBEAT_INTERVAL:-30}"
WORKER_THREADS="${AGENT_WORKER_THREADS:-2}"  # 测试环境使用2个线程

echo "🤖 启动 Agent..."
echo "  Agent ID: $AGENT_ID"
echo "  Agent Name: $AGENT_NAME"
echo "  Server: $SERVER_ENDPOINT"
echo "  Search Roots: $SEARCH_ROOTS"
echo "  Listen Port: $LISTEN_PORT"
echo "  Worker Threads: $WORKER_THREADS"
echo ""

# 构建命令行参数
ARGS=(
  --agent-id "$AGENT_ID"
  --agent-name "$AGENT_NAME"
  --server-endpoint "$SERVER_ENDPOINT"
  --search-roots "$SEARCH_ROOTS"
  --listen-port "$LISTEN_PORT"
  --heartbeat-interval "$HEARTBEAT_INTERVAL"
  --worker-threads "$WORKER_THREADS"
)

if [ "$ENABLE_HEARTBEAT" != "true" ]; then
  ARGS+=(--no-heartbeat)
fi

cd "$PROJECT_ROOT/backend"
cargo run --release -p opsbox-agent -- "${ARGS[@]}"
