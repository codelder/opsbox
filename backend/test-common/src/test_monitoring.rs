//! 测试监控和报告模块
//!
//! 提供测试结果收集、分析和报告生成功能：
//! - 测试结果收集器
//! - 测试分类分析
//! - 测试性能监控
//! - 测试覆盖率跟踪
//! - 测试报告生成

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::TestError;

/// 测试结果枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestResult {
  Passed,
  Failed { error: String },
  Skipped,
  TimedOut { duration: Duration },
}

/// 单个测试用例信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
  /// 测试名称
  pub name: String,
  /// 测试模块路径
  pub module: String,
  /// 测试结果
  pub result: TestResult,
  /// 执行时长
  pub duration: Duration,
  /// 测试分类标签
  pub tags: Vec<String>,
  /// 额外元数据
  pub metadata: HashMap<String, String>,
}

/// 测试分类枚举
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum TestCategory {
  /// 单元测试
  Unit,
  /// 集成测试
  Integration,
  /// 性能测试
  Performance,
  /// 安全测试
  Security,
  /// 边界条件测试
  Boundary,
  /// 端到端测试
  EndToEnd,
  /// 其他测试
  Other,
}

impl TestCategory {
  /// 获取分类描述
  pub fn description(&self) -> &'static str {
    match self {
      TestCategory::Unit => "单元测试",
      TestCategory::Integration => "集成测试",
      TestCategory::Performance => "性能测试",
      TestCategory::Security => "安全测试",
      TestCategory::Boundary => "边界条件测试",
      TestCategory::EndToEnd => "端到端测试",
      TestCategory::Other => "其他测试",
    }
  }

  /// 从测试名称和标签推断分类
  pub fn infer_from_test(name: &str, tags: &[String]) -> Self {
    let name_lower = name.to_lowercase();

    // 检查标签中的分类信息
    for tag in tags {
      match tag.to_lowercase().as_str() {
        "unit" => return TestCategory::Unit,
        "integration" => return TestCategory::Integration,
        "performance" | "bench" => return TestCategory::Performance,
        "security" => return TestCategory::Security,
        "boundary" => return TestCategory::Boundary,
        "e2e" | "endtoend" => return TestCategory::EndToEnd,
        _ => {}
      }
    }

    // 根据测试名称推断
    // 注意：顺序很重要，更具体的分类应该先检查
    if name_lower.contains("integration") {
      TestCategory::Integration
    } else if name_lower.contains("performance") || name_lower.contains("bench") {
      TestCategory::Performance
    } else if name_lower.contains("security") || name_lower.contains("malicious") {
      TestCategory::Security
    } else if name_lower.contains("boundary") || name_lower.contains("edge") {
      TestCategory::Boundary
    } else if name_lower.contains("e2e") || name_lower.contains("end_to_end") {
      TestCategory::EndToEnd
    } else if name_lower.contains("unit") || name_lower.contains("test_") {
      TestCategory::Unit
    } else {
      TestCategory::Other
    }
  }
}

/// 测试执行统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestStatistics {
  /// 总测试数
  pub total_tests: usize,
  /// 通过的测试数
  pub passed_tests: usize,
  /// 失败的测试数
  pub failed_tests: usize,
  /// 跳过的测试数
  pub skipped_tests: usize,
  /// 超时的测试数
  pub timed_out_tests: usize,
  /// 总执行时间
  pub total_duration: Duration,
  /// 分类统计
  pub category_stats: HashMap<TestCategory, usize>,
}

impl TestStatistics {
  /// 更新统计信息
  pub fn update(&mut self, test_case: &TestCase) {
    self.total_tests += 1;
    self.total_duration += test_case.duration;

    let category = TestCategory::infer_from_test(&test_case.name, &test_case.tags);
    *self.category_stats.entry(category).or_insert(0) += 1;

    match test_case.result {
      TestResult::Passed => self.passed_tests += 1,
      TestResult::Failed { .. } => self.failed_tests += 1,
      TestResult::Skipped => self.skipped_tests += 1,
      TestResult::TimedOut { .. } => self.timed_out_tests += 1,
    }
  }

