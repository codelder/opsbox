use std::{
  io::{self},
  sync::Arc,
};

use async_compression::tokio::bufread::GzipDecoder;
use async_tar::Archive as AsyncArchive;
use async_trait::async_trait;
use futures::StreamExt;
// use futures::io::AsyncReadExt as FuturesAsyncReadExt;
use thiserror::Error;
use tokio::{
  fs,
  io::{AsyncRead, BufReader},
  sync::Semaphore,
  task::JoinSet,
};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use log::{debug, info, warn, error};

#[derive(Debug, Error)]
pub enum SearchError {
  #[error("IO错误: {0}")]
  Io(#[from] io::Error),
}

use crate::query::Query;

#[async_trait]
pub trait Search {
  async fn search(
    self,
    spec: &Query,
    context_lines: usize,
  ) -> Result<tokio::sync::mpsc::Receiver<SearchResult>, SearchError>;
}

fn is_probably_text_bytes(sample: &[u8]) -> bool {
  if sample.is_empty() {
    return true;
  }
  if sample.contains(&0) {
    return false;
  }
  let printable = sample
    .iter()
    .filter(|b| matches!(**b, 0x09 | 0x0A | 0x0D | 0x20..=0x7E))
    .count();
  let ratio = printable as f32 / sample.len() as f32;
  if ratio >= 0.95 {
    return true;
  }
  std::str::from_utf8(sample).is_ok()
}

pub async fn grep_context<R: AsyncRead + Unpin>(
  reader: &mut R,
  spec: &crate::query::Query,
  context_lines: usize,
) -> Result<Option<(Vec<String>, Vec<(usize, usize)>)>, SearchError> {
  debug!("开始文本搜索，上下文行数: {}, 搜索条件数: {}", context_lines, spec.terms.len());
  
  // 逐行读取，边采样边判断是否文本，避免整文件读取
  use tokio::io::AsyncBufReadExt as _;
  let mut buf_reader = BufReader::new(reader);
  let mut lines: Vec<String> = Vec::new();
  let mut sample: Vec<u8> = Vec::with_capacity(4096);
  let mut sample_checked = false;
  let mut line = String::new();
  loop {
    line.clear();
    let n = buf_reader.read_line(&mut line).await?;
    if n == 0 {
      break;
    }
    if sample.len() < 4096 {
      let bytes = line.as_bytes();
      let take = (4096 - sample.len()).min(bytes.len());
      sample.extend_from_slice(&bytes[..take]);
    }
    if !sample_checked && sample.len() >= 512 {
      if !is_probably_text_bytes(&sample) {
        debug!("文件不是文本格式，跳过搜索");
        return Ok(None);
      }
      debug!("文件确认为文本格式，继续搜索");
      sample_checked = true;
    }
    let trimmed = line.trim_end_matches(['\r', '\n']);
    lines.push(trimmed.to_string());
  }
  if !sample_checked {
    if !is_probably_text_bytes(&sample) {
      debug!("最终样本检查：文件不是文本格式");
      return Ok(None);
    }
    debug!("最终样本检查：确认为文本文件");
  }

  debug!("读取完成，共{}'行，开始执行搜索逻辑", lines.len());
  
  // 文件级布尔计算：检查各关键字是否在文件中出现
  let term_count = spec.terms.len();
  if term_count == 0 {
    warn!("搜索条件为空，返回无结果");
    return Ok(None);
  }
  let mut occurs: Vec<bool> = vec![false; term_count];
  let positive_indices = spec.positive_term_indices();

  let mut matched_lines: Vec<usize> = Vec::new();

  for (idx, line) in lines.iter().enumerate() {
    let mut line_positive = false;
    for (ti, term) in spec.terms.iter().enumerate() {
      if !occurs[ti] && term.matches(line) {
        occurs[ti] = true;
      }
    }
    // 若该行命中任一正向关键字，则收录
    for &pi in &positive_indices {
      if spec.terms.get(pi).map(|t| t.matches(line)).unwrap_or(false) {
        line_positive = true;
        break;
      }
    }
    if line_positive {
      matched_lines.push(idx);
    }
  }

  // 文件级布尔求值
  debug!("执行文件级布尔计算，关键字出现状态: {:?}", occurs);
  if !spec.eval_file(&occurs) {
    debug!("文件级布尔求值不满足，跳过文件");
    return Ok(None);
  }

  if matched_lines.is_empty() {
    debug!("无匹配行，跳过文件");
    return Ok(None);
  }
  
  info!("找到{}行匹配结果，开始生成上下文区间", matched_lines.len());

  let mut ranges: Vec<(usize, usize)> = Vec::new();
  for idx in matched_lines.into_iter() {
    let s = idx.saturating_sub(context_lines);
    let e = std::cmp::min(idx + context_lines, lines.len().saturating_sub(1));
    ranges.push((s, e));
  }

  ranges.sort_by_key(|r| r.0);
  let mut merged: Vec<(usize, usize)> = Vec::new();
  for (s, e) in ranges {
    if let Some(last) = merged.last_mut() {
      if s <= last.1 + 1 {
        if e > last.1 {
          last.1 = e;
        }
        continue;
      }
    }
    merged.push((s, e));
  }

  Ok(Some((lines, merged)))
}

#[derive(Debug)]
pub struct SearchResult {
  pub path: String,
  pub lines: Vec<String>,
  pub merged: Vec<(usize, usize)>,
}

impl SearchResult {
  fn new(path: String, lines: Vec<String>, merged: Vec<(usize, usize)>) -> Self {
    Self { path, lines, merged }
  }
}

#[async_trait]
impl Search for tokio::fs::ReadDir {
  async fn search(
    self,
    spec: &Query,
    context_lines: usize,
  ) -> Result<tokio::sync::mpsc::Receiver<SearchResult>, SearchError> {
    let (tx, rx) = tokio::sync::mpsc::channel::<SearchResult>(128);

    // 在各任务间共享查询规格
    let spec_arc = Arc::new(spec.clone());

    let max_concurrency = std::thread::available_parallelism()
      .map(|n| n.get())
      .unwrap_or(4)
      .saturating_mul(2)
      .min(256);
    let semaphore = Arc::new(Semaphore::new(max_concurrency));

    tokio::spawn({
      let mut stack = vec![self];
      let spec_outer = Arc::clone(&spec_arc);
      let semaphore = Arc::clone(&semaphore);
      let tx = tx.clone();

      async move {
        let mut tasks = JoinSet::new();

        while let Some(mut rd) = stack.pop() {
          loop {
            match rd.next_entry().await {
              Ok(Some(entry)) => {
                let path = entry.path();

                // 如已指定路径过滤器，则尽早应用
                let path_str = path.to_string_lossy();
                if !spec_outer.path_filter.is_allowed(path_str.as_ref()) {
                  continue;
                }

                // 安全起见：跳过符号链接
                let fty = match entry.file_type().await {
                  Ok(t) => t,
                  Err(_) => continue, // 忽略该项，继续
                };
                if fty.is_symlink() {
                  continue;
                }
                if fty.is_dir() {
                  if let Ok(sub) = fs::read_dir(&path).await {
                    stack.push(sub);
                  }
                  continue;
                }
                if !fty.is_file() {
                  continue;
                }

                // 在 spawn 之前 acquire，避免 spawn 风暴
                let permit = match semaphore.clone().acquire_owned().await {
                  Ok(p) => p,
                  Err(_) => break, // 信号量被关闭
                };

                let txf = tx.clone();
                let spec_local = Arc::clone(&spec_outer);

                tasks.spawn(async move {
                  let _permit = permit; // 持有期间占用并发额度
                  if let Ok(file) = fs::File::open(&path).await {
                    let mut reader = BufReader::new(file);
                    if let Ok(Some((lines, merged))) = grep_context(&mut reader, &spec_local, context_lines).await {
                      let _ = txf
                        .send(SearchResult::new(path.to_string_lossy().into_owned(), lines, merged))
                        .await;
                    }
                  }
                });
              }
              Ok(None) => break, // 当前目录读完
              Err(_) => break,   // 该目录出错，跳过
            }
          }
        }

        // 等待所有文件任务结束
        while tasks.join_next().await.is_some() {}

        // 彻底关闭发送端，通知接收者结束
        drop(tx);

        // 不把错误冒泡给 JoinHandle 的使用者，避免惊扰外层
        Ok::<(), ()>(())
      }
    });

    Ok(rx)
  }
}

trait SearchiableAsyncReader: AsyncRead + Send + Unpin + 'static {}
impl SearchiableAsyncReader for tokio::fs::File {}
impl SearchiableAsyncReader for Box<dyn AsyncRead + Send + Unpin> {}

// 全异步：对 AsyncRead (如 S3 流) 进行 gzip 解压与 tar 迭代
#[async_trait]
impl<T> Search for T
where
  T: SearchiableAsyncReader,
{
  async fn search(
    self,
    spec: &Query,
    context_lines: usize,
  ) -> Result<tokio::sync::mpsc::Receiver<SearchResult>, SearchError> {
    let (tx, rx) = tokio::sync::mpsc::channel::<SearchResult>(8);
    let spec_owned = spec.clone();

    tokio::spawn(async move {
      let gz = GzipDecoder::new(BufReader::new(self));
      //:TODO AsyncRead 不一定是 tar 格式，需要检查
      let archive = AsyncArchive::new(gz.compat());
      let Ok(mut entries) = archive.entries() else {
        error!("无法创建 tar 归档条目迭代器");
        return;
      };

      // 错误检测和处理状态
      let mut error_tracker = TarErrorTracker::new();
      let processing_start = std::time::Instant::now();
      let max_processing_time = std::time::Duration::from_secs(300); // 5分钟超时
      // let max_entries = 10_000; // 最大处理条目数
      
      let mut processed_entries = 0;
      let mut successful_entries = 0;

      loop {
        // 1. 检查超时和限制
        if processing_start.elapsed() > max_processing_time {
          warn!("tar 流处理超时 ({}s)，已处理 {} 个条目", max_processing_time.as_secs(), processed_entries);
          break;
        }
        // if processed_entries >= max_entries {
        //   warn!("达到最大条目数限制 ({}), 终止处理", max_entries);
        //   break;
        // }

        // 2. 为单次条目读取添加超时保护
        let entry_timeout = std::time::Duration::from_secs(30);
        let entry_result = match tokio::time::timeout(entry_timeout, entries.next()).await {
          Ok(Some(entry_res)) => entry_res,
          Ok(None) => {
            info!("tar 流结束，共处理 {} 个条目，成功 {} 个", processed_entries, successful_entries);
            break;
          }
          Err(_) => {
            error!("单个条目读取超时 ({}s)，可能卡在损坏的条目上", entry_timeout.as_secs());
            break;
          }
        };

        processed_entries += 1;

        // 3. 智能错误处理
        let entry = match entry_result {
          Ok(entry) => {
            error_tracker.record_success();
            successful_entries += 1;
            entry
          }
          Err(error) => {
            let action = error_tracker.analyze_error(&error);
            
            match action {
              ErrorAction::AbortProcessing => {
                error!("检测到致命错误模式，终止处理: {}", error);
                break;
              }
              ErrorAction::RetryWithBackoff { delay, attempt } => {
                warn!("重试第 {} 次 (延迟 {:?}): {}", attempt, delay, error);
                tokio::time::sleep(delay).await;
                continue;
              }
              ErrorAction::SkipEntry => {
                warn!("跳过损坏条目 (第 {} 个): {}", processed_entries, error);
                continue;
              }
            }
          }
        };

        // 4. 处理有效条目
        let path = match entry.path() {
          Ok(p) => p.to_string_lossy().to_string(),
          Err(e) => {
            warn!("无法获取条目路径: {}, 使用空路径", e);
            format!("<unknown-{}-{}>", processed_entries, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis())
          }
        };

        // 针对 tar 条目的路径过滤
        if !spec_owned.path_filter.is_allowed(&path) {
          debug!("路径不符合过滤条件，跳过: {}", path);
          continue;
        }

        // 5. 为条目内容处理添加超时保护
        let mut entry_compat = entry.compat();
        let content_timeout = std::time::Duration::from_secs(60);
        
        let grep_result = tokio::time::timeout(
          content_timeout,
          grep_context(&mut entry_compat, &spec_owned, context_lines)
        ).await;

        match grep_result {
          Ok(Ok(Some((lines, merged)))) => {
            debug!("成功处理条目: {}, 匹配行数: {}", path, merged.len());
            if tx.send(SearchResult::new(path, lines, merged)).await.is_err() {
              debug!("接收端已关闭，停止处理");
              break;
            }
          }
          Ok(Ok(None)) => {
            debug!("条目无匹配结果: {}", path);
          }
          Ok(Err(e)) => {
            warn!("处理条目内容时出错: {}, 路径: {}", e, path);
          }
          Err(_) => {
            warn!("处理条目内容超时 ({}s): {}", content_timeout.as_secs(), path);
          }
        }
      }
      
      info!("tar 流处理完成: 总条目数={}, 成功={}, 用时={:?}", 
            processed_entries, successful_entries, processing_start.elapsed());
    });

    Ok(rx)
  }
}

// 智能错误跟踪器
#[derive(Debug)]
struct TarErrorTracker {
    error_fingerprints: std::collections::HashMap<String, ErrorInfo>,
    last_error_time: Option<std::time::Instant>,
    consecutive_errors: usize,
    total_errors: usize,
    rapid_error_count: usize,
    start_time: std::time::Instant,
}

#[derive(Debug)]
struct ErrorInfo {
    count: usize,
    last_seen: std::time::Instant,
    retry_count: u32,
}

#[derive(Debug)]
enum ErrorAction {
    AbortProcessing,                                    // 终止整个处理
    RetryWithBackoff { delay: std::time::Duration, attempt: u32 }, // 重试
    SkipEntry,                                         // 跳过当前条目
}

impl TarErrorTracker {
    fn new() -> Self {
        Self {
            error_fingerprints: std::collections::HashMap::new(),
            last_error_time: None,
            consecutive_errors: 0,
            total_errors: 0,
            rapid_error_count: 0,
            start_time: std::time::Instant::now(),
        }
    }
    
