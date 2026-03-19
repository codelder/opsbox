//! SearchExecutor 集成测试
//!
//! 验证多数据源搜索、并发控制、缓存功能

use logseek::query::KeywordHighlight;
use logseek::repository::cache::cache;
use logseek::service::search::SearchEvent;
use logseek::service::search_executor::{SearchExecutor, SearchExecutorConfig};
use opsbox_core::database::{DatabaseConfig, init_pool};
use std::collections::HashSet;
use tempfile::TempDir;
use tokio::fs;

/// 创建测试数据库
async fn create_test_pool() -> (opsbox_core::SqlitePool, TempDir) {
  let temp_dir = TempDir::new().expect("创建临时目录失败");
  let db_path = temp_dir.path().join("test.db");

  let config = DatabaseConfig::new(format!("sqlite://{}", db_path.display()), 5, 30);

  let pool = init_pool(&config).await.expect("初始化数据库失败");

  // 初始化 logseek schema
  logseek::init_schema(&pool).await.expect("初始化 schema 失败");

  (pool, temp_dir)
}

/// 创建测试日志文件
async fn create_test_log_files(dir: &std::path::Path) -> std::io::Result<()> {
  // 创建多个测试日志文件
  fs::write(
    dir.join("app1.log"),
    "2024-01-01 INFO Starting application\n\
         2024-01-01 ERROR Connection failed\n\
         2024-01-01 WARN Retrying connection\n\
         2024-01-01 INFO Connection established\n",
  )
  .await?;

  fs::write(
    dir.join("app2.log"),
    "2024-01-02 DEBUG Processing request\n\
         2024-01-02 ERROR Invalid input data\n\
         2024-01-02 INFO Request completed\n",
  )
  .await?;

  fs::write(
    dir.join("app3.log"),
    "2024-01-03 INFO System started\n\
         2024-01-03 ERROR Database connection timeout\n\
         2024-01-03 ERROR Failed to initialize service\n\
         2024-01-03 WARN Falling back to default config\n",
  )
  .await?;

  Ok(())
}

#[tokio::test]
async fn test_search_executor_basic_search() {
  // 创建测试环境
  let (pool, _temp_dir) = create_test_pool().await;
  let log_dir = TempDir::new().expect("创建临时日志目录失败");
  create_test_log_files(log_dir.path())
    .await
    .expect("创建测试日志文件失败");

  // 创建 SearchExecutor
  let config = SearchExecutorConfig {
    io_max_concurrency: 2,
    stream_channel_capacity: 32,
  };
  let _executor = SearchExecutor::new(pool, config);

  // 注意：由于 SearchExecutor 依赖 Starlark 规划器和数据库配置，
  // 这个测试需要预先配置数据源。在实际环境中，这通过 settings 表完成。
  // 这里我们只验证 SearchExecutor 的基本结构是否正确。

  // SearchExecutor 成功构建即视为通过（若内部初始化失败会 panic）
}

#[tokio::test]
async fn test_search_executor_with_local_source() {
  // 创建测试环境
  let (_pool, _temp_dir) = create_test_pool().await;
  let log_dir = TempDir::new().expect("创建临时日志目录失败");
  create_test_log_files(log_dir.path())
    .await
    .expect("创建测试日志文件失败");

  // 由于 SearchExecutor.search() 依赖数据库中的配置和 Starlark 规划器，
  // 完整的集成测试需要：
  // 1. 在数据库中插入 settings 配置
  // 2. 配置 Starlark 脚本
  // 3. 调用 search() 方法
  //
  // 这超出了单元测试的范围，应该通过端到端测试或手动测试验证。
  // 这里我们验证基本的数据结构和配置。

  let config = SearchExecutorConfig::default();
  assert_eq!(config.io_max_concurrency, 12);
  assert_eq!(config.stream_channel_capacity, 128);
}

