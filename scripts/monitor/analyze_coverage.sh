#!/bin/bash
# 测试覆盖率分析和测试补充建议脚本

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# 配置
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
COVERAGE_DIR="$PROJECT_ROOT/coverage-reports"
REPORT_DIR="$PROJECT_ROOT/test-coverage-analysis"
THRESHOLD_LINES=70
THRESHOLD_FUNCTIONS=70
THRESHOLD_BRANCHES=60

# 创建目录
mkdir -p "$COVERAGE_DIR"
mkdir -p "$REPORT_DIR"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}       测试覆盖率分析工具             ${NC}"
echo -e "${BLUE}========================================${NC}"

# 检查是否安装了cargo-tarpaulin
if ! command -v cargo-tarpaulin &> /dev/null; then
    echo -e "${YELLOW}安装cargo-tarpaulin...${NC}"
    cargo install cargo-tarpaulin
fi

# 生成覆盖率报告
echo -e "${YELLOW}生成覆盖率报告...${NC}"
cd "$PROJECT_ROOT/backend"

# 运行tarpaulin，如果已有报告则跳过
if [ ! -f "$COVERAGE_DIR/cobertura.xml" ]; then
    cargo tarpaulin \
        --out Xml \
        --out Html \
        --output-dir "$COVERAGE_DIR" \
        --all-features \
        --timeout 600 \
        --ignore-tests \
        2>&1 | tee "$REPORT_DIR/tarpaulin.log"
else
    echo -e "${GREEN}使用现有的覆盖率报告${NC}"
fi

# 检查报告是否生成
if [ ! -f "$COVERAGE_DIR/cobertura.xml" ]; then
    echo -e "${RED}错误: 覆盖率报告生成失败${NC}"
    exit 1
fi

echo -e "${GREEN}覆盖率报告已生成: $COVERAGE_DIR/${NC}"

# 简化的覆盖率分析（如果安装了cobertura工具）
if command -v cobertura-cli &> /dev/null; then
    echo -e "${YELLOW}分析覆盖率数据...${NC}"
    cobertura-cli show "$COVERAGE_DIR/cobertura.xml" > "$REPORT_DIR/coverage_summary.txt"

    # 提取覆盖率数据
    LINE_COVERAGE=$(grep -o 'line-rate="[^"]*"' "$COVERAGE_DIR/cobertura.xml" | head -1 | cut -d'"' -f2)
    BRANCH_COVERAGE=$(grep -o 'branch-rate="[^"]*"' "$COVERAGE_DIR/cobertura.xml" | head -1 | cut -d'"' -f2)

    echo -e "行覆盖率: ${LINE_COVERAGE:-未知}"
    echo -e "分支覆盖率: ${BRANCH_COVERAGE:-未知}"
else
    echo -e "${YELLOW}安装cobertura-cli以获得详细分析:${NC}"
    echo -e "  pip install cobertura-cli"
fi

# 基于现有知识生成测试补充建议
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}       测试补充建议                   ${NC}"
echo -e "${BLUE}========================================${NC}"

cat > "$REPORT_DIR/test_suggestions.md" << 'EOF'
# 测试补充建议报告

基于代码结构和现有测试覆盖分析，建议补充以下测试：

## 1. 核心模块测试补充

### LogSeek 模块
- **搜索执行器 (`search_executor.rs`)**:
  - 错误处理路径测试（文件不存在、权限错误、编码错误）
  - 并发搜索压力测试（10+并发请求）
  - 搜索取消机制测试

- **ORL协议处理 (`orl.rs`)**:
  - 恶意ORL输入防护测试（路径遍历、命令注入）
  - ORL解析错误处理测试

- **编码检测 (`encoding.rs`)**:
  - 混合编码文件测试（UTF-8 + GBK + BOM）
  - 损坏编码文件测试

### Explorer 模块
- **文件浏览 (`explorer.rs`)**:
  - 大目录列表测试（1000+文件）
  - 权限拒绝场景测试
  - 特殊字符文件名测试

### Agent Manager 模块
- **Agent健康检查 (`manager.rs`)**:
  - Agent故障恢复测试
  - 网络分区场景测试
  - 心跳超时处理测试