    fn record_success(&mut self) {
        self.consecutive_errors = 0;
        self.rapid_error_count = 0;
    }
    
    fn analyze_error(&mut self, error: &std::io::Error) -> ErrorAction {
        let now = std::time::Instant::now();
        self.total_errors += 1;
        self.consecutive_errors += 1;
        
        // 创建错误指纹
        let fingerprint = self.create_error_fingerprint(error);
        
        // 分析时间模式（在更新错误统计之前）
        let is_rapid = self.is_rapid_error(now);
        
        // 检查致命错误条件（先检查，避免不必要的更新）
        if self.is_fatal_error_check(error, &fingerprint, is_rapid, now) {
            return ErrorAction::AbortProcessing;
        }
        
        // 检查可重试的错误类型（不依赖状态）
        let can_retry = self.can_retry_error_type(error);
        
        // 更新错误统计并决定重试
        let should_retry = {
            let error_info = self.error_fingerprints.entry(fingerprint.clone())
                .or_insert(ErrorInfo {
                    count: 0,
                    last_seen: now,
                    retry_count: 0,
                });
            
            error_info.count += 1;
            error_info.last_seen = now;
            
            // 检查是否应该重试（结合错误类型和重试次数）
            if can_retry && error_info.retry_count < 3 {
                error_info.retry_count += 1;
                let delay = std::time::Duration::from_millis(100 * (1_u64 << error_info.retry_count.min(4)));
                Some((delay, error_info.retry_count))
            } else {
                None
            }
        };
        
        // 返回决策
        if let Some((delay, attempt)) = should_retry {
            ErrorAction::RetryWithBackoff { delay, attempt }
        } else {
            ErrorAction::SkipEntry
        }
    }
    