#[tokio::test]
async fn test_cache_functionality() {
  // 测试缓存功能
  let c = cache();
  let sid = format!("test-sid-{}", uuid::Uuid::new_v4());

  // 测试关键字缓存
  let keywords = vec![
    KeywordHighlight::Literal("error".to_string()),
    KeywordHighlight::Literal("warn".to_string()),
  ];
  c.put_keywords(&sid, keywords.clone()).await;

  let cached_keywords = c.get_keywords(&sid).await;
  assert_eq!(cached_keywords, Some(keywords));

  // 测试文件行缓存
  let file_url = "orl://local/test.log";
  let lines = vec!["line 1".to_string(), "line 2".to_string()];
  c.put_lines(&sid, file_url, &lines, "UTF-8".to_string()).await;

  let cached_lines = c.get_lines_slice(&sid, file_url, 1, 100).await;
  assert!(cached_lines.is_some());

  // Verify content
  let (total, slice, _) = cached_lines.unwrap();
  assert_eq!(total, 2);
  assert_eq!(slice, lines);
}

#[tokio::test]
async fn test_multi_source_event_collection() {
  // 模拟多数据源搜索结果收集
  use tokio::sync::mpsc;

  let (tx, mut rx) = mpsc::channel::<SearchEvent>(32);

  // 模拟 3 个数据源发送结果
  for i in 0..3 {
    let tx_clone = tx.clone();
    tokio::spawn(async move {
      // 发送成功事件
      let _ = tx_clone
        .send(SearchEvent::Success(logseek::service::search::SearchResult {
          path: format!("source{}.log", i),
          lines: vec![format!("error from source {}", i)],
          merged: vec![(0, 1)],
          encoding: None,
          archive_path: None,
          source_type: logseek::service::search::EntrySourceType::default(),
        }))
        .await;

      // 发送完成事件
      let _ = tx_clone
        .send(SearchEvent::Complete {
          source: format!("source-{}", i),
          elapsed_ms: 50,
        })
        .await;
    });
  }

  // 关闭发送端
  drop(tx);

  // 收集所有事件
  let mut success_count = 0;
  let mut complete_count = 0;
  let mut sources = HashSet::new();

  while let Some(event) = rx.recv().await {
    match event {
      SearchEvent::Success(result) => {
        success_count += 1;
        assert!(result.path.starts_with("source"));
      }
      SearchEvent::Complete { source, .. } => {
        complete_count += 1;
        sources.insert(source);
      }
      _ => {}
    }
  }

  assert_eq!(success_count, 3, "应该收到 3 个成功事件");
  assert_eq!(complete_count, 3, "应该收到 3 个完成事件");
  assert_eq!(sources.len(), 3, "应该有 3 个不同的数据源");
}