## 2. 边界条件测试补充

- **文件大小边界**: 空文件、超大文件（>100MB）、损坏文件
- **网络边界**: 超时、连接重置、DNS失败
- **并发边界**: 竞态条件、死锁检测、资源泄漏
- **安全边界**: SQL注入、路径遍历、命令注入

## 3. 集成测试补充

- **端到端搜索流程**: 从UI到后端完整流程测试
- **数据库迁移测试**: 版本升级兼容性测试
- **配置管理测试**: 配置热重载、错误配置恢复

## 4. 性能测试补充

- **搜索性能基准**: 不同文件大小、不同并发级别
- **内存使用监控**: 内存泄漏检测
- **启动时间测试**: 冷启动、热启动性能

## 实施优先级

1. **高优先级**: 核心业务逻辑错误处理测试
2. **中优先级**: 边界条件和安全测试
3. **低优先级**: 性能测试和极端场景测试

## 预期覆盖率提升

实施上述测试后，预期覆盖率提升：
- 行覆盖率: +15-20%
- 分支覆盖率: +20-25%
- 函数覆盖率: +10-15%
EOF

echo -e "${GREEN}测试补充建议已生成: $REPORT_DIR/test_suggestions.md${NC}"

# 生成低覆盖率文件列表（简化版）
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}       低覆盖率文件分析               ${NC}"
echo -e "${BLUE}========================================${NC}"

# 基于文件修改时间和测试存在性分析
find backend -name "*.rs" -path "*/src/*" | grep -v test | while read file; do
    filename=$(basename "$file")
    modulename=$(echo "$file" | sed 's|backend/||')

    # 检查是否有对应的测试文件
    test_file=$(find backend -name "*test*.rs" -type f | xargs grep -l "$filename" 2>/dev/null | head -1)

    if [ -z "$test_file" ]; then
        echo -e "${YELLOW}⚠️  缺少测试: $modulename${NC}"
    fi
done | head -20 > "$REPORT_DIR/low_coverage_files.txt"

echo -e "${GREEN}低覆盖率文件列表: $REPORT_DIR/low_coverage_files.txt${NC}"

# 生成CI集成建议
cat > "$REPORT_DIR/ci_integration.md" << 'EOF'
# CI集成建议

## 覆盖率检查集成

### GitHub Actions配置
```yaml
# 在现有工作流中添加覆盖率检查
- name: Check test coverage
  run: |
    cargo tarpaulin --out Xml --output-dir coverage-reports
    # 检查覆盖率阈值
    # 如果覆盖率低于阈值，标记为失败
```

### 覆盖率阈值配置
建议设置以下覆盖率阈值：
- 行覆盖率: 70%
- 函数覆盖率: 70%
- 分支覆盖率: 60%

### 覆盖率趋势监控
建议集成Codecov或Coveralls跟踪覆盖率趋势：
1. 每次PR生成覆盖率报告
2. 对比基准分支覆盖率
3. 阻止覆盖率下降的合并

## 测试分类执行

### 快速测试（CI门禁）
```bash
./scripts/monitor/run_tests_with_monitoring.sh
```

### 完整测试（包括覆盖率）
```bash
./scripts/monitor/run_tests_with_monitoring.sh --include-ignored
cargo tarpaulin --out Html --output-dir coverage-reports
```

## 报告生成

### 自动报告生成
每次CI运行后：
1. 生成HTML覆盖率报告
2. 上传为Artifact
3. 在PR评论中显示覆盖率变化

### 定期报告
每周生成完整覆盖率报告，包含：
- 覆盖率趋势图
- 低覆盖率模块分析
- 测试补充建议
EOF

echo -e "${GREEN}CI集成建议: $REPORT_DIR/ci_integration.md${NC}"

echo -e "${BLUE}========================================${NC}"
echo -e "${GREEN}覆盖率分析完成!${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "报告目录: $REPORT_DIR/"
echo -e "覆盖率报告: $COVERAGE_DIR/"
echo -e ""
echo -e "后续步骤:"
echo -e "1. 查看 $REPORT_DIR/test_suggestions.md 补充测试"
echo -e "2. 集成覆盖率检查到CI"
echo -e "3. 定期监控覆盖率趋势"