  /// 计算通过率
  pub fn pass_rate(&self) -> f64 {
    if self.total_tests == 0 {
      0.0
    } else {
      (self.passed_tests as f64 / (self.total_tests - self.skipped_tests) as f64) * 100.0
    }
  }

  /// 计算平均测试时间
  pub fn average_duration(&self) -> Duration {
    if self.total_tests == 0 {
      Duration::ZERO
    } else {
      self.total_duration / self.total_tests as u32
    }
  }
}

/// 测试监控器
pub struct TestMonitor {
  /// 测试用例集合
  test_cases: Vec<TestCase>,
  /// 测试统计
  statistics: TestStatistics,
  /// 开始时间
  start_time: Instant,
  /// 报告输出目录
  report_dir: String,
}

impl TestMonitor {
  /// 创建新的测试监控器
  pub fn new(report_dir: &str) -> Self {
    // 确保报告目录存在
    let _ = fs::create_dir_all(report_dir);

    Self {
      test_cases: Vec::new(),
      statistics: TestStatistics::default(),
      start_time: Instant::now(),
      report_dir: report_dir.to_string(),
    }
  }

  /// 记录测试开始
  pub fn record_test_start(&self, name: &str, module: &str, tags: Vec<String>) -> TestTimer {
    TestTimer {
      name: name.to_string(),
      module: module.to_string(),
      tags,
      start_time: Instant::now(),
    }
  }

  /// 记录测试结束
  pub fn record_test_end(&mut self, timer: TestTimer, result: TestResult) {
    let duration = timer.start_time.elapsed();
    let test_case = TestCase {
      name: timer.name,
      module: timer.module,
      result,
      duration,
      tags: timer.tags,
      metadata: HashMap::new(),
    };

    self.test_cases.push(test_case.clone());
    self.statistics.update(&test_case);
  }

  /// 获取统计信息
  pub fn statistics(&self) -> &TestStatistics {
    &self.statistics
  }

  /// 生成测试报告
  pub fn generate_report(&self, format: ReportFormat) -> Result<String, TestError> {
    match format {
      ReportFormat::Json => self.generate_json_report(),
      ReportFormat::Markdown => self.generate_markdown_report(),
      ReportFormat::Html => self.generate_html_report(),
    }
  }

  /// 生成JSON报告
  fn generate_json_report(&self) -> Result<String, TestError> {
    let report = TestReport {
      timestamp: chrono::Utc::now().to_rfc3339(),
      test_cases: self.test_cases.clone(),
      statistics: self.statistics.clone(),
      total_duration: self.start_time.elapsed(),
    };

    let json = serde_json::to_string_pretty(&report).map_err(|e| TestError::Other(e.to_string()))?;

    let report_path = Path::new(&self.report_dir).join("test_report.json");
    fs::write(&report_path, &json).map_err(TestError::Io)?;

    Ok(json)
  }

