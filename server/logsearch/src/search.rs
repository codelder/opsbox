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
        return Ok(None);
      }
      sample_checked = true;
    }
    let trimmed = line.trim_end_matches(['\r', '\n']);
    lines.push(trimmed.to_string());
  }
  if !sample_checked {
    if !is_probably_text_bytes(&sample) {
      return Ok(None);
    }
  }

  // 文件级布尔计算：检查各关键字是否在文件中出现
  let term_count = spec.terms.len();
  if term_count == 0 {
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
  if !spec.eval_file(&occurs) {
    return Ok(None);
  }

  if matched_lines.is_empty() {
    return Ok(None);
  }

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
        return;
      };

      while let Some(entry_res) = entries.next().await {
        let Ok(entry) = entry_res else {
          continue;
        };
        let path = match entry.path() {
          Ok(p) => p.to_string_lossy().to_string(),
          Err(_) => String::new(),
        };

        // 针对 tar 条目的路径过滤
        if !spec_owned.path_filter.is_allowed(&path) {
          continue;
        }

        // async_tar 的 Entry 实现的是 futures::io::AsyncRead，这里适配为 tokio::io::AsyncRead
        let mut entry_compat = entry.compat();
        let Ok(Some((lines, merged))) = grep_context(&mut entry_compat, &spec_owned, context_lines).await else {
          continue;
        };

        let _ = tx.send(SearchResult::new(path, lines, merged)).await;
      }
    });

    Ok(rx)
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
}
