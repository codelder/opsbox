# 测试监控体系集成指南

## 概述

测试监控体系为OpsBox项目提供全面的测试结果收集、分析和报告功能。该系统包含以下组件：

1. **测试监控器 (`TestMonitor`)**: 收集测试结果和统计信息
2. **测试分类器 (`TestCategory`)**: 自动分类测试用例
3. **报告生成器**: 生成JSON、Markdown和HTML格式的报告
4. **覆盖率跟踪器 (`TestCoverageTracker`)**: 跟踪测试覆盖率
5. **CI集成脚本**: 自动化测试执行和报告生成

## 快速开始

### 在测试中使用监控器

```rust
use opsbox_test_common::test_monitoring::{TestMonitor, TestResult, ReportFormat};

#[tokio::test]
async fn example_test_with_monitoring() {
    // 创建监控器
    let mut monitor = TestMonitor::new("./test-reports");

    // 记录测试开始
    let timer = monitor.record_test_start(
        "example_test_with_monitoring",
        "logseek::tests::example",
        vec!["unit".to_string(), "logseek".to_string()],
    );

    // 执行测试逻辑
    let result = std::panic::catch_unwind(|| {
        // 测试代码
        assert_eq!(1 + 1, 2);
    });

    // 记录测试结束
    match result {
        Ok(_) => {
            monitor.record_test_end(timer, TestResult::Passed);
        }
        Err(e) => {
            let error = if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown error".to_string()
            };
            monitor.record_test_end(timer, TestResult::Failed { error });
        }
    }

    // 生成报告
    let report = monitor.generate_report(ReportFormat::Markdown)
        .expect("Failed to generate report");
    println!("{}", report);
}
```

### 使用监控宏简化

```rust
use opsbox_test_common::{monitor_test};

#[test]
fn test_with_macro() {
    let mut monitor = TestMonitor::new("./test-reports");

    monitor_test!(monitor, "test_with_macro", "module::name", vec!["unit"], {
        // 测试代码
        assert!(true);
    });
}
```

## 测试分类

系统自动根据测试名称和标签进行分类：

| 分类 | 识别规则 | 示例 |
|------|----------|------|
| 单元测试 | 名称包含"unit"或"test_" | `test_user_creation` |
| 集成测试 | 名称包含"integration" | `database_integration` |
| 性能测试 | 名称包含"performance"或"bench" | `search_performance` |
| 安全测试 | 名称包含"security"或"malicious" | `sql_injection_security` |
| 边界条件测试 | 名称包含"boundary"或"edge" | `boundary_conditions` |
| 端到端测试 | 名称包含"e2e"或"end_to_end" | `e2e_workflow` |

## CI/CD集成

### GitHub Actions集成示例

```yaml
name: Tests with Monitoring

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Run tests with monitoring
        run: |
          mkdir -p test-reports
          ./scripts/monitor/run_tests_with_monitoring.sh

      - name: Upload test reports
        uses: actions/upload-artifact@v4
        if: always()
        with:
          name: test-reports
          path: test-reports/
```

### 本地开发

```bash
# 快速运行测试（跳过性能/安全测试）
./scripts/monitor/run_tests_with_monitoring.sh

# 运行所有测试（包括被忽略的）
./scripts/monitor/run_tests_with_monitoring.sh --include-ignored

# 生成覆盖率报告
cargo tarpaulin --out Html --output-dir ./test-reports/coverage
```

## 报告格式

### Markdown报告示例

```markdown
# 测试执行报告

**生成时间**: 2024-01-29T10:30:00Z
**总执行时间**: 2.45s

## 统计摘要

- **总测试数**: 42
- **通过数**: 40
- **失败数**: 2
- **跳过数**: 0
- **通过率**: 95.24%
- **平均测试时间**: 58.33ms

## 分类统计

- **单元测试**: 25
- **集成测试**: 10
- **性能测试**: 5
- **安全测试**: 2

## 失败的测试

### test_database_connection
- **模块**: logseek::tests::database
- **标签**: integration, database
- **错误**: Connection timeout
- **耗时**: 1.23s
```