  /// 生成Markdown报告
  fn generate_markdown_report(&self) -> Result<String, TestError> {
    let mut markdown = String::new();

    // 标题和时间
    markdown.push_str("# 测试执行报告\n\n");
    markdown.push_str(&format!("**生成时间**: {}\n\n", chrono::Utc::now().to_rfc3339()));
    markdown.push_str(&format!("**总执行时间**: {:.2?}\n\n", self.start_time.elapsed()));

    // 统计摘要
    markdown.push_str("## 统计摘要\n\n");
    markdown.push_str(&format!("- **总测试数**: {}\n", self.statistics.total_tests));
    markdown.push_str(&format!("- **通过数**: {}\n", self.statistics.passed_tests));
    markdown.push_str(&format!("- **失败数**: {}\n", self.statistics.failed_tests));
    markdown.push_str(&format!("- **跳过数**: {}\n", self.statistics.skipped_tests));
    markdown.push_str(&format!("- **超时数**: {}\n", self.statistics.timed_out_tests));
    markdown.push_str(&format!("- **通过率**: {:.2}%\n", self.statistics.pass_rate()));
    markdown.push_str(&format!(
      "- **平均测试时间**: {:.2?}\n\n",
      self.statistics.average_duration()
    ));

    // 分类统计
    markdown.push_str("## 分类统计\n\n");
    for (category, count) in &self.statistics.category_stats {
      markdown.push_str(&format!("- **{}**: {}\n", category.description(), count));
    }
    markdown.push('\n');

    // 失败的测试详情
    let failed_tests: Vec<_> = self
      .test_cases
      .iter()
      .filter(|tc| matches!(tc.result, TestResult::Failed { .. }))
      .collect();

    if !failed_tests.is_empty() {
      markdown.push_str("## 失败的测试\n\n");
      for test_case in failed_tests {
        if let TestResult::Failed { error } = &test_case.result {
          markdown.push_str(&format!("### {}\n", test_case.name));
          markdown.push_str(&format!("- **模块**: {}\n", test_case.module));
          markdown.push_str(&format!("- **标签**: {}\n", test_case.tags.join(", ")));
          markdown.push_str(&format!("- **错误**: {}\n", error));
          markdown.push_str(&format!("- **耗时**: {:.2?}\n\n", test_case.duration));
        }
      }
    }

    let report_path = Path::new(&self.report_dir).join("test_report.md");
    fs::write(&report_path, &markdown).map_err(TestError::Io)?;

    Ok(markdown)
  }