    fn create_error_fingerprint(&self, error: &std::io::Error) -> String {
        let mut fingerprint = format!("{:?}", error.kind());
        let error_msg = error.to_string().to_lowercase();
        
        if error_msg.contains("not in gzip format") || error_msg.contains("invalid gzip header") {
            fingerprint.push_str(":invalid_gzip");
        } else if error_msg.contains("tar") && error_msg.contains("header") {
            fingerprint.push_str(":tar_header");
        } else if error_msg.contains("crc") || error_msg.contains("checksum") {
            fingerprint.push_str(":corruption");
        } else if error_msg.contains("timeout") {
            fingerprint.push_str(":timeout");
        } else if error_msg.contains("connection") {
            fingerprint.push_str(":network");
        }
        
        fingerprint
    }
    
    fn is_rapid_error(&mut self, now: std::time::Instant) -> bool {
        if let Some(last_time) = self.last_error_time {
            let interval = now.duration_since(last_time);
            if interval.as_millis() < 10 {
                self.rapid_error_count += 1;
                self.last_error_time = Some(now);
                return self.rapid_error_count >= 5;
            }
        }
        
        self.last_error_time = Some(now);
        self.rapid_error_count = 0;
        false
    }
    
    fn is_fatal_error_check(&self, error: &std::io::Error, fingerprint: &str, is_rapid: bool, _now: std::time::Instant) -> bool {
        // 致命错误类型
        match error.kind() {
            std::io::ErrorKind::PermissionDenied |
            std::io::ErrorKind::NotFound => return true,
            _ => {}
        }
        
        // 格式完全错误
        if fingerprint.contains("invalid_gzip") {
            return true;
        }
        
        // 快速错误循环 (同一错误快速重复)
        if is_rapid && fingerprint.contains("tar_header") {
            return true;
        }
        
        // 连续错误过多
        if self.consecutive_errors >= 50 {
            return true;
        }
        
        // 总体错误率过高
        if self.start_time.elapsed().as_secs() > 60 && self.total_errors > 100 {
            return true;
        }
        
        false
    }
    
    
    fn can_retry_error_type(&self, error: &std::io::Error) -> bool {
        // 可重试的错误类型
        match error.kind() {
            std::io::ErrorKind::TimedOut |
            std::io::ErrorKind::ConnectionAborted |
            std::io::ErrorKind::ConnectionRefused |
            std::io::ErrorKind::Interrupted => true,
            _ => {
                let msg = error.to_string().to_lowercase();
                msg.contains("timeout") ||
                msg.contains("connection reset") ||
                msg.contains("broken pipe") ||
                msg.contains("temporary failure")
            }
        }
    }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::query::Query;
  use std::pin::Pin;
  use std::task::{Context, Poll};
  use tokio::io::{AsyncRead, ReadBuf};

