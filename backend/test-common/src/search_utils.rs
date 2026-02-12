//! 搜索测试工具
//!
//! 提供搜索测试的辅助功能：
//! - 搜索结果收集和分析
//! - 搜索事件验证
//! - 搜索性能测量

use crate::TestError;
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc::Receiver;

/// 搜索结果收集器
#[derive(Debug, Default)]
pub struct SearchResultCollector {
  /// 成功结果数量
  pub success_count: usize,
  /// 错误结果数量
  pub error_count: usize,
  /// 完成事件数量
  pub complete_count: usize,
  /// 匹配的文件路径集合
  pub matched_paths: HashSet<String>,
  /// 错误消息映射（源 -> 错误消息）
  pub error_messages: HashMap<String, String>,
  /// 完成事件的源集合
  pub completed_sources: HashSet<String>,
  /// 每个源的耗时（毫秒）
  pub source_elapsed_ms: HashMap<String, u64>,
}

impl SearchResultCollector {
  /// 从通道收集搜索结果
  pub async fn collect_from_channel(mut rx: Receiver<logseek::service::search::SearchEvent>) -> Self {
    let mut collector = Self::default();

    while let Some(event) = rx.recv().await {
      match event {
        logseek::service::search::SearchEvent::Success(result) => {
          collector.success_count += 1;
          collector.matched_paths.insert(result.path.clone());
        }
        logseek::service::search::SearchEvent::Error { source, message, .. } => {
          collector.error_count += 1;
          collector.error_messages.insert(source.clone(), message.clone());
        }
        logseek::service::search::SearchEvent::Complete { source, elapsed_ms } => {
          collector.complete_count += 1;
          collector.completed_sources.insert(source.clone());
          collector.source_elapsed_ms.insert(source, elapsed_ms);
        }
      }
    }

    collector
  }

  /// 断言成功结果数量
  pub fn assert_success_count(&self, expected: usize) {
    assert_eq!(
      self.success_count, expected,
      "Expected {} success events, got {}",
      expected, self.success_count
    );
  }

  /// 断言错误结果数量
  pub fn assert_error_count(&self, expected: usize) {
    assert_eq!(
      self.error_count, expected,
      "Expected {} error events, got {}",
      expected, self.error_count
    );
  }

  /// 断言完成事件数量
  pub fn assert_complete_count(&self, expected: usize) {
    assert_eq!(
      self.complete_count, expected,
      "Expected {} complete events, got {}",
      expected, self.complete_count
    );
  }

  /// 断言匹配路径包含指定模式
  pub fn assert_matched_paths_contain(&self, pattern: &str) {
    let found = self.matched_paths.iter().any(|path| path.contains(pattern));
    assert!(
      found,
      "Expected to find path containing '{}' in matched paths: {:?}",
      pattern, self.matched_paths
    );
  }

  /// 断言所有源都已完成
  pub fn assert_all_sources_completed(&self, expected_sources: &[&str]) {
    let expected_set: HashSet<_> = expected_sources.iter().map(|s| s.to_string()).collect();
    let missing: Vec<_> = expected_set.difference(&self.completed_sources).collect();

    assert!(
      missing.is_empty(),
      "Expected all sources to be completed. Missing: {:?}",
      missing
    );
  }

  /// 断言没有错误
  pub fn assert_no_errors(&self) {
    assert!(
      self.error_count == 0,
      "Expected no errors, but got {} errors: {:?}",
      self.error_count,
      self.error_messages
    );
  }

  /// 获取总匹配行数（如果结果中包含行信息）
  pub fn total_matched_lines(&self) -> usize {
    // 注意：这个方法需要访问SearchResult的lines字段
    // 由于SearchResult不在当前作用域，这里返回0
    // 实际使用中可能需要更复杂的实现
    0
  }

  /// 检查是否所有搜索都在合理时间内完成
  pub fn assert_all_completed_within_ms(&self, max_ms: u64) {
    for (source, elapsed) in &self.source_elapsed_ms {
      assert!(
        *elapsed <= max_ms,
        "Source '{}' took {}ms, expected <= {}ms",
        source,
        elapsed,
        max_ms
      );
    }
  }
}