### JSON报告结构

```json
{
  "timestamp": "2024-01-29T10:30:00Z",
  "test_cases": [
    {
      "name": "test_example",
      "module": "module::path",
      "result": "Passed",
      "duration": "58.33ms",
      "tags": ["unit", "example"],
      "metadata": {}
    }
  ],
  "statistics": {
    "total_tests": 42,
    "passed_tests": 40,
    "failed_tests": 2,
    "skipped_tests": 0,
    "total_duration": "2.45s",
    "category_stats": {
      "Unit": 25,
      "Integration": 10
    }
  }
}
```

## 测试覆盖率跟踪

### 安装覆盖率工具

```bash
cargo install cargo-tarpaulin
cargo install cargo-llvm-cov
```

### 生成覆盖率报告

```rust
use opsbox_test_common::test_monitoring::TestCoverageTracker;

fn track_coverage() {
    let mut tracker = TestCoverageTracker::new();

    // 添加模块覆盖率
    tracker.add_coverage("logseek", 85.5);
    tracker.add_coverage("agent-manager", 92.3);
    tracker.add_coverage("explorer", 78.9);

    // 生成报告
    let report = tracker.generate_coverage_report();
    println!("{}", report);
}
```

## 最佳实践

### 1. 合理使用测试标签

```rust
// 好的实践
vec!["unit".to_string(), "database".to_string(), "fast".to_string()]

// 更好的实践（明确分类）
vec!["integration".to_string(), "database".to_string(), "slow".to_string()]
```

### 2. 分类性能测试

```rust
#[test]
#[ignore = "性能测试，只在CI中运行"]
fn test_search_performance() {
    // 性能测试代码
}
```

### 3. 集成到现有测试

```rust
// 现有测试
#[tokio::test]
async fn test_existing_functionality() {
    // 添加监控
    let mut monitor = TestMonitor::new("./reports");
    let timer = monitor.record_test_start(
        "test_existing_functionality",
        "module::path",
        vec!["integration".to_string()],
    );

    // 原有测试逻辑
    let result = std::panic::catch_unwind(|| {
        // 测试代码
    });

    // 记录结果
    match result {
        Ok(_) => monitor.record_test_end(timer, TestResult::Passed),
        Err(e) => monitor.record_test_end(timer, TestResult::Failed {
            error: format!("{:?}", e)
        }),
    }
}
```

## 故障排除

### 常见问题

1. **编译错误**: 确保添加了`serde`和`chrono`依赖
2. **报告目录权限**: 确保有写入权限
3. **测试分类错误**: 检查测试名称和标签是否明确

### 调试监控器

```rust
// 启用详细日志
env_logger::init();

let monitor = TestMonitor::new("./reports");
// 监控器会自动记录关键事件
```

## 扩展功能

### 自定义报告格式

```rust
impl TestMonitor {
    pub fn generate_custom_report(&self, template: &str) -> Result<String, TestError> {
        // 实现自定义模板引擎
        Ok(format!("Custom report: {}", template))
    }
}
```

### 数据库存储

```rust
// 将测试结果存储到数据库
pub struct DatabaseTestStorage {
    pool: SqlitePool,
}

impl DatabaseTestStorage {
    pub async fn store_test_results(&self, report: &TestReport) -> Result<(), TestError> {
        // 存储逻辑
        Ok(())
    }
}
```

## 版本历史

- **v0.1.0** (2024-01-29): 初始版本，包含基本监控和报告功能
- **v0.2.0** (计划): 添加数据库存储和实时监控
- **v0.3.0** (计划): 集成分布式测试跟踪

---

**注意**: 此监控体系与现有的`#[ignore]`标记兼容。性能测试和安全测试应标记为`#[ignore]`，在CI中使用`-- --ignored`标志运行。