  // 用于测试的内存 AsyncRead 实现
  struct MemReader {
    buf: Vec<u8>,
    pos: usize,
  }
  impl MemReader {
    fn new<S: AsRef<[u8]>>(s: S) -> Self {
      Self {
        buf: s.as_ref().to_vec(),
        pos: 0,
      }
    }
  }
  impl AsyncRead for MemReader {
    fn poll_read(mut self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
      let remaining = self.buf.len().saturating_sub(self.pos);
      if remaining == 0 {
        return Poll::Ready(Ok(()));
      }
      let n = remaining.min(buf.remaining());
      let end = self.pos + n;
      buf.put_slice(&self.buf[self.pos..end]);
      self.pos = end;
      Poll::Ready(Ok(()))
    }
  }

  async fn grep_with_q(input: &str, q: &str, ctx: usize) -> Option<(Vec<String>, Vec<(usize, usize)>)> {
    let spec = Query::parse_github_like(q).expect("解析失败");
    let mut r = MemReader::new(input.as_bytes());
    grep_context(&mut r, &spec, ctx).await.ok().flatten()
  }

  #[tokio::test]
  async fn grep_basic_and_merge_context() {
    let input = r#"line1
line2
error: first
mid
error: second
line7
"#;
    let res = grep_with_q(input, "error", 1).await.expect("应当有结果");
    let (lines, ranges) = res;
    assert_eq!(lines.len(), 6);
    // hits at line 3 and 5 (1-based), with ctx=1 -> ranges: [2..4] and [4..6] merged into [2..6]
    assert_eq!(ranges, vec![(1, 5)]);
  }

