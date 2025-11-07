#!/bin/bash
# ============================================================================
# LogSeek Agent 启动脚本
# ============================================================================

set -e

# 默认配置
export AGENT_ID=${AGENT_ID:-"agent-$(hostname)"}
export AGENT_NAME=${AGENT_NAME:-"Agent @ $(hostname)"}
export SERVER_ENDPOINT=${SERVER_ENDPOINT:-"http://localhost:4000"}
export SEARCH_ROOTS=${SEARCH_ROOTS:-"/var/log"}
export AGENT_PORT=${AGENT_PORT:-8090}
export ENABLE_HEARTBEAT=${ENABLE_HEARTBEAT:-true}
export HEARTBEAT_INTERVAL=${HEARTBEAT_INTERVAL:-30}
export AGENT_WORKER_THREADS=${AGENT_WORKER_THREADS:-""}  # 空值使用默认策略
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
echo "  Listen Port:    $AGENT_PORT"
echo "  Heartbeat:      $ENABLE_HEARTBEAT"
echo "  Worker Threads: ${AGENT_WORKER_THREADS:-"auto (保守策略)"}"
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

# 启动 Agent
echo "🚀 启动 Agent..."
exec "$PROJECT_ROOT/backend/target/release/opsbox-agent"

