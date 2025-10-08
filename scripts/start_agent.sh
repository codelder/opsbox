#!/bin/bash

cd /Users/wangyue/workspace/codelder/opsboard/server/agent

# 配置环境变量
export SERVER_ENDPOINT="http://localhost:4000"
export AGENT_ID="agent-$(hostname)"
export AGENT_NAME="Test Agent @ $(hostname)"
export SEARCH_ROOTS="/var/log,/tmp"
export AGENT_PORT=8090
export ENABLE_HEARTBEAT=true
export HEARTBEAT_INTERVAL=30

echo "🤖 启动 Agent..."
echo "  Agent ID: $AGENT_ID"
echo "  Server: $SERVER_ENDPOINT"
echo "  Search Roots: $SEARCH_ROOTS"
echo "  Listen Port: $AGENT_PORT"
echo ""

cargo run --release
