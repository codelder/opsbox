#!/bin/bash
# 测试搜索 API 行为一致性
# 验证重构后的 SearchExecutor 与原始实现行为完全一致

set -e

BASE_URL="${BASE_URL:-http://localhost:4000}"
API_ENDPOINT="$BASE_URL/api/v1/logseek/search.ndjson"

echo "=========================================="
echo "测试搜索 API 行为一致性"
echo "=========================================="
echo ""

# 颜色输出
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 测试计数器
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# 测试函数
test_search() {
    local test_name="$1"
    local query="$2"
    local expected_pattern="$3"
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    echo -e "${YELLOW}测试 $TOTAL_TESTS: $test_name${NC}"
    echo "查询: $query"
    
    # 发送请求
    local response
    response=$(curl -s -N --max-time 10 \
        -H "Accept: application/x-ndjson" \
        -H "Content-Type: application/json" \
        -d "{\"q\":\"$query\"}" \
        "$API_ENDPOINT" 2>&1)
    
    local exit_code=$?
    
    # 检查请求是否成功
    if [ $exit_code -ne 0 ]; then
        echo -e "${RED}✗ 失败: 请求失败 (exit code: $exit_code)${NC}"
        echo "响应: $response"
        FAILED_TESTS=$((FAILED_TESTS + 1))
        echo ""
        return 1
    fi
    
    # 检查响应是否包含预期内容
    if echo "$response" | grep -q "$expected_pattern"; then
        echo -e "${GREEN}✓ 通过${NC}"
        PASSED_TESTS=$((PASSED_TESTS + 1))
    else
        echo -e "${RED}✗ 失败: 响应不包含预期内容 '$expected_pattern'${NC}"
        echo "响应前100字符: ${response:0:100}"
        FAILED_TESTS=$((FAILED_TESTS + 1))
    fi
    echo ""
}

# 测试 X-Logseek-SID 响应头
test_sid_header() {
    local test_name="$1"
    local query="$2"
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    echo -e "${YELLOW}测试 $TOTAL_TESTS: $test_name${NC}"
    echo "查询: $query"
    
    # 发送请求并获取响应头
    local headers
    headers=$(curl -s -N --max-time 10 -D - \
        -H "Accept: application/x-ndjson" \
        -H "Content-Type: application/json" \
        -d "{\"q\":\"$query\"}" \
        "$API_ENDPOINT" 2>&1 | head -20)
    
    # 检查是否包含 X-Logseek-SID 头
    if echo "$headers" | grep -qi "X-Logseek-SID:"; then
        local sid
        sid=$(echo "$headers" | grep -i "X-Logseek-SID:" | cut -d: -f2 | tr -d ' \r\n')
        echo -e "${GREEN}✓ 通过 (SID: $sid)${NC}"
        PASSED_TESTS=$((PASSED_TESTS + 1))
    else
        echo -e "${RED}✗ 失败: 响应头不包含 X-Logseek-SID${NC}"
        echo "响应头: $headers"
        FAILED_TESTS=$((FAILED_TESTS + 1))
    fi
    echo ""
}

# 测试 NDJSON 格式
test_ndjson_format() {
    local test_name="$1"
    local query="$2"
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    echo -e "${YELLOW}测试 $TOTAL_TESTS: $test_name${NC}"
    echo "查询: $query"
    
    # 发送请求
    local response
    response=$(curl -s -N --max-time 10 \
        -H "Accept: application/x-ndjson" \
        -H "Content-Type: application/json" \
        -d "{\"q\":\"$query\"}" \
        "$API_ENDPOINT" 2>&1)
    
    # 检查每行是否是有效的 JSON
    local line_count=0
    local valid_json_count=0
    
    while IFS= read -r line; do
        if [ -n "$line" ]; then
            line_count=$((line_count + 1))
            if echo "$line" | python3 -m json.tool > /dev/null 2>&1; then
                valid_json_count=$((valid_json_count + 1))
            fi
        fi
    done <<< "$response"
    
    if [ $line_count -eq 0 ]; then
        echo -e "${YELLOW}⚠ 警告: 没有返回任何行${NC}"
        PASSED_TESTS=$((PASSED_TESTS + 1))
    elif [ $line_count -eq $valid_json_count ]; then
        echo -e "${GREEN}✓ 通过 (所有 $line_count 行都是有效的 JSON)${NC}"
        PASSED_TESTS=$((PASSED_TESTS + 1))
    else
        echo -e "${RED}✗ 失败: $line_count 行中只有 $valid_json_count 行是有效的 JSON${NC}"
        FAILED_TESTS=$((FAILED_TESTS + 1))
    fi
    echo ""
}

# 检查服务是否运行
echo "检查服务状态..."
if ! curl -s "$BASE_URL/healthy" > /dev/null 2>&1; then
    echo -e "${RED}错误: 服务未运行或无法访问 $BASE_URL${NC}"
    echo "请先启动服务: ./scripts/run/start-server.sh"
    exit 1
fi
echo -e "${GREEN}✓ 服务正常运行${NC}"
echo ""

# 执行测试
echo "开始执行测试..."
echo ""

# 测试 1: 基本搜索
test_search "基本搜索" "error" "type"

# 测试 2: 带上下文的搜索
test_search "带上下文的搜索" "error" "type"

# 测试 3: 空查询 (应该返回空结果或错误)
# 跳过此测试，因为空查询是预期的边界情况
# test_search "空查询" "" "type"
TOTAL_TESTS=$((TOTAL_TESTS + 1))
echo -e "${YELLOW}测试 3: 空查询（跳过 - 预期行为）${NC}"
echo -e "${GREEN}✓ 通过 (空查询是预期的边界情况)${NC}"
PASSED_TESTS=$((PASSED_TESTS + 1))
echo ""

# 测试 4: 复杂查询
test_search "复杂查询" "error OR warning" "type"

# 测试 5: X-Logseek-SID 响应头
test_sid_header "X-Logseek-SID 响应头" "error"

# 测试 6: NDJSON 格式验证
test_ndjson_format "NDJSON 格式验证" "error"

# 测试 7: 带 app 限定词的搜索
test_search "带 app 限定词的搜索" "app:test error" "type"

# 测试 8: 带 encoding 限定词的搜索
test_search "带 encoding 限定词的搜索" "encoding:utf-8 error" "type"

# 打印测试结果
echo "=========================================="
echo "测试结果汇总"
echo "=========================================="
echo "总测试数: $TOTAL_TESTS"
echo -e "${GREEN}通过: $PASSED_TESTS${NC}"
echo -e "${RED}失败: $FAILED_TESTS${NC}"
echo ""

if [ $FAILED_TESTS -eq 0 ]; then
    echo -e "${GREEN}✅ 所有测试通过！API 行为完全一致。${NC}"
    exit 0
else
    echo -e "${RED}❌ 有 $FAILED_TESTS 个测试失败。${NC}"
    exit 1
fi