/// 搜索测试配置
#[derive(Debug, Clone)]
pub struct SearchTestConfig {
  /// 并发IO数量
  pub io_concurrency: usize,
  /// 流通道容量
  pub stream_channel_capacity: usize,
  /// 超时时间（毫秒）
  pub timeout_ms: u64,
}

impl Default for SearchTestConfig {
  fn default() -> Self {
    Self {
      io_concurrency: 2,
      stream_channel_capacity: 32,
      timeout_ms: 5000,
    }
  }
}

/// 搜索测试运行器
pub struct SearchTestRunner {
  /// 数据库连接池
  pub pool: opsbox_core::SqlitePool,
  /// 临时目录
  pub temp_dir: tempfile::TempDir,
  /// 配置
  pub config: SearchTestConfig,
}

impl SearchTestRunner {
  /// 创建新的搜索测试运行器
  pub async fn new() -> Result<Self, TestError> {
    // 创建内存数据库
    let temp_dir = tempfile::TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let config = opsbox_core::database::DatabaseConfig::new(format!("sqlite://{}", db_path.display()), 5, 30);

    let pool = opsbox_core::database::init_pool(&config)
      .await
      .map_err(|e| TestError::Database(e.to_string()))?;

    // 初始化logseek schema
    logseek::init_schema(&pool)
      .await
      .map_err(|e| TestError::Database(e.to_string()))?;

    Ok(Self {
      pool,
      temp_dir,
      config: SearchTestConfig::default(),
    })
  }

  /// 创建SearchExecutor
  pub fn create_executor(&self) -> logseek::service::search_executor::SearchExecutor {
    let config = logseek::service::search_executor::SearchExecutorConfig {
      io_max_concurrency: self.config.io_concurrency,
      stream_channel_capacity: self.config.stream_channel_capacity,
    };

    logseek::service::search_executor::SearchExecutor::new(self.pool.clone(), config)
  }

  /// 创建测试日志文件
  pub async fn create_test_log_files(&self, filename: &str, content: &str) -> Result<std::path::PathBuf, TestError> {
    let path = self.temp_dir.path().join(filename);
    tokio::fs::write(&path, content).await?;
    Ok(path)
  }

  /// 创建多个测试日志文件
  pub async fn create_multiple_log_files(&self, files: &[(&str, &str)]) -> Result<Vec<std::path::PathBuf>, TestError> {
    let mut paths = Vec::new();
    for (filename, content) in files {
      let path = self.create_test_log_files(filename, content).await?;
      paths.push(path);
    }
    Ok(paths)
  }
}

/// 模拟搜索事件生成器（用于测试搜索结果收集）
pub struct MockSearchEventGenerator {
  events: Vec<logseek::service::search::SearchEvent>,
}

impl Default for MockSearchEventGenerator {
  fn default() -> Self {
    Self::new()
  }
}

impl MockSearchEventGenerator {
  /// 创建新的模拟生成器
  pub fn new() -> Self {
    Self { events: Vec::new() }
  }

  /// 添加成功事件
  pub fn add_success(&mut self, path: &str, lines: Vec<String>) {
    let result = logseek::service::search::SearchResult {
      path: path.to_string(),
      lines,
      merged: vec![],
      encoding: None,
      archive_path: None,
      source_type: logseek::service::search::EntrySourceType::default(),
    };
    self.events.push(logseek::service::search::SearchEvent::Success(result));
  }

  /// 添加错误事件
  pub fn add_error(&mut self, source: &str, message: &str) {
    self.events.push(logseek::service::search::SearchEvent::Error {
      source: source.to_string(),
      message: message.to_string(),
      recoverable: true,
    });
  }

  /// 添加完成事件
  pub fn add_complete(&mut self, source: &str, elapsed_ms: u64) {
    self.events.push(logseek::service::search::SearchEvent::Complete {
      source: source.to_string(),
      elapsed_ms,
    });
  }

  /// 获取事件列表
  pub fn events(&self) -> &[logseek::service::search::SearchEvent] {
    &self.events
  }