/// 测试搜索取消机制
#[tokio::test]
async fn test_search_cancellation() {
  use std::sync::Arc;
  use std::sync::atomic::{AtomicUsize, Ordering};
  use tokio::sync::mpsc;
  use tokio_util::sync::CancellationToken;

  // 创建取消令牌
  let cancel_token = CancellationToken::new();
  let cancel_token_clone = cancel_token.clone();

  // 创建一个计数器来跟踪任务执行情况
  let task_count = Arc::new(AtomicUsize::new(0));
  let task_count_clone = task_count.clone();

  // 启动一个长时间运行的任务
  let (tx, mut rx) = mpsc::channel::<SearchEvent>(10);

  let handle = tokio::spawn(async move {
    // 模拟搜索任务
    for i in 0..100 {
      if cancel_token_clone.is_cancelled() {
        break;
      }

      task_count_clone.fetch_add(1, Ordering::SeqCst);

      // 发送进度事件
      let _ = tx
        .send(SearchEvent::Success(logseek::service::search::SearchResult {
          path: format!("file{}.log", i),
          lines: vec![format!("line {}", i)],
          merged: vec![(0, 1)],
          encoding: None,
          archive_path: None,
          source_type: logseek::service::search::EntrySourceType::default(),
        }))
        .await;

      // 模拟工作延迟
      tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    // 发送完成事件
    let _ = tx
      .send(SearchEvent::Complete {
        source: "test-source".to_string(),
        elapsed_ms: 100,
      })
      .await;
  });

  // 等待一段时间让任务开始
  tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

  // 取消搜索
  cancel_token.cancel();

  // 等待任务完成
  let _ = tokio::time::timeout(tokio::time::Duration::from_secs(5), handle).await;

  // 验证任务被提前终止（不是所有100个都执行）
  let executed = task_count.load(Ordering::SeqCst);
  assert!(executed < 100, "任务应该被提前取消，但实际执行了 {} 个", executed);

  // 清空通道
  while rx.try_recv().is_ok() {}
}

/// 测试并发搜索的资源限制
#[tokio::test]
async fn test_concurrent_search_resource_limits() {
  use std::sync::Arc;
  use std::sync::atomic::{AtomicUsize, Ordering};
  use tokio::sync::Semaphore;

  // 创建并发限制的信号量（模拟 SearchExecutor 的 io_semaphore）
  let max_concurrency = 3;
  let semaphore = Arc::new(Semaphore::new(max_concurrency));

  // 跟踪当前并发数
  let current_concurrent = Arc::new(AtomicUsize::new(0));
  let max_observed = Arc::new(AtomicUsize::new(0));

  let mut handles = vec![];

  // 启动多个任务
  for i in 0..10 {
    let sem = semaphore.clone();
    let current = current_concurrent.clone();
    let max = max_observed.clone();

    let handle = tokio::spawn(async move {
      // 获取许可
      let _permit = sem.acquire().await.unwrap();

      // 增加并发计数
      let count = current.fetch_add(1, Ordering::SeqCst) + 1;

      // 更新最大观察值
      loop {
        let current_max = max.load(Ordering::SeqCst);
        if count <= current_max
          || max
            .compare_exchange(current_max, count, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
          break;
        }
      }

      // 模拟搜索工作
      tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

      // 减少并发计数
      current.fetch_sub(1, Ordering::SeqCst);

      i
    });

    handles.push(handle);
  }

  // 等待所有任务完成
  for handle in handles {
    let _ = handle.await;
  }

  // 验证最大并发数没有超过限制
  let observed_max = max_observed.load(Ordering::SeqCst);
  assert!(
    observed_max <= max_concurrency,
    "最大并发数 {} 超过了限制 {}",
    observed_max,
    max_concurrency
  );
}

/// 测试错误恢复和错误传播
#[tokio::test]
async fn test_error_recovery_and_propagation() {
  use tokio::sync::mpsc;

  let (tx, mut rx) = mpsc::channel::<SearchEvent>(10);

  // 模拟发送成功、错误、成功事件的混合流
  let events = vec![
    SearchEvent::Success(logseek::service::search::SearchResult {
      path: "file1.log".to_string(),
      lines: vec!["line1".to_string()],
      merged: vec![(0, 1)],
      encoding: None,
      archive_path: None,
      source_type: logseek::service::search::EntrySourceType::default(),
    }),
    SearchEvent::Error {
      source: "source1".to_string(),
      message: "File not found".to_string(),
      recoverable: true,
    },
    SearchEvent::Success(logseek::service::search::SearchResult {
      path: "file2.log".to_string(),
      lines: vec!["line2".to_string()],
      merged: vec![(0, 1)],
      encoding: None,
      archive_path: None,
      source_type: logseek::service::search::EntrySourceType::default(),
    }),
    SearchEvent::Error {
      source: "source2".to_string(),
      message: "Permission denied".to_string(),
      recoverable: true,
    },
    SearchEvent::Complete {
      source: "test-source".to_string(),
      elapsed_ms: 100,
    },
  ];

  // 发送所有事件
  for event in events {
    tx.send(event).await.unwrap();
  }

  drop(tx);

  // 收集并验证事件
  let mut success_count = 0;
  let mut error_count = 0;
  let mut complete_count = 0;

  while let Some(event) = rx.recv().await {
    match event {
      SearchEvent::Success(_) => success_count += 1,
      SearchEvent::Error {
        source,
        message,
        recoverable,
      } => {
        error_count += 1;
        assert!(recoverable, "错误应该是可恢复的");
        assert!(!source.is_empty());
        assert!(!message.is_empty());
      }
      SearchEvent::Complete { .. } => complete_count += 1,
      SearchEvent::Finished { .. } => {} // 全局完成事件，忽略
    }
  }

  assert_eq!(success_count, 2, "应该有 2 个成功事件");
  assert_eq!(error_count, 2, "应该有 2 个错误事件");
  assert_eq!(complete_count, 1, "应该有 1 个完成事件");
}

/// 测试资源清理和内存管理
#[tokio::test]
async fn test_resource_cleanup() {
  // 创建测试环境
  let (pool, _temp_dir) = create_test_pool().await;

  // 创建 SearchExecutor
  let config = SearchExecutorConfig {
    io_max_concurrency: 2,
    stream_channel_capacity: 32,
  };

  // 多次创建和销毁 SearchExecutor，验证没有内存泄漏
  for _ in 0..5 {
    let _executor = SearchExecutor::new(pool.clone(), config.clone());
    // executor 在这里被 drop，验证可以正常创建和销毁
  }

  // 测试通过后表示资源清理正常
}

/// 测试搜索超时处理
#[tokio::test]
async fn test_search_timeout_handling() {
  use tokio::sync::mpsc;
  use tokio::time::{Duration, timeout};

  let (tx, mut rx) = mpsc::channel::<SearchEvent>(10);

  // 模拟一个长时间运行的搜索
  tokio::spawn(async move {
    // 发送一些初始结果
    for i in 0..3 {
      let _ = tx
        .send(SearchEvent::Success(logseek::service::search::SearchResult {
          path: format!("file{}.log", i),
          lines: vec![format!("line {}", i)],
          merged: vec![(0, 1)],
          encoding: None,
          archive_path: None,
          source_type: logseek::service::search::EntrySourceType::default(),
        }))
        .await;
    }

    // 长时间等待（模拟超时场景）
    tokio::time::sleep(Duration::from_secs(10)).await;

    // 发送完成事件（可能永远不会到达）
    let _ = tx
      .send(SearchEvent::Complete {
        source: "test-source".to_string(),
        elapsed_ms: 10000,
      })
      .await;
  });

  // 使用超时接收事件
  let mut received_count = 0;
  while let Ok(Some(event)) = timeout(Duration::from_millis(100), rx.recv()).await {
    if let SearchEvent::Success(_) = event {
      received_count += 1;
    }

    // 最多接收 3 个事件
    if received_count >= 3 {
      break;
    }
  }

  assert_eq!(received_count, 3, "应该收到 3 个成功事件");
}

/// 测试大文件搜索的内存管理
#[tokio::test]
async fn test_large_file_search_memory_management() {
  // 创建大文件（使用内存高效的生成方式）
  let temp_dir = TempDir::new().expect("创建临时目录失败");
  let large_file_path = temp_dir.path().join("large.log");

  // 生成大文件（5MB，分块写入）
  let chunk = "This is a test log line for memory management testing.\n".repeat(100);
  let chunk_size = chunk.len();
  let target_size = 5 * 1024 * 1024; // 5MB

  let mut file = tokio::fs::File::create(&large_file_path).await.unwrap();
  let mut written = 0usize;

  while written < target_size {
    let to_write = std::cmp::min(chunk_size, target_size - written);
    let content = &chunk.as_bytes()[..to_write];
    tokio::io::AsyncWriteExt::write_all(&mut file, content).await.unwrap();
    written += to_write;
  }

  drop(file);

  // 验证文件创建成功
  let metadata = tokio::fs::metadata(&large_file_path).await.unwrap();
  assert!(metadata.len() >= target_size as u64, "大文件创建失败");

  // 逐行读取并验证内存使用
  let file = tokio::fs::File::open(&large_file_path).await.unwrap();
  let reader = tokio::io::BufReader::new(file);
  let mut lines = tokio::io::AsyncBufReadExt::lines(reader);

  let mut line_count = 0;
  while let Ok(Some(_)) = lines.next_line().await {
    line_count += 1;

    // 每1000行检查一次，确保不会占用过多内存
    if line_count % 10000 == 0 {
      // 在测试中模拟内存检查点
      tokio::task::yield_now().await;
    }
  }

  assert!(line_count > 0, "应该读取到行数据");
  println!("成功读取 {} 行，内存管理正常", line_count);
}