  /// 生成HTML报告
  fn generate_html_report(&self) -> Result<String, TestError> {
    // 简化的HTML报告，包含统计信息和表格
    let mut html = String::new();

    html.push_str("<!DOCTYPE html>\n");
    html.push_str("<html lang=\"zh-CN\">\n");
    html.push_str("<head>\n");
    html.push_str("    <meta charset=\"UTF-8\">\n");
    html.push_str("    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
    html.push_str("    <title>测试执行报告</title>\n");
    html.push_str("    <style>\n");
    html.push_str("        body { font-family: Arial, sans-serif; margin: 20px; }\n");
    html.push_str("        h1 { color: #333; }\n");
    html
      .push_str("        .summary { background: #f5f5f5; padding: 15px; border-radius: 5px; margin-bottom: 20px; }\n");
    html.push_str("        .stat { display: inline-block; margin-right: 20px; }\n");
    html.push_str("        .pass { color: green; font-weight: bold; }\n");
    html.push_str("        .fail { color: red; font-weight: bold; }\n");
    html.push_str("        .skip { color: orange; }\n");
    html.push_str("        table { border-collapse: collapse; width: 100%; margin-top: 20px; }\n");
    html.push_str("        th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }\n");
    html.push_str("        th { background-color: #f2f2f2; }\n");
    html.push_str("        tr:nth-child(even) { background-color: #f9f9f9; }\n");
    html.push_str("    </style>\n");
    html.push_str("</head>\n");
    html.push_str("<body>\n");

    html.push_str("    <h1>测试执行报告</h1>\n");
    html.push_str(&format!(
      "    <p><strong>生成时间</strong>: {}</p>\n",
      chrono::Utc::now().to_rfc3339()
    ));
    html.push_str(&format!(
      "    <p><strong>总执行时间</strong>: {:.2?}</p>\n",
      self.start_time.elapsed()
    ));

    // 统计摘要
    html.push_str("    <div class=\"summary\">\n");
    html.push_str("        <h2>统计摘要</h2>\n");
    html.push_str(&format!(
      "        <div class=\"stat\"><strong>总测试数</strong>: {}</div>\n",
      self.statistics.total_tests
    ));
    html.push_str(&format!(
      "        <div class=\"stat pass\"><strong>通过数</strong>: {}</div>\n",
      self.statistics.passed_tests
    ));
    html.push_str(&format!(
      "        <div class=\"stat fail\"><strong>失败数</strong>: {}</div>\n",
      self.statistics.failed_tests
    ));
    html.push_str(&format!(
      "        <div class=\"stat skip\"><strong>跳过数</strong>: {}</div>\n",
      self.statistics.skipped_tests
    ));
    html.push_str(&format!(
      "        <div class=\"stat\"><strong>通过率</strong>: {:.2}%</div>\n",
      self.statistics.pass_rate()
    ));
    html.push_str("    </div>\n");

    // 失败的测试表格
    let failed_tests: Vec<_> = self
      .test_cases
      .iter()
      .filter(|tc| matches!(tc.result, TestResult::Failed { .. }))
      .collect();

    if !failed_tests.is_empty() {
      html.push_str("    <h2>失败的测试</h2>\n");
      html.push_str("    <table>\n");
      html.push_str("        <tr>\n");
      html.push_str("            <th>测试名称</th>\n");
      html.push_str("            <th>模块</th>\n");
      html.push_str("            <th>标签</th>\n");
      html.push_str("            <th>错误信息</th>\n");
      html.push_str("            <th>耗时</th>\n");
      html.push_str("        </tr>\n");

      for test_case in failed_tests {
        if let TestResult::Failed { error } = &test_case.result {
          html.push_str("        <tr>\n");
          html.push_str(&format!("            <td>{}</td>\n", test_case.name));
          html.push_str(&format!("            <td>{}</td>\n", test_case.module));
          html.push_str(&format!("            <td>{}</td>\n", test_case.tags.join(", ")));
          html.push_str(&format!("            <td>{}</td>\n", error));
          html.push_str(&format!("            <td>{:.2?}</td>\n", test_case.duration));
          html.push_str("        </tr>\n");
        }
      }

      html.push_str("    </table>\n");
    }

    html.push_str("</body>\n");
    html.push_str("</html>\n");

    let report_path = Path::new(&self.report_dir).join("test_report.html");
    fs::write(&report_path, &html).map_err(TestError::Io)?;

    Ok(html)
  }
}

/// 测试计时器
pub struct TestTimer {
  name: String,
  module: String,
  tags: Vec<String>,
  start_time: Instant,
}

/// 完整测试报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReport {
  /// 报告时间戳
  pub timestamp: String,
  /// 测试用例列表
  pub test_cases: Vec<TestCase>,
  /// 统计信息
  pub statistics: TestStatistics,
  /// 总执行时间
  pub total_duration: Duration,
}

/// 报告格式枚举
#[derive(Debug, Clone, Copy)]
pub enum ReportFormat {
  Json,
  Markdown,
  Html,
}

/// 测试分类分析器
pub struct TestCategoryAnalyzer;

impl TestCategoryAnalyzer {
  /// 分析测试套件的分类构成
  pub fn analyze_suite(test_cases: &[TestCase]) -> HashMap<TestCategory, usize> {
    let mut category_counts = HashMap::new();

    for test_case in test_cases {
      let category = TestCategory::infer_from_test(&test_case.name, &test_case.tags);
      *category_counts.entry(category).or_insert(0) += 1;
    }

    category_counts
  }

  /// 生成分类建议
  pub fn suggest_test_categories(_module_name: &str, test_names: &[&str]) -> Vec<(String, TestCategory)> {
    let mut suggestions = Vec::new();

    for test_name in test_names {
      let name_lower = test_name.to_lowercase();
      let category = if name_lower.contains("integration") || name_lower.contains("_integration") {
        TestCategory::Integration
      } else if name_lower.contains("performance") || name_lower.contains("bench") {
        TestCategory::Performance
      } else if name_lower.contains("security") || name_lower.contains("malicious") {
        TestCategory::Security
      } else if name_lower.contains("boundary") || name_lower.contains("edge") {
        TestCategory::Boundary
      } else if name_lower.contains("e2e") || name_lower.contains("end_to_end") {
        TestCategory::EndToEnd
      } else {
        TestCategory::Unit
      };

      suggestions.push((test_name.to_string(), category));
    }

    suggestions
  }
}

