//! 性能集成测试
//!
//! 测试关键操作的性能特征，提供基准数据

use opsbox_test_common::{bench, performance::{PerformanceRunner, assert_reasonable_time}};
use regex::Regex;
use std::time::Duration;

/// 测试基本操作性能
#[test]
fn test_basic_operations_performance() {
    let mut runner = PerformanceRunner::new("基本操作性能测试");

    // 测量字符串操作
    runner.measure_iterations("字符串拼接", 1000, |i| {
        let _s = format!("test_{}", i);
    });

    // 测量向量操作
    runner.measure_iterations("向量创建", 100, |_| {
        let _v: Vec<u32> = (0..1000).collect();
    });

    // 测量哈希映射操作
    runner.measure_iterations("哈希映射插入", 100, |i| {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        for j in 0..100 {
            map.insert(format!("key_{}_{}", i, j), j);
        }
    });

    runner.print_report();
}

/// 测试正则表达式性能
#[test]
fn test_regex_performance() {
    let mut runner = PerformanceRunner::new("正则表达式性能测试");

    // 测量正则表达式编译
    runner.measure("编译简单正则表达式", || {
        Regex::new(r"error|warn|info").unwrap()
    });

    // 测量正则表达式匹配
    let re = Regex::new(r"error|warn|info").unwrap();
    let log_line = "2024-01-01 INFO Test log entry with error message";

    runner.measure_iterations("正则表达式匹配", 1000, |_| {
        let _found = re.is_match(log_line);
    });

    runner.print_report();
}

/// 测试合理时间断言
#[test]
fn test_reasonable_time_assertion() {
    // 验证快速操作在合理时间内完成
    let result = assert_reasonable_time(
        "快速计算",
        Duration::from_millis(100),
        || {
            let _sum: u64 = (0..10000).sum();
        },
    );

    assert!(result.is_ok(), "快速计算应该在100毫秒内完成");

    // 测试bench宏
    let value = bench!("简单计算", {
        let mut sum = 0;
        for i in 0..10000 {
            sum += i;
        }
        sum
    });

    assert_eq!(value, 49995000, "计算结果应该正确");
    println!("✅ bench宏测试通过");
}

/// 测试文件I/O性能（同步）
#[test]
fn test_file_io_performance() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let mut runner = PerformanceRunner::new("文件I/O性能测试");

    // 测量文件写入
    runner.measure_iterations("小文件写入", 100, |i| {
        let path = temp_dir.path().join(format!("test_{}.txt", i));
        std::fs::write(&path, "test content").expect("写入文件失败");
    });

    // 测量文件读取
    runner.measure_iterations("小文件读取", 100, |i| {
        let path = temp_dir.path().join(format!("test_{}.txt", i));
        let _content = std::fs::read_to_string(&path).expect("读取文件失败");
    });

    // 测量文件删除
    runner.measure_iterations("文件删除", 100, |i| {
        let path = temp_dir.path().join(format!("test_{}.txt", i));
        std::fs::remove_file(&path).expect("删除文件失败");
    });

    runner.print_report();
}

/// 性能测试示例和文档
#[test]
fn test_performance_examples() {
    println!("📋 性能测试工具使用示例:");
    println!();

    // 示例1: 使用PerformanceRunner
    let mut runner = PerformanceRunner::new("示例测试");

    runner.measure("示例操作", || {
        // 模拟一些工作
        std::thread::sleep(Duration::from_millis(10));
    });

    runner.measure_iterations("迭代示例", 50, |i| {
        let _result = i * 2;
    });

    runner.print_report();

    // 示例2: 使用assert_reasonable_time
    println!("⏱️  合理时间断言示例:");
    match assert_reasonable_time("快速操作", Duration::from_millis(50), || {
        std::thread::sleep(Duration::from_millis(5));
    }) {
        Ok(_) => println!("  ✅ 操作在预期时间内完成"),
        Err(e) => println!("  ⚠️  操作超时: {}", e),
    }

    // 示例3: 使用bench宏
    println!();
    println!("⚡ bench宏示例:");
    let result = bench!("计算平方和", {
        (0..1000).map(|x| x * x).sum::<u32>()
    });
    println!("  结果: {}", result);

    println!();
    println!("🎯 性能测试框架已就绪，可以扩展更多测试!");
}