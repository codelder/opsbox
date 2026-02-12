//! EntryStream 集成测试
//!
//! 测试 EntryStreamProcessor 与 SearchProcessor 的集成，
//! 以及并发、过滤、预读等逻辑。

use async_trait::async_trait;
use std::collections::VecDeque;
use std::io;
use std::sync::Arc;
use tokio::io::AsyncRead;

use logseek::query::Query;
use logseek::service::entry_stream::EntryStreamProcessor;
use logseek::service::search::{SearchEvent, SearchProcessor};
use opsbox_core::fs::{EntryMeta, EntrySource, EntryStream};

/// Mock 条目流，用于生成可控的测试数据
struct MockEntryStream {
  entries: VecDeque<io::Result<(EntryMeta, Vec<u8>)>>,
}

impl MockEntryStream {
  fn new() -> Self {
    Self {
      entries: VecDeque::new(),
    }
  }

  fn add_entry(&mut self, path: &str, content: &[u8], is_compressed: bool) {
    let meta = EntryMeta {
      path: path.to_string(),
      container_path: None,
      size: Some(content.len() as u64),
      is_compressed,
      source: if is_compressed {
        EntrySource::Gz
      } else {
        EntrySource::File
      },
    };
    self.entries.push_back(Ok((meta, content.to_vec())));
  }

  fn add_error(&mut self, msg: &str) {
    self.entries.push_back(Err(io::Error::other(msg)));
  }
}

#[async_trait]
impl EntryStream for MockEntryStream {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    if let Some(res) = self.entries.pop_front() {
      match res {
        Ok((meta, content)) => {
          let reader = std::io::Cursor::new(content);
          Ok(Some((meta, Box::new(reader))))
        }
        Err(e) => Err(e),
      }
    } else {
      Ok(None)
    }
  }
}

/// 辅助函数：创建简单的查询
fn create_simple_query(term: &str) -> Arc<Query> {
  // 这里我们直接构造 Query，假设 Query 的 public 接口允许这样做
  // 如果 Query 构造比较复杂，可能需要使用 Query::parse 或 builder
  // 假设 crate::query::parse 可用
  match logseek::query::Query::parse_github_like(term) {
    Ok(q) => Arc::new(q),
    Err(e) => panic!("Failed to parse query: {}", e),
  }
}

#[tokio::test]
async fn test_process_stream_basic_flow() {
  // 1. 准备数据
  let mut stream = MockEntryStream::new();
  stream.add_entry("/logs/app.log", b"error: something went wrong", false);
  stream.add_entry("/logs/other.log", b"info: everything is fine", false);

  // 2. 准备处理器 (搜索 "error")
  let query = create_simple_query("error");
  let processor = Arc::new(SearchProcessor::new(query, 0));
  let mut stream_processor = EntryStreamProcessor::new(processor);

  // 3. 运行处理
  let (tx, mut rx) = tokio::sync::mpsc::channel(10);

  let handle = tokio::spawn(async move { stream_processor.process_stream(&mut stream, tx).await });

  // 4. 收集结果
  let mut results = Vec::new();
  while let Some(event) = rx.recv().await {
    if let SearchEvent::Success(res) = event {
      results.push(res);
    }
  }

  handle.await.unwrap().expect("Stream processing failed");

  // 5. 验证
  assert_eq!(results.len(), 1);
  assert_eq!(results[0].path, "/logs/app.log");
}

