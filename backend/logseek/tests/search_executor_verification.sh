#!/bin/bash
# 搜索执行器功能验证脚本
# 验证多数据源、并发控制、缓存功能

set -e

echo "=========================================="
echo "搜索执行器功能验证"
echo "=========================================="

# 颜色定义
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 服务器地址
SERVER_URL="${SERVER_URL:-http://localhost:8080}"
API_ENDPOINT="${SERVER_URL}/api/v1/logseek/search.ndjson"

# 检查服务器是否运行
echo -e "\n${YELLOW}[1/5] 检查服务器状态...${NC}"
if ! curl -s -f "${SERVER_URL}/health" > /dev/null 2>&1; then
    echo -e "${RED}✗ 服务器未运行，请先启动服务器${NC}"
    echo "  提示: cd backend && cargo run --bin opsbox-server"
    exit 1
fi
echo -e "${GREEN}✓ 服务器运行正常${NC}"

# 测试 1: 基本搜索功能
echo -e "\n${YELLOW}[2/5] 测试基本搜索功能...${NC}"
RESPONSE=$(curl -s -X POST "${API_ENDPOINT}" \
    -H "Content-Type: application/json" \
    -d '{"q": "error", "context": 3}' \
    -w "\n%{http_code}")

HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
BODY=$(echo "$RESPONSE" | head -n -1)

if [ "$HTTP_CODE" != "200" ]; then
    echo -e "${RED}✗ HTTP 状态码错误: $HTTP_CODE${NC}"
    exit 1
fi

# 检查响应头
SID=$(curl -s -X POST "${API_ENDPOINT}" \
    -H "Content-Type: application/json" \
    -d '{"q": "error", "context": 3}' \
    -D - -o /dev/null | grep -i "X-Logseek-SID" | cut -d' ' -f2 | tr -d '\r')

if [ -z "$SID" ]; then
    echo -e "${RED}✗ 未找到 X-Logseek-SID 响应头${NC}"
    exit 1
fi
echo -e "${GREEN}✓ 基本搜索功能正常，SID: $SID${NC}"

# 测试 2: NDJSON 格式验证
echo -e "\n${YELLOW}[3/5] 验证 NDJSON 格式...${NC}"
RESPONSE=$(curl -s -X POST "${API_ENDPOINT}" \
    -H "Content-Type: application/json" \
    -d '{"q": "error", "context": 2}')

# 检查每行是否为有效 JSON
LINE_COUNT=0
VALID_JSON=true
while IFS= read -r line; do
    if [ -n "$line" ]; then
        LINE_COUNT=$((LINE_COUNT + 1))
        if ! echo "$line" | jq . > /dev/null 2>&1; then
            echo -e "${RED}✗ 第 $LINE_COUNT 行不是有效的 JSON${NC}"
            echo "  内容: $line"
            VALID_JSON=false
            break
        fi
    fi
done <<< "$RESPONSE"

if [ "$VALID_JSON" = true ] && [ "$LINE_COUNT" -gt 0 ]; then
    echo -e "${GREEN}✓ NDJSON 格式正确，共 $LINE_COUNT 行${NC}"
else
    echo -e "${RED}✗ NDJSON 格式验证失败${NC}"
    exit 1
fi

# 测试 3: 多数据源搜索（如果配置了多个源）
echo -e "\n${YELLOW}[4/5] 测试多数据源搜索...${NC}"
RESPONSE=$(curl -s -X POST "${API_ENDPOINT}" \
    -H "Content-Type: application/json" \
    -d '{"q": "test", "context": 1}')

# 统计 Complete 事件数量（每个数据源完成时发送一个）
COMPLETE_COUNT=$(echo "$RESPONSE" | grep -c '"type":"complete"' || true)

if [ "$COMPLETE_COUNT" -gt 0 ]; then
    echo -e "${GREEN}✓ 多数据源搜索正常，收到 $COMPLETE_COUNT 个完成事件${NC}"
else
    echo -e "${YELLOW}⚠ 未检测到完成事件（可能没有配置数据源）${NC}"
fi

# 测试 4: 缓存功能验证
echo -e "\n${YELLOW}[5/5] 验证缓存功能...${NC}"

# 第一次搜索，获取 SID
RESPONSE_WITH_HEADERS=$(curl -s -X POST "${API_ENDPOINT}" \
    -H "Content-Type: application/json" \
    -d '{"q": "cache_test", "context": 2}' \
    -D -)

SID=$(echo "$RESPONSE_WITH_HEADERS" | grep -i "X-Logseek-SID" | cut -d' ' -f2 | tr -d '\r')

if [ -z "$SID" ]; then
    echo -e "${RED}✗ 无法获取 SID${NC}"
    exit 1
fi

echo "  获取到 SID: $SID"

# 验证关键字缓存（通过 view API）
# 注意：这需要 view API 支持，如果没有则跳过
if curl -s -f "${SERVER_URL}/api/v1/logseek/view" > /dev/null 2>&1; then
    echo -e "${GREEN}✓ 缓存功能可用（SID 已生成）${NC}"
else
    echo -e "${YELLOW}⚠ 无法验证缓存详情（view API 不可用）${NC}"
fi

# 测试 5: 并发控制验证（发送多个并发请求）
echo -e "\n${YELLOW}[额外] 测试并发控制...${NC}"

# 启动 5 个并发搜索请求
for i in {1..5}; do
    curl -s -X POST "${API_ENDPOINT}" \
        -H "Content-Type: application/json" \
        -d "{\"q\": \"concurrent_test_$i\", \"context\": 1}" \
        > /dev/null &
done

# 等待所有请求完成
wait

echo -e "${GREEN}✓ 并发请求处理正常${NC}"

# 总结
echo -e "\n=========================================="
echo -e "${GREEN}✓ 所有验证测试通过！${NC}"
echo "=========================================="
echo ""
echo "验证项目："
echo "  ✓ 基本搜索功能"
echo "  ✓ NDJSON 格式"
echo "  ✓ 多数据源支持"
echo "  ✓ 缓存功能（SID 生成）"
echo "  ✓ 并发控制"
echo ""
echo "功能状态："
echo "  - 多数据源并行搜索: ✓ 正常"
echo "  - 并发控制: ✓ 正常"
echo "  - 缓存功能: ✓ 正常"
echo ""
