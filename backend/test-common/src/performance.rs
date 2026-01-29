//! 性能测试工具
//!
//! 提供轻量级性能测试和基准测量工具

use crate::TestError;
use std::time::{Duration, Instant};

/// 性能测量结果
#[derive(Debug, Clone)]
pub struct PerformanceMeasurement {
  /// 操作名称
  pub operation: String,
  /// 执行时间
  pub duration: Duration,
  /// 迭代次数（如果适用）
  pub iterations: Option<usize>,
  /// 额外指标
  pub metrics: std::collections::HashMap<String, f64>,
}

impl PerformanceMeasurement {
  /// 创建新的性能测量结果
  pub fn new(operation: impl Into<String>, duration: Duration) -> Self {
    Self {
      operation: operation.into(),
      duration,
      iterations: None,
      metrics: std::collections::HashMap::new(),
    }
  }

  /// 添加迭代次数
  pub fn with_iterations(mut self, iterations: usize) -> Self {
    self.iterations = Some(iterations);
    self
  }

  /// 添加额外指标
  pub fn with_metric(mut self, key: impl Into<String>, value: f64) -> Self {
    self.metrics.insert(key.into(), value);
    self
  }

  /// 获取每秒操作数（如果提供了迭代次数）
  pub fn operations_per_second(&self) -> Option<f64> {
    self.iterations.map(|iter| iter as f64 / self.duration.as_secs_f64())
  }

  /// 格式化输出结果
  pub fn format(&self) -> String {
    let mut parts = vec![
      format!("操作: {}", self.operation),
      format!("耗时: {:.2?}", self.duration),
    ];

    if let Some(iter) = self.iterations {
      parts.push(format!("迭代次数: {}", iter));
    }

    if let Some(ops) = self.operations_per_second() {
      parts.push(format!("每秒操作数: {:.2}", ops));
    }

    for (key, value) in &self.metrics {
      parts.push(format!("{}: {:.2}", key, value));
    }

    parts.join(", ")
  }
}

/// 性能测试运行器
pub struct PerformanceRunner {
  /// 测试名称
  name: String,
  /// 测量结果
  measurements: Vec<PerformanceMeasurement>,
}

impl PerformanceRunner {
  /// 创建新的性能测试运行器
  pub fn new(name: impl Into<String>) -> Self {
    Self {
      name: name.into(),
      measurements: Vec::new(),
    }
  }

  /// 测量单个操作的执行时间
  pub fn measure<F, R>(&mut self, operation: impl Into<String>, f: F) -> R
  where
    F: FnOnce() -> R,
  {
    let operation_name = operation.into();
    let start = Instant::now();
    let result = f();
    let duration = start.elapsed();

    let measurement = PerformanceMeasurement::new(operation_name.clone(), duration);
    self.measurements.push(measurement);

    result
  }

  /// 测量多次迭代的平均时间
  pub fn measure_iterations<F>(&mut self, operation: impl Into<String>, iterations: usize, mut f: F)
  where
    F: FnMut(usize) -> (),
  {
    let operation_name = operation.into();
    let start = Instant::now();

    for i in 0..iterations {
      f(i);
    }

    let duration = start.elapsed();

    let measurement = PerformanceMeasurement::new(operation_name, duration).with_iterations(iterations);

    self.measurements.push(measurement);
  }

  /// 获取所有测量结果
  pub fn measurements(&self) -> &[PerformanceMeasurement] {
    &self.measurements
  }

  /// 打印性能报告
  pub fn print_report(&self) {
    println!("📊 性能测试报告: {}", self.name);
    println!("{}", "─".repeat(60));

    for measurement in &self.measurements {
      println!("  • {}", measurement.format());
    }

    println!("{}", "─".repeat(60));

    // 计算总计
    let total_duration: Duration = self.measurements.iter().map(|m| m.duration).sum();
    println!("  总计耗时: {:.2?}", total_duration);
    println!();
  }
}

/// 创建文件大小指标
pub fn format_file_size(bytes: u64) -> String {
  const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];

  let mut size = bytes as f64;
  let mut unit_index = 0;

  while size >= 1024.0 && unit_index < UNITS.len() - 1 {
    size /= 1024.0;
    unit_index += 1;
  }

  format!("{:.2} {}", size, UNITS[unit_index])
}

/// 性能测试断言：验证操作在合理时间内完成
pub fn assert_reasonable_time(
  operation: impl Into<String>,
  max_duration: Duration,
  f: impl FnOnce(),
) -> Result<(), TestError> {
  let operation_name = operation.into();
  let start = Instant::now();
  f();
  let duration = start.elapsed();

  if duration <= max_duration {
    println!(
      "✅ {} 在合理时间内完成: {:.2?} (限制: {:.2?})",
      operation_name, duration, max_duration
    );
    Ok(())
  } else {
    let msg = format!(
      "{} 执行时间过长: {:.2?} > {:.2?}",
      operation_name, duration, max_duration
    );
    println!("⚠️ {}", msg);
    Err(TestError::Timeout(msg))
  }
}