/// 测试覆盖率跟踪器（简化版）
pub struct TestCoverageTracker {
  /// 覆盖率数据
  coverage_data: HashMap<String, f64>,
}

impl Default for TestCoverageTracker {
  fn default() -> Self {
    Self::new()
  }
}

impl TestCoverageTracker {
  /// 创建新的覆盖率跟踪器
  pub fn new() -> Self {
    Self {
      coverage_data: HashMap::new(),
    }
  }

  /// 添加覆盖率数据
  pub fn add_coverage(&mut self, module: &str, coverage_percentage: f64) {
    self.coverage_data.insert(module.to_string(), coverage_percentage);
  }

  /// 获取总体覆盖率
  pub fn overall_coverage(&self) -> f64 {
    if self.coverage_data.is_empty() {
      0.0
    } else {
      self.coverage_data.values().sum::<f64>() / self.coverage_data.len() as f64
    }
  }

  /// 生成覆盖率报告
  pub fn generate_coverage_report(&self) -> String {
    let mut report = String::new();

    report.push_str("## 测试覆盖率报告\n\n");
    report.push_str(&format!("**总体覆盖率**: {:.2}%\n\n", self.overall_coverage()));

    report.push_str("| 模块 | 覆盖率 |\n");
    report.push_str("|------|--------|\n");

    let mut entries: Vec<_> = self.coverage_data.iter().collect();
    entries.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());

    for (module, coverage) in entries {
      report.push_str(&format!("| {} | {:.2}% |\n", module, coverage));
    }

    report
  }
}