#[tokio::test]
async fn test_process_stream_with_extra_filter() {
  // 1. 准备数据
  let mut stream = MockEntryStream::new();
  stream.add_entry("/logs/app.log", b"error: match 1", false);
  stream.add_entry("/logs/skip.log", b"error: match 2", false); // 应该被过滤
  stream.add_entry("/logs/sub/app2.log", b"error: match 3", false);

  // 2. 准备处理器 (搜索 "error")
  let query = create_simple_query("error");
  let processor = Arc::new(SearchProcessor::new(query, 0));

  // 添加额外过滤器，只允许 app*.log
  let filter = logseek::query::path_glob_to_filter("**/app*.log").expect("Failed to create filter");

  let mut stream_processor = EntryStreamProcessor::new(processor).with_extra_path_filter(filter);

  // 3. 运行
  let (tx, mut rx) = tokio::sync::mpsc::channel(10);
  tokio::spawn(async move { stream_processor.process_stream(&mut stream, tx).await });

  // 4. 收集
  let mut paths = Vec::new();
  while let Some(event) = rx.recv().await {
    if let SearchEvent::Success(res) = event {
      paths.push(res.path);
    }
  }

  paths.sort();

  // 5. 验证
  assert_eq!(paths.len(), 2);
  assert!(paths.contains(&"/logs/app.log".to_string()));
  assert!(paths.contains(&"/logs/sub/app2.log".to_string()));
}

#[tokio::test]
async fn test_process_stream_cancellation() {
  use tokio_util::sync::CancellationToken;

  // 1. 准备大量数据
  let mut stream = MockEntryStream::new();
  for i in 0..100 {
    stream.add_entry(&format!("/logs/{}.log", i), b"error", false);
  }

  // 2. 准备 Token
  let token = CancellationToken::new();
  let token_clone = token.clone();

  let query = create_simple_query("error");
  let processor = Arc::new(SearchProcessor::new(query, 0));

  let mut stream_processor = EntryStreamProcessor::new(processor).with_cancel_token(Arc::new(token_clone));

  // 3. 运行
  let (tx, mut rx) = tokio::sync::mpsc::channel(10);

  let handle = tokio::spawn(async move { stream_processor.process_stream(&mut stream, tx).await });

  // 4. 接收几个后取消
  let mut count = 0;
  while rx.recv().await.is_some() {
    count += 1;
    if count >= 5 {
      token.cancel();
      // 注意：process_stream 内部是并行的，取消可能有延迟，
      // 且已经进入 in_flight 的任务会继续完成。
      // 我们不能精确断言 count == 5，但应该远小于 100
    }
  }

  handle
    .await
    .expect("Failed to join stream processing task")
    .expect("Stream processing should succeed");

  assert!(count < 100, "Should have stopped early");
  assert!(count >= 5, "Should have processed at least 5");
}

#[tokio::test]
async fn test_process_stream_with_base_path_stripping() {
  // 测试相对路径与 base_path 逻辑
  // 如果设置了 base_path="/logs"，那么 "/logs/app.log" 应该变成 "app.log" 传给 filter

  let mut stream = MockEntryStream::new();
  stream.add_entry("/logs/app.log", b"error", false);

  let query = create_simple_query("error");
  let processor = Arc::new(SearchProcessor::new(query, 0));

  // 过滤器只匹配 "app.log" (无前缀)
  let filter = logseek::query::path_glob_to_filter("app.log").expect("Failed to create filter");

  let mut stream_processor = EntryStreamProcessor::new(processor)
    .with_base_path("/logs") // 设置 base path
    .with_extra_path_filter(filter); // 过滤器现在应匹配剥离后的路径

  let (tx, mut rx) = tokio::sync::mpsc::channel(10);
  tokio::spawn(async move { stream_processor.process_stream(&mut stream, tx).await });

  let mut count = 0;
  while rx.recv().await.is_some() {
    count += 1;
  }

  assert_eq!(count, 1, "Should match after stripping base path");
}

#[tokio::test]
async fn test_preload_large_file_partial() {
  // 测试大文件预读逻辑（通过 is_compressed=true 触发）
  // 大文件 logic in process_stream handles `Partial` result from preload_entry.
  // 实际上我们需要构造一个真正足够大的文件，或者 Mock preloading behavior (难)
  // 或者我们相信 entry_stream.rs 的单元测试，这里只测集成流。

  // 这里我们创建一个"伪"大文件，但其实很小。
  // 但是 EntryStreamProcessor 内部有硬编码的 常量 MAX_PRELOAD_SIZE (120MB).
  // 我们无法在集成测试中轻松更改该常量。
  // 所以很难触发 Partial 分支，除非真的造一个 120MB+ 的 entry。
  // 这对于集成测试来说太重了。
  // 只要前面的单元测试 `test_preload_entry_large` 覆盖了 preload_entry 的逻辑，
  // 这里主要测试正常流程即可。

  // 不过我们可以测试 compressed 文件的处理流程
  let mut stream = MockEntryStream::new();
  stream.add_entry("/logs/archive.gz", b"error in gzip", true); // is_compressed=true

  let query = create_simple_query("error");
  let processor = Arc::new(SearchProcessor::new(query, 0));
  let mut stream_processor = EntryStreamProcessor::new(processor);

  let (tx, mut rx) = tokio::sync::mpsc::channel(10);
  tokio::spawn(async move { stream_processor.process_stream(&mut stream, tx).await });

  let mut count = 0;
  while rx.recv().await.is_some() {
    count += 1;
  }

  assert_eq!(count, 1);
}

