//! 性能测试工具
//!
//! 提供轻量级性能测试和基准测量工具

use std::time::{Duration, Instant};
use crate::TestError;

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
        self.iterations.map(|iter| {
            iter as f64 / self.duration.as_secs_f64()
        })
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
    pub fn measure_iterations<F>(
        &mut self,
        operation: impl Into<String>,
        iterations: usize,
        mut f: F,
    ) where
        F: FnMut(usize) -> (),
    {
        let operation_name = operation.into();
        let start = Instant::now();

        for i in 0..iterations {
            f(i);
        }

        let duration = start.elapsed();

        let measurement = PerformanceMeasurement::new(operation_name, duration)
            .with_iterations(iterations);

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
        let total_duration: Duration = self.measurements.iter()
            .map(|m| m.duration)
            .sum();
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
        println!("✅ {} 在合理时间内完成: {:.2?} (限制: {:.2?})",
                 operation_name, duration, max_duration);
        Ok(())
    } else {
        let msg = format!("{} 执行时间过长: {:.2?} > {:.2?}",
                          operation_name, duration, max_duration);
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