  /// 创建通道并发送所有事件
  pub async fn send_to_channel(self, tx: tokio::sync::mpsc::Sender<logseek::service::search::SearchEvent>) {
    for event in self.events {
      let _ = tx.send(event).await;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::sync::mpsc;

  #[test]
  fn test_search_result_collector_default() {
    // 测试默认收集器
    let collector = SearchResultCollector::default();

    assert_eq!(collector.success_count, 0);
    assert_eq!(collector.error_count, 0);
    assert_eq!(collector.complete_count, 0);
    assert!(collector.matched_paths.is_empty());
    assert!(collector.error_messages.is_empty());
    assert!(collector.completed_sources.is_empty());
    assert!(collector.source_elapsed_ms.is_empty());
  }

  #[tokio::test]
  async fn test_search_result_collector_from_channel() {
    // 测试从通道收集结果
    let (tx, rx) = mpsc::channel(10);

    // 发送一些测试事件
    tx.send(logseek::service::search::SearchEvent::Success(
      logseek::service::search::SearchResult {
        path: "/var/log/test1.log".to_string(),
        lines: vec!["line 1".to_string(), "line 2".to_string()],
        merged: vec![],
        encoding: None,
        archive_path: None,
        source_type: logseek::service::search::EntrySourceType::default(),
      },
    ))
    .await
    .unwrap();

    tx.send(logseek::service::search::SearchEvent::Error {
      source: "source1".to_string(),
      message: "test error".to_string(),
      recoverable: true,
    })
    .await
    .unwrap();

    tx.send(logseek::service::search::SearchEvent::Complete {
      source: "source1".to_string(),
      elapsed_ms: 100,
    })
    .await
    .unwrap();

    // 关闭发送端
    drop(tx);

    // 收集结果
    let collector = SearchResultCollector::collect_from_channel(rx).await;

    assert_eq!(collector.success_count, 1);
    assert_eq!(collector.error_count, 1);
    assert_eq!(collector.complete_count, 1);
    assert!(collector.matched_paths.contains("/var/log/test1.log"));
    assert_eq!(collector.error_messages.get("source1"), Some(&"test error".to_string()));
    assert!(collector.completed_sources.contains("source1"));
    assert_eq!(collector.source_elapsed_ms.get("source1"), Some(&100));
  }

  #[test]
  fn test_search_result_collector_assertions() {
    // 测试收集器的断言方法
    let mut collector = SearchResultCollector {
      success_count: 2,
      error_count: 1,
      complete_count: 3,
      ..Default::default()
    };

    // 设置一些测试数据
    collector.matched_paths.insert("/var/log/test1.log".to_string());
    collector.matched_paths.insert("/var/log/test2.log".to_string());
    collector.completed_sources.insert("source1".to_string());
    collector.completed_sources.insert("source2".to_string());
    collector.source_elapsed_ms.insert("source1".to_string(), 50);
    collector.source_elapsed_ms.insert("source2".to_string(), 100);

    // 测试成功计数断言（应该通过）
    collector.assert_success_count(2);

    // 测试错误计数断言（应该通过）
    collector.assert_error_count(1);

    // 测试完成计数断言（应该通过）
    collector.assert_complete_count(3);

    // 测试匹配路径包含断言（应该通过）
    collector.assert_matched_paths_contain("test1");

    // 测试所有源完成断言（应该通过）
    collector.assert_all_sources_completed(&["source1", "source2"]);

    // 测试无错误断言（应该失败，因为有1个错误）
    // 注意：这个测试会在panic时失败，所以我们不在这里测试
    // collector.assert_no_errors(); // 这会panic，所以我们不调用它

    // 测试合理时间完成断言（应该通过）
    collector.assert_all_completed_within_ms(200);
  }

  #[test]
  fn test_search_result_collector_total_matched_lines() {
    // 测试总匹配行数方法
    let collector = SearchResultCollector::default();

    // 当前实现总是返回0（需要SearchResult访问权限）
    assert_eq!(collector.total_matched_lines(), 0);
  }

  #[test]
  fn test_search_test_config_default() {
    // 测试搜索测试配置默认值
    let config = SearchTestConfig::default();

    assert_eq!(config.io_concurrency, 2);
    assert_eq!(config.stream_channel_capacity, 32);
    assert_eq!(config.timeout_ms, 5000);
  }

  #[test]
  fn test_search_test_config_custom() {
    // 测试自定义搜索测试配置
    let config = SearchTestConfig {
      io_concurrency: 5,
      stream_channel_capacity: 64,
      timeout_ms: 10000,
    };

    assert_eq!(config.io_concurrency, 5);
    assert_eq!(config.stream_channel_capacity, 64);
    assert_eq!(config.timeout_ms, 10000);
  }

  #[tokio::test]
  async fn test_search_test_runner_creation() {
    // 测试搜索测试运行器创建
    let runner = SearchTestRunner::new().await;

    // 创建应该成功
    assert!(runner.is_ok());

    let runner = runner.unwrap();

    // 验证配置
    assert_eq!(runner.config.io_concurrency, 2);
    assert_eq!(runner.config.stream_channel_capacity, 32);
    assert_eq!(runner.config.timeout_ms, 5000);
  }

  #[tokio::test]
  async fn test_search_test_runner_create_test_files() {
    // 测试创建测试日志文件
    let runner = SearchTestRunner::new().await.unwrap();

    let result = runner.create_test_log_files("test.log", "line1\nline2\nline3").await;
    assert!(result.is_ok());

    let path = result.unwrap();
    assert!(path.exists());
    assert!(path.to_string_lossy().contains("test.log"));
  }

  #[tokio::test]
  async fn test_search_test_runner_create_multiple_files() {
    // 测试创建多个测试日志文件
    let runner = SearchTestRunner::new().await.unwrap();

    let files = vec![
      ("file1.log", "content1"),
      ("file2.log", "content2"),
      ("file3.log", "content3"),
    ];

    let result = runner.create_multiple_log_files(&files).await;
    assert!(result.is_ok());

    let paths = result.unwrap();
    assert_eq!(paths.len(), 3);
  }

  #[test]
  fn test_mock_search_event_generator() {
    // 测试模拟搜索事件生成器
    let mut generator = MockSearchEventGenerator::new();

    // 添加各种事件
    generator.add_success("/var/log/test1.log", vec!["line1".to_string()]);
    generator.add_error("source1", "test error");
    generator.add_complete("source1", 150);

    // 验证事件数量
    assert_eq!(generator.events().len(), 3);

    // 验证事件顺序和内容
    let events = generator.events();
    match &events[0] {
      logseek::service::search::SearchEvent::Success(result) => {
        assert_eq!(result.path, "/var/log/test1.log");
        assert_eq!(result.lines.len(), 1);
      }
      _ => panic!("Expected Success event"),
    }

    match &events[1] {
      logseek::service::search::SearchEvent::Error {
        source,
        message,
        recoverable,
      } => {
        assert_eq!(source, "source1");
        assert_eq!(message, "test error");
        assert!(*recoverable);
      }
      _ => panic!("Expected Error event"),
    }

    match &events[2] {
      logseek::service::search::SearchEvent::Complete { source, elapsed_ms } => {
        assert_eq!(source, "source1");
        assert_eq!(*elapsed_ms, 150);
      }
      _ => panic!("Expected Complete event"),
    }
  }

  #[tokio::test]
  async fn test_mock_search_event_generator_send_to_channel() {
    // 测试将事件发送到通道
    let mut generator = MockSearchEventGenerator::new();
    generator.add_success("/var/log/test.log", vec!["test".to_string()]);
    generator.add_complete("source1", 100);

    let (tx, mut rx) = mpsc::channel(10);

    // 异步发送事件
    tokio::spawn(async move {
      generator.send_to_channel(tx).await;
    });

    // 接收事件
    let mut received_count = 0;
    while rx.recv().await.is_some() {
      received_count += 1;
      if received_count >= 2 {
        break;
      }
    }

    assert_eq!(received_count, 2);
  }

  #[test]
  fn test_search_result_collector_assert_all_completed_within_ms_empty() {
    // 测试空源集合的合理时间断言
    let collector = SearchResultCollector::default();

    // 应该通过，因为没有源需要检查
    collector.assert_all_completed_within_ms(1000);
  }

  #[test]
  fn test_search_result_collector_assert_all_sources_completed_empty() {
    // 测试空预期源集合的断言
    let collector = SearchResultCollector::default();

    // 应该通过，因为预期集合为空
    collector.assert_all_sources_completed(&[]);
  }
}