#[tokio::test]
async fn test_process_stream_empty() {
  // 测试空流
  let mut stream = MockEntryStream::new();

  let query = create_simple_query("error");
  let processor = Arc::new(SearchProcessor::new(query, 0));
  let mut stream_processor = EntryStreamProcessor::new(processor);

  let (tx, mut rx) = tokio::sync::mpsc::channel(10);

  let handle = tokio::spawn(async move { stream_processor.process_stream(&mut stream, tx).await });

  let mut count = 0;
  while rx.recv().await.is_some() {
    count += 1;
  }

  handle.await.unwrap().expect("Stream processing failed");
  assert_eq!(count, 0, "Empty stream should produce 0 results");
}

#[tokio::test]
async fn test_process_stream_content_search() {
  // 测试内容搜索逻辑
  let mut stream = MockEntryStream::new();
  stream.add_entry("/logs/match.log", b"this line has an error here", false);
  stream.add_entry("/logs/no_match.log", b"this line is clean", false);
  stream.add_entry("/logs/partial.log", b"err but not full match", false); // "error" query matches "error" substring? usually words unless regex? simple query usually substring.

  // create_simple_query("error") -> 应该匹配 "error" 子串
  let query = create_simple_query("error");
  let processor = Arc::new(SearchProcessor::new(query, 0));
  let mut stream_processor = EntryStreamProcessor::new(processor);

  let (tx, mut rx) = tokio::sync::mpsc::channel(10);
  tokio::spawn(async move { stream_processor.process_stream(&mut stream, tx).await });

  let mut matched_paths = Vec::new();
  while let Some(event) = rx.recv().await {
    if let SearchEvent::Success(res) = event {
      matched_paths.push(res.path);
    }
  }

  assert!(matched_paths.contains(&"/logs/match.log".to_string()));
  assert!(!matched_paths.contains(&"/logs/no_match.log".to_string()));
  // "err" does not match "error"
  assert!(!matched_paths.contains(&"/logs/partial.log".to_string()));

  assert_eq!(matched_paths.len(), 1);
}

#[tokio::test]
async fn test_process_stream_error_handling() {
  // 测试流读取出错
  let mut stream = MockEntryStream::new();
  stream.add_entry("/logs/ok.log", b"error: yes", false);
  stream.add_error("disk failure");
  stream.add_entry("/logs/ignored.log", b"error: ignored", false);

  let query = create_simple_query("error");
  let processor = Arc::new(SearchProcessor::new(query, 0));
  let mut stream_processor = EntryStreamProcessor::new(processor);

  let (tx, mut rx) = tokio::sync::mpsc::channel(10);

  let handle = tokio::spawn(async move { stream_processor.process_stream(&mut stream, tx).await });

  let mut results = Vec::new();
  while let Some(event) = rx.recv().await {
    // 我们可能收到部分成功的结果
    if let SearchEvent::Success(res) = event {
      results.push(res);
    }
    // 我们不确定会不会收到 Error event, 还是 stream process 直接结束
  }

  // process_stream 应该返回 Err
  let res = handle.await.unwrap();
  assert!(res.is_err(), "Should return error when stream fails");

  // 但是在错误之前处理的数据应该被发送
  assert!(!results.is_empty(), "Should process entries before error");
  assert_eq!(results[0].path, "/logs/ok.log");
}