  #[tokio::test]
  async fn grep_not_excludes_file() {
    let input = r#"error present
but also debug here
"#;
    let res = grep_with_q(input, "error -debug", 1).await;
    assert!(res.is_none(), "由于存在取反关键字，应当排除该文件");
  }

  #[tokio::test]
  async fn grep_or_and_precedence() {
    let input = r#"foo only
baz only
foo and baz
"#;
    // (foo OR bar) baz
    let some1 = grep_with_q(input, "(foo OR bar) baz", 0).await;
    assert!(some1.is_some(), "应当匹配 'foo and baz'");

    // bar alone shouldn't satisfy because baz also required
    let some2 = grep_with_q("bar only\n", "(foo OR bar) baz", 0).await;
    assert!(some2.is_none());
  }

  #[tokio::test]
  async fn grep_binary_rejected() {
    // 在早期包含一个 NUL 字节
    let bytes = [0x66u8, 0x6Fu8, 0x00u8, 0x61u8]; // f o \\0 a
    let spec = Query::parse_github_like("foo").unwrap();
    let mut r = MemReader::new(bytes);
    let res = grep_context(&mut r, &spec, 1).await.ok().flatten();
    assert!(res.is_none(), "binary-like content should be rejected");
  }

  #[tokio::test]
  async fn grep_explicit_and_equivalence() {
    let input = r#"foo here
bar here
foo and bar here
"#;
    let a = grep_with_q(input, "foo bar", 0).await;
    let b = grep_with_q(input, "foo AND bar", 0).await;
    assert!(a.is_some() && b.is_some());
    assert_eq!(a.as_ref().unwrap().1, b.as_ref().unwrap().1);
  }

