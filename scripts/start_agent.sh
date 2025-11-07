#!/bin/bash

cd /Users/wangyue/workspace/codelder/opsbox/backend/agent

# 配置环境变量
export SERVER_ENDPOINT="http://localhost:4000"
export AGENT_ID="agent-$(hostname)"
export AGENT_NAME="Test Agent @ $(hostname)"
export SEARCH_ROOTS="/var/log,/tmp"
export AGENT_PORT=8090
export ENABLE_HEARTBEAT=true
export HEARTBEAT_INTERVAL=30
export AGENT_WORKER_THREADS=2  # 测试环境使用2个线程

echo "🤖 启动 Agent..."
echo "  Agent ID: $AGENT_ID"
echo "  Server: $SERVER_ENDPOINT"
echo "  Search Roots: $SEARCH_ROOTS"
echo "  Listen Port: $AGENT_PORT"
echo "  Worker Threads: $AGENT_WORKER_THREADS"
echo ""

cargo run --release
