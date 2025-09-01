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
  #[error("io error: {0}")]
  Io(#[from] io::Error),
}

use crate::query::QuerySpec;

#[async_trait]
pub trait Search {
  async fn search(
    self,
    spec: &QuerySpec,
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

pub async fn grep_context_from_reader_async<R: AsyncRead + Unpin>(
  reader: &mut R,
  spec: &crate::query::QuerySpec,
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

  // Evaluate expression at file level: whether terms occur anywhere
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
    // include the line if it matches any positive term
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

  // File-level boolean evaluation
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
    spec: &QuerySpec,
    context_lines: usize,
  ) -> Result<tokio::sync::mpsc::Receiver<SearchResult>, SearchError> {
    let (tx, rx) = tokio::sync::mpsc::channel::<SearchResult>(128);

    // Share query spec across tasks
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

                // Apply path filters early if specified
                let path_str = path.to_string_lossy();
                if !spec_outer.path_allowed(path_str.as_ref()) {
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
                    if let Ok(Some((lines, merged))) =
                      grep_context_from_reader_async(&mut reader, &spec_local, context_lines).await
                    {
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
    spec: &QuerySpec,
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

        // Path filters for tar entry
        if !spec_owned.path_allowed(&path) {
          continue;
        }

        // async_tar 的 Entry 实现的是 futures::io::AsyncRead，这里适配为 tokio::io::AsyncRead
        let mut entry_compat = entry.compat();
        let Ok(Some((lines, merged))) =
          grep_context_from_reader_async(&mut entry_compat, &spec_owned, context_lines).await
        else {
          continue;
        };

        let _ = tx.send(SearchResult::new(path, lines, merged)).await;
      }
    });

    Ok(rx)
  }
}