  #[tokio::test]
  async fn grep_phrase_and_regex_positive() {
    let input = r#"connection reset by peer
ERR123 occurred
"#;
    // 短语 或 正则
    let some = grep_with_q(input, "\"connection reset\" OR /ERR\\d{3}/", 0).await;
    assert!(some.is_some());
  }

  #[tokio::test]
  async fn grep_negated_group_excludes() {
    let input = r#"bad state
warning present
"#;
    let res = grep_with_q(input, "-(bad OR warning) ok", 0).await;
    // contains a negated term => file should be excluded regardless of ok
    assert!(res.is_none());
  }

  #[tokio::test]
  async fn grep_only_negation_has_no_output() {
    let input = r#"just normal text
"#;
    // 只有 NOT 子句为真，但没有任一正向关键字驱动高亮 => 不输出
    let res = grep_with_q(input, "-error", 1).await;
    assert!(res.is_none());
  }

  #[tokio::test]
  async fn grep_parentheses_adjacency_is_and() {
    let input = r#"alpha beta both
alpha only
beta only
"#;
    // (alpha) (beta) == alpha AND beta
    let some = grep_with_q(input, "(alpha) (beta)", 0).await;
    assert!(some.is_some());
    let none = grep_with_q(input, "(alpha) (beta)", 0).await;
    assert!(none.is_some());
  }

  #[tokio::test]
  async fn grep_case_sensitivity_literal() {
    let input = r#"Foo upper
foo lower
"#;
    // 字面量大小写敏感
    let hit_lower = grep_with_q(input, "foo", 0).await;
    assert!(hit_lower.is_some());
    let hit_upper = grep_with_q(input, "Foo", 0).await;
    assert!(hit_upper.is_some());
    let miss_mixed = grep_with_q(input, "fOo", 0).await;
    assert!(miss_mixed.is_none());
  }

  // === look-around 支持测试（依赖 fancy-regex 动态选择） ===
  #[tokio::test]
  async fn grep_lookahead_positive() {
    let input_hit = "prefix foobar suffix";
    let input_miss = "prefix foobaz suffix";
    assert!(grep_with_q(input_hit, r#"/foo(?=bar)/"#, 0).await.is_some());
    assert!(grep_with_q(input_miss, r#"/foo(?=bar)/"#, 0).await.is_none());
  }

  #[tokio::test]
  async fn grep_lookahead_negative() {
    let input_hit = "foobaz here";
    let input_miss = "foobar here";
    assert!(grep_with_q(input_hit, r#"/foo(?!bar)/"#, 0).await.is_some());
    assert!(grep_with_q(input_miss, r#"/foo(?!bar)/"#, 0).await.is_none());
  }

  #[tokio::test]
  async fn grep_lookbehind_positive() {
    let input_hit = "ERR123 occurred";
    let input_miss = "E123 occurred";
    // 数字前必须有 ERR（测试保留反斜杠后的 look-behind）
    assert!(grep_with_q(input_hit, r#"/(?<=ERR)\d+/"#, 0).await.is_some());
    assert!(grep_with_q(input_miss, r#"/(?<=ERR)\d+/"#, 0).await.is_none());
  }

  #[tokio::test]
  async fn grep_lookbehind_negative() {
    // 使用紧邻前缀以避免空格导致的误解
    let input_hit = "zoobar end"; // bar 前不是 foo
    let input_miss = "foobar end"; // bar 前是 foo
    assert!(grep_with_q(input_hit, r#"/(?<!foo)bar/"#, 0).await.is_some());
    assert!(grep_with_q(input_miss, r#"/(?<!foo)bar/"#, 0).await.is_none());
  }

  #[tokio::test]
  async fn grep_lookbehind_log() {
    // 使用紧邻前缀以避免空格导致的误解
    let input_hit = "zoobar end"; // bar 前不是 foo
    let input_miss = "foobar end"; // bar 前是 foo
    // 需求：整行不得出现 foo，且同时出现 bar 与 end（顺序任意）
    // 解释：
    // ^(?!.*foo) —— 行首负向先行，整行不包含 foo
    // .*(bar.*end|end.*bar) —— 同时包含 bar 与 end
    assert!(
      grep_with_q(input_hit, r#"/^(?!.*foo).*(?:bar.*end|end.*bar)/"#, 0)
        .await
        .is_some()
    );
    assert!(
      grep_with_q(input_miss, r#"/^(?!.*foo).*(?:bar.*end|end.*bar)/"#, 0)
        .await
        .is_none()
    );
  }
}