/// 测试监控宏（简化版）
#[macro_export]
macro_rules! monitor_test {
  ($monitor:expr, $name:expr, $module:expr, $tags:expr, $block:block) => {{
    let timer = $monitor.record_test_start($name, $module, $tags);
    let result = std::panic::catch_unwind(|| $block);
    match result {
      Ok(_) => {
        $monitor.record_test_end(timer, $crate::test_monitoring::TestResult::Passed);
        true
      }
      Err(e) => {
        let error = if let Some(s) = e.downcast_ref::<&str>() {
          s.to_string()
        } else if let Some(s) = e.downcast_ref::<String>() {
          s.clone()
        } else {
          "Unknown error".to_string()
        };
        $monitor.record_test_end(timer, $crate::test_monitoring::TestResult::Failed { error });
        false
      }
    }
  }};
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::Duration;
  use tempfile::tempdir;

  #[test]
  fn test_test_result_variants() {
    // 测试TestResult枚举的各种变体
    let passed = TestResult::Passed;
    let failed = TestResult::Failed {
      error: "test error".to_string(),
    };
    let skipped = TestResult::Skipped;
    let timed_out = TestResult::TimedOut {
      duration: Duration::from_secs(5),
    };

    // 验证变体能正确构造
    match passed {
      TestResult::Passed => {}
      _ => panic!("Expected Passed"),
    }

    match failed {
      TestResult::Failed { error } => assert_eq!(error, "test error"),
      _ => panic!("Expected Failed"),
    }

    match skipped {
      TestResult::Skipped => {}
      _ => panic!("Expected Skipped"),
    }

    match timed_out {
      TestResult::TimedOut { duration } => assert_eq!(duration, Duration::from_secs(5)),
      _ => panic!("Expected TimedOut"),
    }
  }

  #[test]
  fn test_test_case_creation() {
    // 测试TestCase结构体的创建
    let mut metadata = HashMap::new();
    metadata.insert("key".to_string(), "value".to_string());

    let test_case = TestCase {
      name: "test_example".to_string(),
      module: "test_module".to_string(),
      result: TestResult::Passed,
      duration: Duration::from_millis(150),
      tags: vec!["unit".to_string(), "fast".to_string()],
      metadata,
    };

    assert_eq!(test_case.name, "test_example");
    assert_eq!(test_case.module, "test_module");
    assert_eq!(test_case.duration, Duration::from_millis(150));
    assert_eq!(test_case.tags.len(), 2);
    assert_eq!(test_case.metadata.get("key"), Some(&"value".to_string()));
  }

  #[test]
  fn test_test_category_description() {
    // 测试分类描述
    assert_eq!(TestCategory::Unit.description(), "单元测试");
    assert_eq!(TestCategory::Integration.description(), "集成测试");
    assert_eq!(TestCategory::Performance.description(), "性能测试");
    assert_eq!(TestCategory::Security.description(), "安全测试");
    assert_eq!(TestCategory::Boundary.description(), "边界条件测试");
    assert_eq!(TestCategory::EndToEnd.description(), "端到端测试");
    assert_eq!(TestCategory::Other.description(), "其他测试");
  }

  #[test]
  fn test_test_category_infer_from_name() {
    // 测试从名称推断分类
    assert_eq!(TestCategory::infer_from_test("test_something", &[]), TestCategory::Unit);
    assert_eq!(
      TestCategory::infer_from_test("test_integration_flow", &[]),
      TestCategory::Integration
    );
    assert_eq!(
      TestCategory::infer_from_test("benchmark_performance", &[]),
      TestCategory::Performance
    );
    assert_eq!(
      TestCategory::infer_from_test("security_test", &[]),
      TestCategory::Security
    );
    assert_eq!(
      TestCategory::infer_from_test("test_boundary_conditions", &[]),
      TestCategory::Boundary
    );
    assert_eq!(TestCategory::infer_from_test("e2e_test", &[]), TestCategory::EndToEnd);
    assert_eq!(TestCategory::infer_from_test("other_test", &[]), TestCategory::Other);
  }

  #[test]
  fn test_test_category_infer_from_tags() {
    // 测试从标签推断分类
    let tags = vec!["integration".to_string()];
    assert_eq!(
      TestCategory::infer_from_test("any_test", &tags),
      TestCategory::Integration
    );

    let tags = vec!["performance".to_string()];
    assert_eq!(
      TestCategory::infer_from_test("any_test", &tags),
      TestCategory::Performance
    );

    let tags = vec!["security".to_string()];
    assert_eq!(TestCategory::infer_from_test("any_test", &tags), TestCategory::Security);

    // 标签优先级测试：标签优先于名称
    let tags = vec!["unit".to_string()];
    assert_eq!(
      TestCategory::infer_from_test("integration_test", &tags),
      TestCategory::Unit
    );
  }

  #[test]
  fn test_test_statistics_update() {
    // 测试统计信息更新
    let mut stats = TestStatistics::default();

    let test_case = TestCase {
      name: "test1".to_string(),
      module: "module1".to_string(),
      result: TestResult::Passed,
      duration: Duration::from_millis(100),
      tags: vec!["unit".to_string()],
      metadata: HashMap::new(),
    };

    stats.update(&test_case);

    assert_eq!(stats.total_tests, 1);
    assert_eq!(stats.passed_tests, 1);
    assert_eq!(stats.failed_tests, 0);
    assert_eq!(stats.skipped_tests, 0);
    assert_eq!(stats.timed_out_tests, 0);
    assert_eq!(stats.total_duration, Duration::from_millis(100));
    assert_eq!(stats.category_stats.get(&TestCategory::Unit), Some(&1));
  }

  #[test]
  fn test_test_statistics_pass_rate() {
    // 测试通过率计算
    let mut stats = TestStatistics::default();

    // 空统计
    assert_eq!(stats.pass_rate(), 0.0);

    // 全部通过
    let test_case1 = TestCase {
      name: "test1".to_string(),
      module: "module1".to_string(),
      result: TestResult::Passed,
      duration: Duration::from_millis(100),
      tags: vec![],
      metadata: HashMap::new(),
    };

    let test_case2 = TestCase {
      name: "test2".to_string(),
      module: "module1".to_string(),
      result: TestResult::Passed,
      duration: Duration::from_millis(200),
      tags: vec![],
      metadata: HashMap::new(),
    };

    stats.update(&test_case1);
    stats.update(&test_case2);

    // 2个测试，0个跳过，100%通过率
    assert_eq!(stats.pass_rate(), 100.0);

    // 添加失败测试
    let test_case3 = TestCase {
      name: "test3".to_string(),
      module: "module1".to_string(),
      result: TestResult::Failed {
        error: "error".to_string(),
      },
      duration: Duration::from_millis(150),
      tags: vec![],
      metadata: HashMap::new(),
    };

    stats.update(&test_case3);

    // 3个测试，0个跳过，2/3通过 = 66.67%
    assert!((stats.pass_rate() - 66.6667).abs() < 0.001);

    // 添加跳过测试（跳过的不计入分母）
    let test_case4 = TestCase {
      name: "test4".to_string(),
      module: "module1".to_string(),
      result: TestResult::Skipped,
      duration: Duration::from_millis(50),
      tags: vec![],
      metadata: HashMap::new(),
    };

    stats.update(&test_case4);

    // 4个测试，1个跳过，2/3通过 = 66.67%（分母排除跳过的）
    assert!((stats.pass_rate() - 66.6667).abs() < 0.001);
  }

  #[test]
  fn test_test_statistics_average_duration() {
    // 测试平均时长计算
    let mut stats = TestStatistics::default();

    // 空统计
    assert_eq!(stats.average_duration(), Duration::ZERO);

    // 添加测试
    let test_case1 = TestCase {
      name: "test1".to_string(),
      module: "module1".to_string(),
      result: TestResult::Passed,
      duration: Duration::from_millis(100),
      tags: vec![],
      metadata: HashMap::new(),
    };

    let test_case2 = TestCase {
      name: "test2".to_string(),
      module: "module1".to_string(),
      result: TestResult::Passed,
      duration: Duration::from_millis(200),
      tags: vec![],
      metadata: HashMap::new(),
    };

    stats.update(&test_case1);
    stats.update(&test_case2);

    // 平均时长 = (100 + 200) / 2 = 150ms
    assert_eq!(stats.average_duration(), Duration::from_millis(150));
  }

  #[test]
  fn test_test_monitor_creation() {
    // 测试监控器创建
    let temp_dir = tempdir().unwrap();
    let monitor = TestMonitor::new(temp_dir.path().to_str().unwrap());

    assert_eq!(monitor.test_cases.len(), 0);
    assert_eq!(monitor.statistics.total_tests, 0);
  }

  #[test]
  fn test_test_monitor_record_test() {
    // 测试记录测试功能
    let temp_dir = tempdir().unwrap();
    let mut monitor = TestMonitor::new(temp_dir.path().to_str().unwrap());

    let timer = monitor.record_test_start("test1", "module1", vec!["unit".to_string()]);

    // 等待一小段时间模拟测试执行
    std::thread::sleep(Duration::from_millis(10));

    monitor.record_test_end(timer, TestResult::Passed);

    assert_eq!(monitor.test_cases.len(), 1);
    assert_eq!(monitor.statistics.total_tests, 1);
    assert_eq!(monitor.statistics.passed_tests, 1);
  }

  #[test]
  fn test_test_timer_structure() {
    // 测试TestTimer结构体
    let timer = TestTimer {
      name: "test1".to_string(),
      module: "module1".to_string(),
      tags: vec!["unit".to_string(), "fast".to_string()],
      start_time: Instant::now(),
    };

    assert_eq!(timer.name, "test1");
    assert_eq!(timer.module, "module1");
    assert_eq!(timer.tags.len(), 2);
  }

  #[test]
  fn test_test_category_analyzer() {
    // 测试分类分析器
    let test_cases = vec![
      TestCase {
        name: "test_unit".to_string(),
        module: "module1".to_string(),
        result: TestResult::Passed,
        duration: Duration::from_millis(100),
        tags: vec!["unit".to_string()],
        metadata: HashMap::new(),
      },
      TestCase {
        name: "test_integration".to_string(),
        module: "module2".to_string(),
        result: TestResult::Passed,
        duration: Duration::from_millis(200),
        tags: vec!["integration".to_string()],
        metadata: HashMap::new(),
      },
    ];

    let analysis = TestCategoryAnalyzer::analyze_suite(&test_cases);

    assert_eq!(analysis.get(&TestCategory::Unit), Some(&1));
    assert_eq!(analysis.get(&TestCategory::Integration), Some(&1));
  }

  #[test]
  fn test_test_coverage_tracker() {
    // 测试覆盖率跟踪器
    let mut tracker = TestCoverageTracker::new();

    // 初始覆盖率
    assert_eq!(tracker.overall_coverage(), 0.0);

    // 添加覆盖率数据
    tracker.add_coverage("module1", 75.5);
    tracker.add_coverage("module2", 82.3);

    // 计算总体覆盖率 (75.5 + 82.3) / 2 = 78.9
    let overall = tracker.overall_coverage();
    assert!((overall - 78.9).abs() < 0.001);

    // 生成报告
    let report = tracker.generate_coverage_report();
    assert!(report.contains("## 测试覆盖率报告"));
    assert!(report.contains("module1"));
    assert!(report.contains("module2"));
  }

  #[tokio::test]
  async fn test_monitor_test_macro() {
    // 测试监控宏（异步版本）
    let temp_dir = tempdir().unwrap();
    let mut monitor = TestMonitor::new(temp_dir.path().to_str().unwrap());

    let result = monitor_test!(monitor, "macro_test", "test_module", vec!["unit".to_string()], {
      // 测试代码块
      assert_eq!(2 + 2, 4);
    });

    assert!(result); // 宏应该返回true表示通过
    assert_eq!(monitor.test_cases.len(), 1);
    assert_eq!(monitor.statistics.passed_tests, 1);
  }

  #[tokio::test]
  async fn test_monitor_test_macro_failure() {
    // 测试监控宏的失败处理
    let temp_dir = tempdir().unwrap();
    let mut monitor = TestMonitor::new(temp_dir.path().to_str().unwrap());

    let result = monitor_test!(monitor, "macro_fail_test", "test_module", vec!["unit".to_string()], {
      // 故意失败的测试代码块
      panic!("Intentional panic for test");
    });

    assert!(!result); // 宏应该返回false表示失败
    assert_eq!(monitor.test_cases.len(), 1);
    assert_eq!(monitor.statistics.failed_tests, 1);
  }

  #[test]
  fn test_report_format_enum() {
    // 测试报告格式枚举
    let json_format = ReportFormat::Json;
    let markdown_format = ReportFormat::Markdown;
    let html_format = ReportFormat::Html;

    // 验证枚举值能正确构造（编译时检查）
    match json_format {
      ReportFormat::Json => {}
      _ => panic!("Expected Json format"),
    }

    match markdown_format {
      ReportFormat::Markdown => {}
      _ => panic!("Expected Markdown format"),
    }

    match html_format {
      ReportFormat::Html => {}
      _ => panic!("Expected Html format"),
    }
  }

  #[test]
  fn test_test_report_structure() {
    // 测试TestReport结构体
    let report = TestReport {
      timestamp: "2024-01-01T00:00:00Z".to_string(),
      test_cases: vec![],
      statistics: TestStatistics::default(),
      total_duration: Duration::from_secs(10),
    };

    assert_eq!(report.timestamp, "2024-01-01T00:00:00Z");
    assert_eq!(report.test_cases.len(), 0);
    assert_eq!(report.total_duration, Duration::from_secs(10));
  }
}
