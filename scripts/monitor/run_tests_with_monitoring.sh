#!/bin/bash
# 测试运行脚本，集成监控功能
# 运行所有测试并生成监控报告

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# 配置文件
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
REPORT_DIR="$PROJECT_ROOT/test-reports"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# 创建报告目录
mkdir -p "$REPORT_DIR/$TIMESTAMP"
echo -e "${BLUE}测试报告将保存到: $REPORT_DIR/$TIMESTAMP${NC}"

# 清理函数
cleanup() {
    echo -e "\n${YELLOW}清理中...${NC}"
    # 保留最近5个报告，删除旧的
    cd "$REPORT_DIR" && ls -t | tail -n +6 | xargs rm -rf 2>/dev/null || true
    echo -e "${GREEN}清理完成${NC}"
}

# 捕获退出信号
trap cleanup EXIT INT TERM

# 运行后端测试
echo -e "\n${BLUE}========== 运行后端测试 ==========${NC}"
cd "$PROJECT_ROOT/backend"

# 设置测试环境变量
export CARGO_PROFILE_DEV_DEBUG=true
export RUST_BACKTRACE=1

# 运行测试并捕获输出
echo -e "${YELLOW}运行所有测试...${NC}"
TEST_OUTPUT_FILE="$REPORT_DIR/$TIMESTAMP/test_output.log"

# 运行测试，根据参数决定是否运行被忽略的测试
RUN_IGNORED=false
if [[ "$1" == "--include-ignored" ]] || [[ "$1" == "-i" ]]; then
    RUN_IGNORED=true
    echo -e "${YELLOW}包含被忽略的测试（性能/安全测试）${NC}"
fi

# 构建测试二进制文件
echo -e "${YELLOW}构建测试二进制文件...${NC}"
cargo test --no-run --all-features 2>&1 | tee -a "$TEST_OUTPUT_FILE"

if [ "$RUN_IGNORED" = true ]; then
    echo -e "${YELLOW}运行所有测试（包括被忽略的）...${NC}"
    cargo test --all-features -- --nocapture --test-threads=1 2>&1 | tee -a "$TEST_OUTPUT_FILE"

    echo -e "${YELLOW}运行被忽略的测试...${NC}"
    cargo test --all-features -- --ignored --nocapture 2>&1 | tee -a "$TEST_OUTPUT_FILE"
else
    echo -e "${YELLOW}运行快速测试（跳过被忽略的测试）...${NC}"
    cargo test --all-features -- --nocapture --test-threads=1 2>&1 | tee -a "$TEST_OUTPUT_FILE"
fi

# 检查测试结果
if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ 所有测试通过${NC}"
    TEST_STATUS="PASSED"
else
    echo -e "${RED}✗ 测试失败${NC}"
    TEST_STATUS="FAILED"
    # 不立即退出，继续生成报告
fi

# 生成测试报告
echo -e "\n${BLUE}========== 生成测试报告 ==========${NC}"

# 分析测试输出
echo -e "${YELLOW}分析测试输出...${NC}"

# 创建测试报告摘要
REPORT_SUMMARY="$REPORT_DIR/$TIMESTAMP/test_summary.md"

cat > "$REPORT_SUMMARY" << EOF
# 测试执行报告

**执行时间**: $(date)
**状态**: $TEST_STATUS
**报告目录**: $REPORT_DIR/$TIMESTAMP

## 测试执行详情

\`\`\`
$(tail -50 "$TEST_OUTPUT_FILE")
\`\`\`

## 测试分类

根据现有测试文件自动分类：

| 分类 | 测试文件数 |
|------|------------|
| 单元测试 | $(find . -name "*_test.rs" -o -name "test*.rs" | grep -v integration | grep -v performance | grep -v security | wc -l) |
| 集成测试 | $(find . -name "*_integration.rs" | wc -l) |
| 性能测试 | $(find . -name "*_performance*.rs" | wc -l) |
| 安全测试 | $(find . -name "*_security*.rs" | wc -l) |
| 边界条件测试 | $(find . -name "*_boundary*.rs" | wc -l) |

## 测试覆盖率跟踪

注意：需要安装 cargo-tarpaulin 或 cargo-llvm-cov 来生成准确的覆盖率报告。

\`\`\`bash
# 安装覆盖率工具
cargo install cargo-tarpaulin
# 生成覆盖率报告
cargo tarpaulin --out Html --output-dir $REPORT_DIR/$TIMESTAMP/coverage
\`\`\`

## CI集成

将此脚本集成到GitHub Actions中：

\`\`\`yaml
- name: Run tests with monitoring
  run: ./scripts/monitor/run_tests_with_monitoring.sh
\`\`\`
EOF

echo -e "${GREEN}测试报告已生成: $REPORT_SUMMARY${NC}"

# 如果测试失败，显示详细信息
if [ "$TEST_STATUS" = "FAILED" ]; then
    echo -e "\n${RED}========== 失败测试详情 ==========${NC}"
    grep -A 10 -B 5 "FAILED\|ERROR\|panicked" "$TEST_OUTPUT_FILE" | head -100

    # 显示最后50行日志
    echo -e "\n${RED}========== 最后50行测试输出 ==========${NC}"
    tail -50 "$TEST_OUTPUT_FILE"

    exit 1
else
    echo -e "${GREEN}✓ 测试执行完成${NC}"
    exit 0
fi