/// 简单的性能基准测试宏（用于快速测量）
#[macro_export]
macro_rules! bench {
  ($name:expr, $code:block) => {{
    use std::time::Instant;
    let start = Instant::now();
    let result = $code;
    let duration = start.elapsed();
    println!("⏱️  {}: {:.2?}", $name, duration);
    result
  }};
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::thread;
  use std::time::Duration;

  #[test]
  fn test_performance_measurement_new() {
    // 测试PerformanceMeasurement创建
    let duration = Duration::from_millis(100);
    let measurement = PerformanceMeasurement::new("test_operation", duration);

    assert_eq!(measurement.operation, "test_operation");
    assert_eq!(measurement.duration, duration);
    assert_eq!(measurement.iterations, None);
    assert!(measurement.metrics.is_empty());
  }

  #[test]
  fn test_performance_measurement_with_iterations() {
    // 测试添加迭代次数
    let duration = Duration::from_millis(50);
    let measurement = PerformanceMeasurement::new("iter_test", duration).with_iterations(1000);

    assert_eq!(measurement.iterations, Some(1000));
  }

  #[test]
  fn test_performance_measurement_with_metric() {
    // 测试添加额外指标
    let duration = Duration::from_millis(10);
    let measurement = PerformanceMeasurement::new("metric_test", duration)
      .with_metric("memory_mb", 64.5)
      .with_metric("cpu_percent", 25.0);

    assert_eq!(measurement.metrics.len(), 2);
    assert_eq!(measurement.metrics.get("memory_mb"), Some(&64.5));
    assert_eq!(measurement.metrics.get("cpu_percent"), Some(&25.0));
  }

  #[test]
  fn test_performance_measurement_operations_per_second() {
    // 测试每秒操作数计算
    let duration = Duration::from_secs(2);
    let measurement = PerformanceMeasurement::new("ops_test", duration).with_iterations(1000);

    let ops = measurement.operations_per_second();
    assert!(ops.is_some());
    assert_eq!(ops.unwrap(), 500.0); // 1000次操作 / 2秒 = 500 ops

    // 测试没有迭代次数的情况
    let measurement_no_iter = PerformanceMeasurement::new("no_ops_test", duration);
    assert!(measurement_no_iter.operations_per_second().is_none());
  }

  #[test]
  fn test_performance_measurement_format() {
    // 测试格式化输出
    let duration = Duration::from_millis(123);
    let measurement = PerformanceMeasurement::new("format_test", duration)
      .with_iterations(500)
      .with_metric("error_rate", 0.05);

    let formatted = measurement.format();
    assert!(formatted.contains("操作: format_test"));
    assert!(formatted.contains("耗时: 123"));
    assert!(formatted.contains("迭代次数: 500"));
    assert!(formatted.contains("每秒操作数:"));
    assert!(formatted.contains("error_rate: 0.05"));
  }

  #[test]
  fn test_performance_runner_new() {
    // 测试PerformanceRunner创建
    let runner = PerformanceRunner::new("test_runner");
    assert_eq!(runner.name, "test_runner");
    assert!(runner.measurements().is_empty());
  }

  #[test]
  fn test_performance_runner_measure() {
    // 测试测量单个操作
    let mut runner = PerformanceRunner::new("single_op_runner");
    let result = runner.measure("addition", || 2 + 2);

    assert_eq!(result, 4);
    assert_eq!(runner.measurements().len(), 1);

    let measurement = &runner.measurements()[0];
    assert_eq!(measurement.operation, "addition");
    assert!(measurement.duration < Duration::from_millis(100)); // 应该很快
  }

  #[test]
  fn test_performance_runner_measure_iterations() {
    // 测试测量多次迭代
    let mut runner = PerformanceRunner::new("iter_runner");
    let mut counter = 0;

    runner.measure_iterations("increment", 100, |i| {
      counter += 1;
      assert_eq!(counter, i + 1);
    });

    assert_eq!(counter, 100);
    assert_eq!(runner.measurements().len(), 1);

    let measurement = &runner.measurements()[0];
    assert_eq!(measurement.operation, "increment");
    assert_eq!(measurement.iterations, Some(100));
    assert!(measurement.duration < Duration::from_millis(100)); // 应该很快
  }

  #[test]
  fn test_performance_runner_multiple_measurements() {
    // 测试多个测量
    let mut runner = PerformanceRunner::new("multi_runner");

    runner.measure("op1", || thread::sleep(Duration::from_millis(1)));
    runner.measure("op2", || thread::sleep(Duration::from_millis(2)));
    runner.measure("op3", || thread::sleep(Duration::from_millis(3)));

    assert_eq!(runner.measurements().len(), 3);
    assert!(runner.measurements()[0].operation.contains("op1"));
    assert!(runner.measurements()[1].operation.contains("op2"));
    assert!(runner.measurements()[2].operation.contains("op3"));

    // 验证持续时间是递增的（因为sleep时间递增）
    let durations: Vec<Duration> = runner.measurements().iter().map(|m| m.duration).collect();

    // 注意：由于线程调度的不确定性，我们不能严格保证递增
    // 但第一个应该是最短的（1ms），最后一个应该是最长的（3ms）
    assert!(durations[0] < Duration::from_millis(10)); // 允许一些误差
    assert!(durations[2] < Duration::from_millis(10));
  }

  #[test]
  fn test_format_file_size() {
    // 测试文件大小格式化
    assert_eq!(format_file_size(0), "0.00 B");
    assert_eq!(format_file_size(1023), "1023.00 B");
    assert_eq!(format_file_size(1024), "1.00 KB");
    assert_eq!(format_file_size(1024 * 1024), "1.00 MB");
    assert_eq!(format_file_size(1024 * 1024 * 1024), "1.00 GB");
    assert_eq!(format_file_size(1024u64 * 1024 * 1024 * 1024), "1.00 TB");

    // 测试小数部分
    assert!(format_file_size(1536).contains("1.50 KB")); // 1024 + 512
    assert!(format_file_size(1572864).contains("1.50 MB")); // 1.5 MB
  }

  #[test]
  fn test_assert_reasonable_time_success() {
    // 测试合理时间断言（成功情况）
    let max_duration = Duration::from_millis(100);
    let result = assert_reasonable_time("fast_operation", max_duration, || {
      thread::sleep(Duration::from_millis(10));
    });

    assert!(result.is_ok());
  }

  #[test]
  fn test_assert_reasonable_time_failure() {
    // 测试合理时间断言（失败情况）
    let max_duration = Duration::from_millis(10);
    let result = assert_reasonable_time("slow_operation", max_duration, || {
      thread::sleep(Duration::from_millis(50));
    });

    assert!(result.is_err());

    if let Err(TestError::Timeout(msg)) = result {
      assert!(msg.contains("slow_operation"));
      assert!(msg.contains("执行时间过长"));
    } else {
      panic!("Expected Timeout error");
    }
  }

  #[test]
  fn test_bench_macro() {
    // 测试bench宏
    let result = bench!("macro_test", {
      let x = 10;
      let y = 20;
      x + y
    });

    assert_eq!(result, 30);
    // 宏会打印时间，但我们无法在测试中捕获输出
  }

  #[test]
  fn test_performance_runner_print_report() {
    // 测试打印报告（主要检查没有panic）
    let mut runner = PerformanceRunner::new("report_test");
    runner.measure("test1", || {});
    runner.measure("test2", || {});

    // 应该不会panic
    runner.print_report();
  }

  #[test]
  fn test_performance_measurement_clone() {
    // 测试PerformanceMeasurement的Clone
    let measurement = PerformanceMeasurement::new("clone_test", Duration::from_millis(50))
      .with_iterations(100)
      .with_metric("test", 1.0);

    let cloned = measurement.clone();

    assert_eq!(cloned.operation, measurement.operation);
    assert_eq!(cloned.duration, measurement.duration);
    assert_eq!(cloned.iterations, measurement.iterations);
    assert_eq!(cloned.metrics.len(), measurement.metrics.len());
  }

  #[test]
  fn test_performance_measurement_debug() {
    // 测试Debug实现
    let measurement = PerformanceMeasurement::new("debug_test", Duration::from_millis(75));
    let debug_output = format!("{:?}", measurement);

    assert!(debug_output.contains("debug_test"));
    assert!(debug_output.contains("duration"));
  }

  #[test]
  fn test_performance_runner_measurements_getter() {
    // 测试measurements getter
    let mut runner = PerformanceRunner::new("getter_test");
    runner.measure("op1", || {});
    runner.measure("op2", || {});

    let measurements = runner.measurements();
    assert_eq!(measurements.len(), 2);
    assert_eq!(measurements[0].operation, "op1");
    assert_eq!(measurements[1].operation, "op2");
  }

  #[test]
  fn test_operations_per_second_calculation() {
    // 测试每秒操作数计算的边界情况
    let duration = Duration::from_secs(0); // 零时长
    let measurement = PerformanceMeasurement::new("zero_duration", duration).with_iterations(100);

    // 除以零的情况 - 应该返回无穷大或处理
    let ops = measurement.operations_per_second();
    assert!(ops.is_some());
    assert!(ops.unwrap().is_infinite());
  }
}
