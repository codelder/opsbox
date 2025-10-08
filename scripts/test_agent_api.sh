#!/bin/bash

echo "=========================================="
echo "测试 Agent Manager API"
echo "=========================================="
echo ""

BASE_URL="http://localhost:4000"

# 1. 测试健康检查
echo "1️⃣  测试服务健康检查..."
curl -s "$BASE_URL/healthy"
echo ""
echo ""

# 2. 列出所有 Agent
echo "2️⃣  列出所有 Agent..."
curl -s "$BASE_URL/api/v1/agents" | python3 -m json.tool 2>/dev/null || curl -s "$BASE_URL/api/v1/agents"
echo ""
echo ""

# 3. 手动注册一个测试 Agent
echo "3️⃣  手动注册测试 Agent..."
curl -s -X POST "$BASE_URL/api/v1/agents/register" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "test-agent-manual",
    "name": "Manual Test Agent",
    "version": "1.0.0",
    "hostname": "localhost",
    "tags": ["test", "manual"],
    "search_roots": ["/var/log", "/tmp"],
    "last_heartbeat": 0,
    "status": {"type": "Online"}
  }'
echo ""
echo ""

# 4. 再次列出所有 Agent
echo "4️⃣  再次列出所有 Agent（应该看到刚注册的）..."
curl -s "$BASE_URL/api/v1/agents" | python3 -m json.tool 2>/dev/null || curl -s "$BASE_URL/api/v1/agents"
echo ""
echo ""

# 5. 获取特定 Agent
echo "5️⃣  获取特定 Agent 信息..."
curl -s "$BASE_URL/api/v1/agents/test-agent-manual" | python3 -m json.tool 2>/dev/null || curl -s "$BASE_URL/api/v1/agents/test-agent-manual"
echo ""
echo ""

# 6. 发送心跳
echo "6️⃣  发送心跳..."
curl -s -X POST "$BASE_URL/api/v1/agents/test-agent-manual/heartbeat" | python3 -m json.tool 2>/dev/null || curl -s -X POST "$BASE_URL/api/v1/agents/test-agent-manual/heartbeat"
echo ""
echo ""

# 7. 注销 Agent
echo "7️⃣  注销 Agent..."
curl -s -X DELETE "$BASE_URL/api/v1/agents/test-agent-manual"
echo ""
echo ""

# 8. 最后列出所有 Agent
echo "8️⃣  最后列出所有 Agent（测试 Agent 应该已被删除）..."
curl -s "$BASE_URL/api/v1/agents" | python3 -m json.tool 2>/dev/null || curl -s "$BASE_URL/api/v1/agents"
echo ""
echo ""

echo "=========================================="
echo "✅ 测试完成！"
echo "=========================================="
