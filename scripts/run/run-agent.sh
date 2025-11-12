#!/bin/bash
# ============================================================================
# LogSeek Agent 启动脚本
# ============================================================================

set -e

# 默认配置
AGENT_ID=${AGENT_ID:-"agent-$(hostname)"}
AGENT_NAME=${AGENT_NAME:-"Agent @ $(hostname)"}
SERVER_ENDPOINT=${SERVER_ENDPOINT:-"http://localhost:4000"}
SEARCH_ROOTS=${SEARCH_ROOTS:-"/var/log"}
LISTEN_PORT=${AGENT_PORT:-4001}  # Agent 默认端口是 4001
ENABLE_HEARTBEAT=${ENABLE_HEARTBEAT:-true}
HEARTBEAT_INTERVAL=${HEARTBEAT_INTERVAL:-30}
WORKER_THREADS=${AGENT_WORKER_THREADS:-""}  # 空值使用默认策略
export RUST_LOG=${RUST_LOG:-info}

echo "╔══════════════════════════════════════════╗"
echo "║   LogSeek Agent 启动脚本                 ║"
echo "╚══════════════════════════════════════════╝"
echo ""
echo "配置信息:"
echo "  Agent ID:       $AGENT_ID"
echo "  Agent Name:     $AGENT_NAME"
echo "  Server:         $SERVER_ENDPOINT"
echo "  Search Roots:   $SEARCH_ROOTS"
echo "  Listen Port:    $LISTEN_PORT"
echo "  Heartbeat:      $ENABLE_HEARTBEAT"
echo "  Worker Threads: ${WORKER_THREADS:-"auto (保守策略)"}"
echo ""

# 获取项目根目录（相对于脚本位置）
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# 检查是否已编译
if [ ! -f "$PROJECT_ROOT/backend/target/release/opsbox-agent" ]; then
    echo "⚠️  未找到编译后的 Agent 程序，正在编译..."
    cd "$PROJECT_ROOT/backend"
    cargo build --release -p opsbox-agent
    cd -
fi

# 构建命令行参数
ARGS=(
  --agent-id "$AGENT_ID"
  --agent-name "$AGENT_NAME"
  --server-endpoint "$SERVER_ENDPOINT"
  --search-roots "$SEARCH_ROOTS"
  --listen-port "$LISTEN_PORT"
  --heartbeat-interval "$HEARTBEAT_INTERVAL"
)

if [ "$ENABLE_HEARTBEAT" != "true" ]; then
  ARGS+=(--no-heartbeat)
fi

if [ -n "$WORKER_THREADS" ]; then
  ARGS+=(--worker-threads "$WORKER_THREADS")
fi

# 启动 Agent
echo "🚀 启动 Agent..."
exec "$PROJECT_ROOT/backend/target/release/opsbox-agent" "${ARGS[@]}"

