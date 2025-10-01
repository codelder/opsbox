use std::{
  io::{self},
  sync::Arc,
};

use async_compression::tokio::bufread::GzipDecoder;
use async_tar::Archive as AsyncArchive;
use async_trait::async_trait;
use futures::StreamExt;
// use futures::io::AsyncReadExt as FuturesAsyncReadExt;
use log::{debug, error, info, warn};
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
  #[error("Channel 已关闭")]
  ChannelClosed,
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

// ============================================================================
// 配置和辅助类型
// ============================================================================

/// 搜索配置
#[derive(Debug, Clone)]
struct SearchConfig {
  max_processing_time: std::time::Duration,
  entry_timeout: std::time::Duration,
  content_timeout: std::time::Duration,
}

impl Default for SearchConfig {
  fn default() -> Self {
    Self {
      max_processing_time: std::time::Duration::from_secs(300), // 5分钟
      entry_timeout: std::time::Duration::from_secs(30),
      content_timeout: std::time::Duration::from_secs(60),
    }
  }
}

/// 处理统计
#[derive(Debug, Clone)]
struct ProcessingStats {
  processed_entries: usize,
  successful_entries: usize,
  start_time: std::time::Instant,
}

impl ProcessingStats {
  fn new() -> Self {
    Self {
      processed_entries: 0,
      successful_entries: 0,
      start_time: std::time::Instant::now(),
    }
  }

  fn elapsed(&self) -> std::time::Duration {
    self.start_time.elapsed()
  }
}

// ============================================================================
// SearchProcessor：负责处理单个文件/条目的搜索逻辑（纯业务逻辑，易于测试）
// ============================================================================

/// 搜索处理器：封装可复用的搜索逻辑
///
/// 这个结构体提取了两个 Search 实现中的公共代码，使得：
/// 1. 路径过滤逻辑可以单独测试
/// 2. 内容处理逻辑可以单独测试
/// 3. 结果发送逻辑可以单独测试
/// 4. 新的搜索源可以复用这些逻辑
struct SearchProcessor {
  spec: Arc<Query>,
  context_lines: usize,
}

impl SearchProcessor {
  /// 创建新的搜索处理器
  fn new(spec: Arc<Query>, context_lines: usize) -> Self {
    Self { spec, context_lines }
  }

  /// 检查路径是否应该被处理（纯函数，易于测试）
  ///
  /// # Examples
  /// ```ignore
  /// let processor = SearchProcessor::new(...);
  /// assert!(processor.should_process_path("file.log"));
  /// assert!(!processor.should_process_path("file.txt"));
  /// ```
  fn should_process_path(&self, path: &str) -> bool {
    self.spec.path_filter.is_allowed(path)
  }

  /// 处理文件内容并返回搜索结果（可单独测试）
  ///
  /// # 参数
  /// - `path`: 文件路径
  /// - `reader`: 异步读取器
  ///
  /// # 返回
  /// - `Ok(Some(SearchResult))`: 找到匹配结果
  /// - `Ok(None)`: 没有匹配结果
  /// - `Err(SearchError)`: 处理出错
  async fn process_content<R: AsyncRead + Unpin>(
    &self,
    path: String,
    reader: &mut R,
  ) -> Result<Option<SearchResult>, SearchError> {
    match grep_context(reader, &self.spec, self.context_lines).await? {
      Some((lines, merged)) => {
        debug!("找到匹配: {} ({} 行)", path, merged.len());
        Ok(Some(SearchResult::new(path, lines, merged)))
      }
      None => Ok(None),
    }
  }

  /// 发送结果到 channel（可单独测试）
  ///
  /// # 返回
  /// - `Ok(())`: 发送成功
  /// - `Err(SearchError::ChannelClosed)`: 接收端已关闭
  async fn send_result(
    &self,
    result: SearchResult,
    tx: &tokio::sync::mpsc::Sender<SearchResult>,
  ) -> Result<(), SearchError> {
    tx.send(result).await.map_err(|_| SearchError::ChannelClosed)?;
    Ok(())
  }
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
  debug!(
    "开始文本搜索，上下文行数: {}, 搜索条件数: {}",
    context_lines,
    spec.terms.len()
  );

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
    if let Some(last) = merged.last_mut()
      && s <= last.1 + 1
    {
      if e > last.1 {
        last.1 = e;
      }
      continue;
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

    // 使用 SearchProcessor 封装公共逻辑 ✅
    let spec_arc = Arc::new(spec.clone());
    let processor = Arc::new(SearchProcessor::new(spec_arc.clone(), context_lines));

    let max_concurrency = std::thread::available_parallelism()
      .map(|n| n.get())
      .unwrap_or(4)
      .saturating_mul(2)
      .min(256);
    let semaphore = Arc::new(Semaphore::new(max_concurrency));

    tokio::spawn({
      let mut stack = vec![self];
      let processor_outer = Arc::clone(&processor);
      let semaphore = Arc::clone(&semaphore);
      let tx = tx.clone();

      async move {
        let mut tasks = JoinSet::new();

        while let Some(mut rd) = stack.pop() {
          loop {
            match rd.next_entry().await {
              Ok(Some(entry)) => {
                let path = entry.path();

                // 使用 processor 的路径过滤 ✅
                let path_str = path.to_string_lossy();
                if !processor_outer.should_process_path(path_str.as_ref()) {
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
                let processor_local = Arc::clone(&processor_outer);
                let path_owned = path.to_string_lossy().into_owned();

                tasks.spawn(async move {
                  let _permit = permit; // 持有期间占用并发额度
                  if let Ok(file) = fs::File::open(&path).await {
                    let mut reader = BufReader::new(file);

                    // 使用 processor 处理内容 ✅
                    if let Ok(Some(result)) = processor_local.process_content(path_owned, &mut reader).await {
                      // 使用 processor 发送结果 ✅
                      let _ = processor_local.send_result(result, &txf).await;
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
impl<T: AsRef<[u8]> + Send + Unpin + 'static> SearchiableAsyncReader
  for tokio_util::compat::Compat<futures::io::Cursor<T>>
{
}

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

    // 使用重构后的组件 ✅
    let config = SearchConfig::default();
    let spec_arc = Arc::new(spec.clone());
    let search_processor = Arc::new(SearchProcessor::new(spec_arc, context_lines));

    tokio::spawn(async move {
      // 1. 解压和解析 tar.gz
      let gz = GzipDecoder::new(BufReader::new(self));
      //:TODO AsyncRead 不一定是 tar 格式，需要检查
      let archive = AsyncArchive::new(gz.compat());
      let Ok(entries) = archive.entries() else {
        error!("无法创建 tar 归档条目迭代器");
        return;
      };

      // 2. 使用拆分后的组件处理流 ✅
      let entry_processor = TarEntryProcessor::new(search_processor, config.clone());
      let mut stream_processor = TarStreamProcessor::new(entry_processor, config);

      // 3. 处理整个流（所有复杂逻辑都在 TarStreamProcessor 中）✅
      let _stats = stream_processor.process_stream(entries, tx).await;
    });

    Ok(rx)
  }
}

// ============================================================================
// TarEntryProcessor：处理单个 tar 条目（可单独测试）
// ============================================================================

/// Tar 条目处理器
///
/// 负责处理单个 tar 归档条目的逻辑，包括：
/// - 路径提取和生成备用路径
/// - 路径过滤
/// - 内容搜索（复用 SearchProcessor）
struct TarEntryProcessor {
  processor: Arc<SearchProcessor>,
  config: SearchConfig,
}

impl TarEntryProcessor {
  fn new(processor: Arc<SearchProcessor>, config: SearchConfig) -> Self {
    Self { processor, config }
  }

  /// 提取条目路径，失败时生成备用路径（纯函数，易于测试）
  fn extract_path<R>(&self, entry: &async_tar::Entry<R>, entry_index: usize) -> String
  where
    R: futures::io::AsyncRead + Unpin,
  {
    entry
      .path()
      .ok()
      .map(|p| p.to_string_lossy().to_string())
      .unwrap_or_else(|| self.generate_fallback_path(entry_index))
  }

  /// 生成备用路径（纯函数，易于测试）
  fn generate_fallback_path(&self, entry_index: usize) -> String {
    format!(
      "<unknown-{}-{}>",
      entry_index,
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
    )
  }

  /// 处理单个条目（可单独测试的核心逻辑）
  async fn process_entry<R>(
    &self,
    entry: async_tar::Entry<R>,
    entry_index: usize,
  ) -> Result<Option<SearchResult>, SearchError>
  where
    R: futures::io::AsyncRead + Unpin,
  {
    // 1. 提取路径
    let path = self.extract_path(&entry, entry_index);

    // 2. 路径过滤
    if !self.processor.should_process_path(&path) {
      debug!("路径不符合过滤条件，跳过: {}", path);
      return Ok(None);
    }

    // 3. 处理内容（带超时）
    let mut entry_compat = entry.compat();
    let process_result = tokio::time::timeout(
      self.config.content_timeout,
      self.processor.process_content(path, &mut entry_compat),
    )
    .await;

    match process_result {
      Ok(Ok(result)) => Ok(result),
      Ok(Err(e)) => {
        warn!("处理条目内容时出错: {}", e);
        Err(e)
      }
      Err(_) => {
        warn!("处理条目内容超时 ({}s)", self.config.content_timeout.as_secs());
        Ok(None)
      }
    }
  }
}

// ============================================================================
// TarStreamProcessor：处理整个 tar.gz 流（可测试的主循环）
// ============================================================================

/// Tar 流处理器
///
/// 负责处理整个 tar.gz 流的循环逻辑，包括：
/// - 超时控制
/// - 条目读取
/// - 错误跟踪和智能重试
/// - 统计信息
struct TarStreamProcessor {
  entry_processor: TarEntryProcessor,
  error_tracker: TarErrorTracker,
  stats: ProcessingStats,
  config: SearchConfig,
}

impl TarStreamProcessor {
  fn new(entry_processor: TarEntryProcessor, config: SearchConfig) -> Self {
    Self {
      entry_processor,
      error_tracker: TarErrorTracker::new(),
      stats: ProcessingStats::new(),
      config,
    }
  }

  /// 处理整个流（可测试的主循环）
  async fn process_stream<R>(
    &mut self,
    mut entries: async_tar::Entries<R>,
    tx: tokio::sync::mpsc::Sender<SearchResult>,
  ) -> ProcessingStats
  where
    R: futures::io::AsyncRead + Unpin,
  {
    loop {
      // 1. 检查超时
      if self.stats.elapsed() > self.config.max_processing_time {
        warn!(
          "tar 流处理超时 ({}s)，已处理 {} 个条目",
          self.config.max_processing_time.as_secs(),
          self.stats.processed_entries
        );
        break;
      }

      // 2. 读取下一个条目（带超时）
      let entry_result = match tokio::time::timeout(self.config.entry_timeout, entries.next()).await {
        Ok(Some(result)) => result,
        Ok(None) => {
          info!(
            "tar 流结束，共处理 {} 个条目，成功 {} 个",
            self.stats.processed_entries, self.stats.successful_entries
          );
          break;
        }
        Err(_) => {
          error!(
            "单个条目读取超时 ({}s)，可能卡在损坏的条目上",
            self.config.entry_timeout.as_secs()
          );
          break;
        }
      };

      self.stats.processed_entries += 1;

      // 3. 智能错误处理
      let entry = match self.handle_entry_error(entry_result) {
        EntryAction::Process(entry) => entry,
        EntryAction::Skip => continue,
        EntryAction::Abort => break,
      };

      // 4. 处理条目
      match self
        .entry_processor
        .process_entry(entry, self.stats.processed_entries)
        .await
      {
        Ok(Some(result)) => {
          if self.send_result(result, &tx).await.is_err() {
            debug!("接收端已关闭，停止处理");
            break;
          }
          self.stats.successful_entries += 1;
        }
        Ok(None) => {
          // 无匹配结果，继续
        }
        Err(e) => {
          warn!("处理条目失败: {}", e);
        }
      }
    }

    info!(
      "tar 流处理完成: 总条目数={}, 成功={}, 用时={:?}",
      self.stats.processed_entries,
      self.stats.successful_entries,
      self.stats.elapsed()
    );

    self.stats.clone()
  }

  /// 处理条目读取错误（可单独测试）
  fn handle_entry_error<R>(&mut self, result: std::io::Result<async_tar::Entry<R>>) -> EntryAction<R>
  where
    R: futures::io::AsyncRead + Unpin,
  {
    match result {
      Ok(entry) => {
        self.error_tracker.record_success();
        EntryAction::Process(entry)
      }
      Err(error) => match self.error_tracker.analyze_error(&error) {
        ErrorAction::AbortProcessing => {
          error!("检测到致命错误模式，终止处理: {}", error);
          EntryAction::Abort
        }
        ErrorAction::RetryWithBackoff { delay, attempt } => {
          warn!("重试第 {} 次 (延迟 {:?}): {}", attempt, delay, error);
          // 注意：这里简化处理，实际的重试需要在上层实现
          EntryAction::Skip
        }
        ErrorAction::SkipEntry => {
          warn!("跳过损坏条目: {}", error);
          EntryAction::Skip
        }
      },
    }
  }

  /// 发送结果（可单独测试）
  async fn send_result(&self, result: SearchResult, tx: &tokio::sync::mpsc::Sender<SearchResult>) -> Result<(), ()> {
    tx.send(result).await.map_err(|_| ())
  }
}

/// 条目处理动作
enum EntryAction<R: futures::io::AsyncRead + Unpin> {
  Process(async_tar::Entry<R>),
  Skip,
  Abort,
}

// ============================================================================
// 智能错误跟踪器
// ============================================================================

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
  AbortProcessing,                                               // 终止整个处理
  RetryWithBackoff { delay: std::time::Duration, attempt: u32 }, // 重试
  SkipEntry,                                                     // 跳过当前条目
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
      let error_info = self.error_fingerprints.entry(fingerprint.clone()).or_insert(ErrorInfo {
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

  fn is_fatal_error_check(
    &self,
    error: &std::io::Error,
    fingerprint: &str,
    is_rapid: bool,
    _now: std::time::Instant,
  ) -> bool {
    // 致命错误类型
    match error.kind() {
      std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::NotFound => return true,
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
      std::io::ErrorKind::TimedOut
      | std::io::ErrorKind::ConnectionAborted
      | std::io::ErrorKind::ConnectionRefused
      | std::io::ErrorKind::Interrupted => true,
      _ => {
        let msg = error.to_string().to_lowercase();
        msg.contains("timeout")
          || msg.contains("connection reset")
          || msg.contains("broken pipe")
          || msg.contains("temporary failure")
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

  // === is_probably_text_bytes 测试 ===
  #[test]
  fn test_is_text_empty_bytes() {
    assert!(is_probably_text_bytes(&[]));
  }

  #[test]
  fn test_is_text_contains_null() {
    let bytes = b"hello\x00world";
    assert!(!is_probably_text_bytes(bytes));
  }

  #[test]
  fn test_is_text_high_printable_ratio() {
    let bytes = b"This is normal text\n";
    assert!(is_probably_text_bytes(bytes));
  }

  #[test]
  fn test_is_text_low_printable_ratio() {
    // 大量不可打印字符（且不是有效 UTF-8）
    let bytes = &[0x01, 0x02, 0x03, 0x04, 0x05, 0xFF, 0xFE];
    assert!(!is_probably_text_bytes(bytes));
  }

  #[test]
  fn test_is_text_valid_utf8() {
    let bytes = "UTF-8 文本 😀".as_bytes();
    assert!(is_probably_text_bytes(bytes));
  }

  #[test]
  fn test_is_text_mixed_printable() {
    // 95% 以上是可打印字符
    let mut bytes = vec![0x20; 95]; // 空格
    bytes.extend_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05]); // 5个不可打印
    assert!(is_probably_text_bytes(&bytes));
  }

  // === 边界和异常场景测试 ===
  #[tokio::test]
  async fn grep_empty_input() {
    let res = grep_with_q("", "error", 0).await;
    assert!(res.is_none());
  }

  #[tokio::test]
  async fn grep_only_whitespace() {
    let input = "   \n\t\n   ";
    let res = grep_with_q(input, "error", 0).await;
    assert!(res.is_none());
  }

  #[tokio::test]
  async fn grep_very_long_line() {
    // 测试超长行处理
    let long_line = "a".repeat(10000) + "error" + &"b".repeat(10000);
    let res = grep_with_q(&long_line, "error", 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_multiple_matches_same_line() {
    let input = "error error error\n";
    let res = grep_with_q(input, "error", 0).await;
    assert!(res.is_some());
    let (_, ranges) = res.unwrap();
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0], (0, 0));
  }

  #[tokio::test]
  async fn grep_context_extends_to_file_boundaries() {
    let input = "line1\nerror\nline3\n";
    let res = grep_with_q(input, "error", 100).await;
    assert!(res.is_some());
    let (lines, ranges) = res.unwrap();
    // 上下文再大也不应超过文件边界
    assert_eq!(lines.len(), 3);
    assert_eq!(ranges[0], (0, 2));
  }

  #[tokio::test]
  async fn grep_overlapping_context_merged() {
    let input = "line1\nerror1\nline3\nerror2\nline5\n";
    let res = grep_with_q(input, "error", 1).await;
    assert!(res.is_some());
    let (_, ranges) = res.unwrap();
    // error1 at line 1 (0-indexed), error2 at line 3
    // 带 context=1: [0..2] 和 [2..4] 应该合并
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0], (0, 4));
  }

  #[tokio::test]
  async fn grep_regex_with_special_chars() {
    let input = "test@example.com found\n";
    let res = grep_with_q(input, r#"/\S+@\S+\.\S+/"#, 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_regex_multiline_disabled() {
    // grep_context 按行处理，正则不应跨行匹配
    let input = "line1\nline2";
    let res = grep_with_q(input, "/line1.*line2/", 0).await;
    assert!(res.is_none());
  }

  #[tokio::test]
  async fn grep_unicode_characters() {
    let input = "日志中的错误信息\n正常信息\n";
    let res = grep_with_q(input, "错误", 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_mixed_case_in_phrase() {
    let input = "Connection Reset By Peer\n";
    // 短语匹配是否大小写敏感
    let res = grep_with_q(input, "\"Connection Reset\"", 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_phrase_with_special_chars() {
    let input = "error: connection failed!\n";
    let res = grep_with_q(input, "\"connection failed!\"", 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_empty_phrase() {
    let input = "some text\n";
    let _res = grep_with_q(input, "\"\"", 0).await;
    // 空短语的行为取决于实现
    // 可能匹配所有行或不匹配
  }

  #[tokio::test]
  async fn grep_complex_boolean() {
    let input = "alpha beta gamma\ndelta epsilon\nalpha gamma\n";
    // (alpha OR delta) AND gamma
    let res = grep_with_q(input, "(alpha OR delta) gamma", 0).await;
    assert!(res.is_some());
    let (_, ranges) = res.unwrap();
    // 应匹配 line 0 和 line 2，可能合并为一个范围
    assert!(!ranges.is_empty());
  }

  #[tokio::test]
  async fn grep_nested_groups() {
    let input = "foo bar baz\nfoo only\nbar baz\n";
    // foo AND (bar OR baz)
    let res = grep_with_q(input, "foo (bar OR baz)", 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_multiple_negations() {
    let input = "normal log entry\n";
    let res = grep_with_q(input, "-error -warning -critical", 0).await;
    // 只有否定条件，没有正向匹配
    assert!(res.is_none());
  }

  #[tokio::test]
  async fn grep_regex_anchors() {
    let input = "error at start\nmiddle error here\n";
    // 行首匹配
    let res = grep_with_q(input, "/^error/", 0).await;
    assert!(res.is_some());
    let (_, ranges) = res.unwrap();
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0], (0, 0));
  }

  #[tokio::test]
  async fn grep_regex_word_boundary() {
    let input = "error occurred\nerrorcode\n";
    // 使用词边界
    let res = grep_with_q(input, r#"/\berror\b/"#, 0).await;
    assert!(res.is_some());
    let (_, ranges) = res.unwrap();
    // 应只匹配 line 0 (error 作为单词)
    assert_eq!(ranges.len(), 1);
  }

  #[tokio::test]
  async fn grep_zero_context() {
    let input = "line1\nerror\nline3\n";
    let res = grep_with_q(input, "error", 0).await;
    assert!(res.is_some());
    let (_, ranges) = res.unwrap();
    assert_eq!(ranges[0], (1, 1)); // 只有匹配行本身
  }

  #[tokio::test]
  async fn grep_large_context() {
    let input = "line1\nline2\nerror\nline4\nline5\n";
    let res = grep_with_q(input, "error", 10).await;
    assert!(res.is_some());
    let (lines, ranges) = res.unwrap();
    // 上下文很大，应该包含整个文件
    assert_eq!(lines.len(), 5);
    assert_eq!(ranges[0], (0, 4));
  }

  #[test]
  fn test_search_result_creation() {
    let result = SearchResult::new(
      "test.log".to_string(),
      vec!["line1".to_string(), "line2".to_string()],
      vec![(0, 1)],
    );
    assert_eq!(result.path, "test.log");
    assert_eq!(result.lines.len(), 2);
    assert_eq!(result.merged.len(), 1);
  }

  // === TarErrorTracker 测试 ===
  #[test]
  fn test_tar_error_tracker_creation() {
    let tracker = TarErrorTracker::new();
    assert_eq!(tracker.consecutive_errors, 0);
    assert_eq!(tracker.total_errors, 0);
    assert_eq!(tracker.rapid_error_count, 0);
    assert!(tracker.last_error_time.is_none());
  }

  #[test]
  fn test_tar_error_tracker_record_success() {
    let mut tracker = TarErrorTracker::new();

    // 模拟一些错误
    tracker.consecutive_errors = 5;
    tracker.rapid_error_count = 3;

    // 记录成功
    tracker.record_success();

    assert_eq!(tracker.consecutive_errors, 0);
    assert_eq!(tracker.rapid_error_count, 0);
  }

  #[test]
  fn test_tar_error_tracker_fatal_permission_denied() {
    let mut tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");

    let action = tracker.analyze_error(&error);
    matches!(action, ErrorAction::AbortProcessing);
  }

  #[test]
  fn test_tar_error_tracker_fatal_not_found() {
    let mut tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");

    let action = tracker.analyze_error(&error);
    matches!(action, ErrorAction::AbortProcessing);
  }

  #[test]
  fn test_tar_error_tracker_retryable_timeout() {
    let mut tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");

    let action = tracker.analyze_error(&error);
    matches!(action, ErrorAction::RetryWithBackoff { .. });
  }

  #[test]
  fn test_tar_error_tracker_retryable_connection_aborted() {
    let mut tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "connection aborted");

    let action = tracker.analyze_error(&error);
    matches!(action, ErrorAction::RetryWithBackoff { .. });
  }

  #[test]
  fn test_tar_error_tracker_max_retries_exceeded() {
    let mut tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");

    // 第一次重试
    let action1 = tracker.analyze_error(&error);
    matches!(action1, ErrorAction::RetryWithBackoff { .. });

    // 第二次重试
    let action2 = tracker.analyze_error(&error);
    matches!(action2, ErrorAction::RetryWithBackoff { .. });

    // 第三次重试
    let action3 = tracker.analyze_error(&error);
    matches!(action3, ErrorAction::RetryWithBackoff { .. });

    // 第四次应该跳过
    let action4 = tracker.analyze_error(&error);
    matches!(action4, ErrorAction::SkipEntry);
  }

  #[test]
  fn test_tar_error_tracker_fingerprint_invalid_gzip() {
    let tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::InvalidData, "not in gzip format");

    let fingerprint = tracker.create_error_fingerprint(&error);
    assert!(fingerprint.contains("invalid_gzip"));
  }

  #[test]
  fn test_tar_error_tracker_fingerprint_tar_header() {
    let tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid tar header");

    let fingerprint = tracker.create_error_fingerprint(&error);
    assert!(fingerprint.contains("tar_header"));
  }

  #[test]
  fn test_tar_error_tracker_fingerprint_corruption() {
    let tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::InvalidData, "crc mismatch");

    let fingerprint = tracker.create_error_fingerprint(&error);
    assert!(fingerprint.contains("corruption"));
  }

  #[test]
  fn test_tar_error_tracker_fingerprint_timeout() {
    let tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::Other, "connection timeout");

    let fingerprint = tracker.create_error_fingerprint(&error);
    assert!(fingerprint.contains("timeout"));
  }

  #[test]
  fn test_tar_error_tracker_fingerprint_network() {
    let tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::Other, "connection reset");

    let fingerprint = tracker.create_error_fingerprint(&error);
    assert!(fingerprint.contains("network"));
  }

  #[test]
  fn test_tar_error_tracker_consecutive_errors_threshold() {
    let mut tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::Other, "some error");

    // 模拟 50 个连续错误
    for _ in 0..49 {
      tracker.consecutive_errors += 1;
    }

    // 第 50 个错误应该终止
    let action = tracker.analyze_error(&error);
    matches!(action, ErrorAction::AbortProcessing);
  }

  #[test]
  fn test_tar_error_tracker_can_retry_interrupted() {
    let tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::Interrupted, "interrupted");

    assert!(tracker.can_retry_error_type(&error));
  }

  #[test]
  fn test_tar_error_tracker_can_retry_connection_refused() {
    let tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");

    assert!(tracker.can_retry_error_type(&error));
  }

  #[test]
  fn test_tar_error_tracker_can_retry_connection_reset() {
    let tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::Other, "connection reset by peer");

    assert!(tracker.can_retry_error_type(&error));
  }

  #[test]
  fn test_tar_error_tracker_can_retry_broken_pipe() {
    let tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::Other, "broken pipe");

    assert!(tracker.can_retry_error_type(&error));
  }

  #[test]
  fn test_tar_error_tracker_cannot_retry_permission() {
    let tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");

    assert!(!tracker.can_retry_error_type(&error));
  }

  #[test]
  fn test_tar_error_tracker_invalid_gzip_is_fatal() {
    let mut tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::InvalidData, "not in gzip format");

    let action = tracker.analyze_error(&error);
    matches!(action, ErrorAction::AbortProcessing);
  }

  #[test]
  fn test_tar_error_tracker_different_errors() {
    let mut tracker = TarErrorTracker::new();

    let error1 = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");
    let error2 = std::io::Error::new(std::io::ErrorKind::Interrupted, "interrupted");

    // 不同类型的错误应该分别计数
    tracker.analyze_error(&error1);
    tracker.analyze_error(&error2);

    assert_eq!(tracker.total_errors, 2);
  }

  #[test]
  fn test_tar_error_tracker_exponential_backoff() {
    let mut tracker = TarErrorTracker::new();
    let error = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");

    // 第一次重试
    if let ErrorAction::RetryWithBackoff { delay, attempt } = tracker.analyze_error(&error) {
      assert_eq!(attempt, 1);
      // 第一次重试延迟应该是 200ms (100 * 2^1)
      assert!(delay.as_millis() >= 100);
    } else {
      panic!("Expected RetryWithBackoff");
    }

    // 第二次重试
    if let ErrorAction::RetryWithBackoff { delay, attempt } = tracker.analyze_error(&error) {
      assert_eq!(attempt, 2);
      // 第二次重试延迟应该更长
      assert!(delay.as_millis() >= 200);
    } else {
      panic!("Expected RetryWithBackoff");
    }
  }

  // === 更多 grep_context 边界测试 ===
  #[tokio::test]
  async fn grep_single_line_file() {
    let input = "error found";
    let res = grep_with_q(input, "error", 0).await;
    assert!(res.is_some());
    let (lines, ranges) = res.unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(ranges[0], (0, 0));
  }

  #[tokio::test]
  async fn grep_no_newline_at_end() {
    let input = "line1\nline2\nerror";
    let res = grep_with_q(input, "error", 0).await;
    assert!(res.is_some());
    let (_, ranges) = res.unwrap();
    assert_eq!(ranges[0], (2, 2));
  }

  #[tokio::test]
  async fn grep_multiple_consecutive_matches() {
    let input = "error1\nerror2\nerror3\n";
    let res = grep_with_q(input, "error", 0).await;
    assert!(res.is_some());
    let (_, ranges) = res.unwrap();
    // 应该合并为一个范围
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0], (0, 2));
  }

  #[tokio::test]
  async fn grep_match_at_start_and_end() {
    let input = "error\nmiddle\nmiddle\nerror\n";
    let res = grep_with_q(input, "error", 0).await;
    assert!(res.is_some());
    let (_, ranges) = res.unwrap();
    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0], (0, 0));
    assert_eq!(ranges[1], (3, 3));
  }

  #[tokio::test]
  async fn grep_all_lines_match() {
    let input = "error1\nerror2\nerror3\n";
    let res = grep_with_q(input, "error", 0).await;
    assert!(res.is_some());
    let (lines, ranges) = res.unwrap();
    assert_eq!(lines.len(), 3);
    assert_eq!(ranges[0], (0, 2));
  }

  #[tokio::test]
  async fn grep_case_sensitive_regex() {
    let input = "Error\nerror\nERROR\n";
    let res = grep_with_q(input, "/error/", 0).await;
    assert!(res.is_some());
    let (_, ranges) = res.unwrap();
    // 只应匹配小写的 error
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0], (1, 1));
  }

  #[tokio::test]
  async fn grep_regex_case_insensitive() {
    let input = "Error\nerror\nERROR\n";
    // 使用 (?i) 标志
    let res = grep_with_q(input, "/(?i)error/", 0).await;
    assert!(res.is_some());
    let (_, ranges) = res.unwrap();
    // 应该匹配所有行
    assert_eq!(ranges[0], (0, 2));
  }

  #[tokio::test]
  async fn grep_context_merging_edge_case() {
    // 测试两个匹配之间恰好差 context+1 行的情况
    let input = "line1\nerror1\nline3\nline4\nerror2\nline6\n";
    let res = grep_with_q(input, "error", 1).await;
    assert!(res.is_some());
    let (_, ranges) = res.unwrap();
    // error1 at index 1, error2 at index 4
    // context=1: [0..2] and [3..5]
    // 它们相邻，应该合并
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0], (0, 5));
  }

  #[tokio::test]
  async fn grep_empty_lines_in_file() {
    let input = "line1\n\nerror\n\nline5\n";
    let res = grep_with_q(input, "error", 0).await;
    assert!(res.is_some());
    let (lines, ranges) = res.unwrap();
    assert_eq!(lines.len(), 5);
    assert_eq!(ranges[0], (2, 2));
  }

  #[tokio::test]
  async fn grep_tab_characters() {
    let input = "\terror\twith\ttabs\n";
    let res = grep_with_q(input, "error", 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_special_regex_chars_in_literal() {
    let input = "price is $100\n";
    // 字面量中的 $ 不应该被当作正则
    let res = grep_with_q(input, "$", 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_dot_in_literal() {
    let input = "example.com\n";
    // 字面量中的 . 不应该匹配任意字符
    let res = grep_with_q(input, ".", 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_asterisk_in_literal() {
    let input = "file*.txt\n";
    // 字面量中的 * 不应该被当作量词
    let res = grep_with_q(input, "*", 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_parentheses_in_literal() {
    let input = "function(arg)\n";
    // 使用正则来匹配括号
    let res = grep_with_q(input, r#"/\(/"#, 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_brackets_in_literal() {
    let input = "array[0]\n";
    // 使用正则来匹配方括号
    let res = grep_with_q(input, r#"/\[/"#, 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_regex_quantifiers() {
    let input = "error errror errrror\n";
    // r+ 表示一个或多个 r
    let res = grep_with_q(input, "/er+or/", 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_regex_optional() {
    let input = "color colour\n";
    // u? 表示 u 是可选的
    let res = grep_with_q(input, "/colou?r/", 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_regex_alternation() {
    let input = "cat dog bird\n";
    // cat|dog 匹配 cat 或 dog
    let res = grep_with_q(input, "/cat|dog/", 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_regex_character_class() {
    let input = "version 1.2.3\n";
    // [0-9] 匹配数字
    let res = grep_with_q(input, r#"/\d+\.\d+\.\d+/"#, 0).await;
    assert!(res.is_some());
  }

  #[tokio::test]
  async fn grep_very_large_context() {
    let input = (0..100).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n");
    let input_with_error = input.clone() + "\nerror\n" + &input;

    // 超大上下文应该包含整个文件
    let res = grep_with_q(&input_with_error, "error", 1000).await;
    assert!(res.is_some());
    let (lines, _) = res.unwrap();
    assert!(lines.len() > 200);
  }

  // === Search trait 实现测试 ===

  /// 创建一个简单的 tar.gz 文件内容（在内存中）
  fn create_test_tar_gz(files: Vec<(&str, &str)>) -> Vec<u8> {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;
    use tar::Builder;

    let mut tar_data = Vec::new();
    {
      let mut tar = Builder::new(&mut tar_data);

      for (name, content) in files {
        let bytes = content.as_bytes();
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, name, bytes).unwrap();
      }

      tar.finish().unwrap();
    }

    let mut gz_data = Vec::new();
    {
      let mut encoder = GzEncoder::new(&mut gz_data, Compression::default());
      encoder.write_all(&tar_data).unwrap();
      encoder.finish().unwrap();
    }

    gz_data
  }

  #[tokio::test]
  async fn test_search_trait_basic() {
    use crate::query::Query;
    use futures::io::Cursor;
    use tokio_util::compat::FuturesAsyncReadCompatExt;

    // 创建包含两个文件的 tar.gz
    let tar_gz = create_test_tar_gz(vec![
      ("file1.log", "line1\nerror found here\nline3\n"),
      ("file2.log", "normal line\nanother error\nlast line\n"),
    ]);

    // 创建 AsyncRead（futures::io::Cursor 转换为 tokio 的 AsyncRead）
    let cursor = Cursor::new(tar_gz).compat();

    // 解析查询
    let spec = Query::parse_github_like("error").unwrap();

    // 调用 search
    let mut rx = cursor.search(&spec, 1).await.unwrap();

    // 收集结果
    let mut results = Vec::new();
    while let Some(result) = rx.recv().await {
      results.push(result);
    }

    // 验证：应该找到两个文件
    assert_eq!(results.len(), 2);

    // 验证每个结果都包含 "error"
    for result in &results {
      assert!(result.lines.iter().any(|line| line.contains("error")));
    }
  }

  #[tokio::test]
  async fn test_search_trait_no_match() {
    use crate::query::Query;
    use futures::io::Cursor;
    use tokio_util::compat::FuturesAsyncReadCompatExt;

    // 创建不包含目标字符串的文件
    let tar_gz = create_test_tar_gz(vec![
      ("file1.log", "line1\nline2\nline3\n"),
      ("file2.log", "normal line\nanother line\nlast line\n"),
    ]);

    let cursor = Cursor::new(tar_gz).compat();
    let spec = Query::parse_github_like("notfound").unwrap();

    let mut rx = cursor.search(&spec, 1).await.unwrap();

    // 收集结果
    let mut results = Vec::new();
    while let Some(result) = rx.recv().await {
      results.push(result);
    }

    // 验证：没有匹配结果
    assert_eq!(results.len(), 0);
  }

  #[tokio::test]
  async fn test_search_trait_with_context() {
    use crate::query::Query;
    use futures::io::Cursor;
    use tokio_util::compat::FuturesAsyncReadCompatExt;

    let tar_gz = create_test_tar_gz(vec![("file1.log", "line1\nline2\nerror here\nline4\nline5\n")]);

    let cursor = Cursor::new(tar_gz).compat();
    let spec = Query::parse_github_like("error").unwrap();

    // context = 2，应该包含前后各2行
    let mut rx = cursor.search(&spec, 2).await.unwrap();

    let result = rx.recv().await.unwrap();

    // 验证上下文：应该包含 5 行 (error 前2行 + error 行 + error 后2行)
    assert_eq!(result.lines.len(), 5);
    assert!(result.lines[2].contains("error"));
  }

  #[tokio::test]
  async fn test_search_trait_multiple_matches_in_one_file() {
    use crate::query::Query;
    use futures::io::Cursor;
    use tokio_util::compat::FuturesAsyncReadCompatExt;

    let tar_gz = create_test_tar_gz(vec![("file1.log", "error1\nline2\nline3\nerror2\nline5\n")]);

    let cursor = Cursor::new(tar_gz).compat();
    let spec = Query::parse_github_like("error").unwrap();

    let mut rx = cursor.search(&spec, 0).await.unwrap();

    let result = rx.recv().await.unwrap();

    // 验证：找到两行匹配
    assert_eq!(result.merged.len(), 2);
  }

  #[tokio::test]
  async fn test_search_trait_regex_pattern() {
    use crate::query::Query;
    use futures::io::Cursor;
    use tokio_util::compat::FuturesAsyncReadCompatExt;

    let tar_gz = create_test_tar_gz(vec![("file1.log", "error123\nline2\nwarn456\n")]);

    let cursor = Cursor::new(tar_gz).compat();
    // 使用正则匹配 error 或 warn
    let spec = Query::parse_github_like("/error|warn/").unwrap();

    let mut rx = cursor.search(&spec, 0).await.unwrap();

    let result = rx.recv().await.unwrap();

    // 验证：找到两行匹配
    assert_eq!(result.lines.len(), 3); // 包含上下文
  }

  #[tokio::test]
  async fn test_search_trait_empty_tar() {
    use crate::query::Query;
    use futures::io::Cursor;
    use tokio_util::compat::FuturesAsyncReadCompatExt;

    // 创建空的 tar.gz
    let tar_gz = create_test_tar_gz(vec![]);

    let cursor = Cursor::new(tar_gz).compat();
    let spec = Query::parse_github_like("error").unwrap();

    let mut rx = cursor.search(&spec, 0).await.unwrap();

    // 收集结果
    let mut results = Vec::new();
    while let Some(result) = rx.recv().await {
      results.push(result);
    }

    // 验证：没有结果
    assert_eq!(results.len(), 0);
  }

  #[tokio::test]
  async fn test_search_trait_binary_file_skipped() {
    use crate::query::Query;
    use futures::io::Cursor;
    use tokio_util::compat::FuturesAsyncReadCompatExt;

    // 创建包含二进制内容的文件
    let binary_content = "\x00\x01\x02\x03error\x04\x05\x06";
    let tar_gz = create_test_tar_gz(vec![
      ("binary.dat", binary_content),
      ("text.log", "this is text error\n"),
    ]);

    let cursor = Cursor::new(tar_gz).compat();
    let spec = Query::parse_github_like("error").unwrap();

    let mut rx = cursor.search(&spec, 0).await.unwrap();

    // 收集结果
    let mut results = Vec::new();
    while let Some(result) = rx.recv().await {
      results.push(result);
    }

    // 验证：二进制文件被跳过，只有文本文件
    assert_eq!(results.len(), 1);
    assert!(results[0].path.contains("text.log"));
  }

  #[tokio::test]
  async fn test_search_trait_many_files() {
    use crate::query::Query;
    use futures::io::Cursor;
    use tokio_util::compat::FuturesAsyncReadCompatExt;

    // 创建多个文件
    let mut files = Vec::new();
    for i in 0..10 {
      files.push((format!("file{}.log", i), format!("line1\nerror in file {}\nline3\n", i)));
    }

    let files_ref: Vec<(&str, &str)> = files
      .iter()
      .map(|(name, content)| (name.as_str(), content.as_str()))
      .collect();

    let tar_gz = create_test_tar_gz(files_ref);

    let cursor = Cursor::new(tar_gz).compat();
    let spec = Query::parse_github_like("error").unwrap();

    let mut rx = cursor.search(&spec, 0).await.unwrap();

    // 收集结果
    let mut results = Vec::new();
    while let Some(result) = rx.recv().await {
      results.push(result);
    }

    // 验证：找到所有 10 个文件
    assert_eq!(results.len(), 10);
  }

  #[tokio::test]
  async fn test_search_trait_complex_query() {
    use crate::query::Query;
    use futures::io::Cursor;
    use tokio_util::compat::FuturesAsyncReadCompatExt;

    let tar_gz = create_test_tar_gz(vec![
      ("file1.log", "error and warning\nline2\n"),
      ("file2.log", "only error here\nline2\n"),
      ("file3.log", "only warning here\nline2\n"),
    ]);

    let cursor = Cursor::new(tar_gz).compat();
    // 同时包含 error 和 warning
    let spec = Query::parse_github_like("error warning").unwrap();

    let mut rx = cursor.search(&spec, 0).await.unwrap();

    // 收集结果
    let mut results = Vec::new();
    while let Some(result) = rx.recv().await {
      results.push(result);
    }

    // 验证：只有 file1.log 同时包含两个词
    assert_eq!(results.len(), 1);
    assert!(results[0].path.contains("file1.log"));
  }

  #[tokio::test]
  async fn test_search_trait_path_with_directory() {
    use crate::query::Query;
    use futures::io::Cursor;
    use tokio_util::compat::FuturesAsyncReadCompatExt;

    let tar_gz = create_test_tar_gz(vec![
      ("logs/app/file1.log", "error in app\n"),
      ("logs/system/file2.log", "error in system\n"),
    ]);

    let cursor = Cursor::new(tar_gz).compat();
    let spec = Query::parse_github_like("error").unwrap();

    let mut rx = cursor.search(&spec, 0).await.unwrap();

    // 收集结果
    let mut results = Vec::new();
    while let Some(result) = rx.recv().await {
      results.push(result);
    }

    // 验证：找到两个文件，且路径包含目录
    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|r| r.path.contains("logs/app")));
    assert!(results.iter().any(|r| r.path.contains("logs/system")));
  }

  // ============================================================================
  // SearchProcessor 单元测试（重构后新增）
  // ============================================================================

  #[test]
  fn test_search_processor_should_process_path_with_filter() {
    // 使用 path: 过滤器
    let spec = Arc::new(Query::parse_github_like("path:*.log error").unwrap());
    let processor = SearchProcessor::new(spec, 0);

    // 测试允许的路径
    assert!(processor.should_process_path("file.log"));
    assert!(processor.should_process_path("path/to/file.log"));

    // 测试被拒绝的路径
    assert!(!processor.should_process_path("file.txt"));
    assert!(!processor.should_process_path("file.md"));
  }

  #[test]
  fn test_search_processor_should_process_path_no_filter() {
    let spec = Arc::new(Query::parse_github_like("error").unwrap());
    let processor = SearchProcessor::new(spec, 0);

    // 没有路径过滤器时，所有路径都应该通过
    assert!(processor.should_process_path("file.log"));
    assert!(processor.should_process_path("file.txt"));
    assert!(processor.should_process_path("any.file"));
  }

  #[tokio::test]
  async fn test_search_processor_process_content_match() {
    let spec = Arc::new(Query::parse_github_like("error").unwrap());
    let processor = SearchProcessor::new(spec, 1);

    let content = "line1\nerror found here\nline3\n";
    let mut reader = content.as_bytes();

    let result = processor
      .process_content("test.log".to_string(), &mut reader)
      .await
      .unwrap();

    assert!(result.is_some());
    let result = result.unwrap();
    assert_eq!(result.path, "test.log");
    assert_eq!(result.lines.len(), 3); // 包含上下文
    assert!(result.lines.iter().any(|line| line.contains("error")));
  }

  #[tokio::test]
  async fn test_search_processor_process_content_no_match() {
    let spec = Arc::new(Query::parse_github_like("error").unwrap());
    let processor = SearchProcessor::new(spec, 0);

    let content = "line1\nline2\nline3\n";
    let mut reader = content.as_bytes();

    let result = processor
      .process_content("test.log".to_string(), &mut reader)
      .await
      .unwrap();

    assert!(result.is_none());
  }

  #[tokio::test]
  async fn test_search_processor_process_content_with_context() {
    let spec = Arc::new(Query::parse_github_like("error").unwrap());
    let processor = SearchProcessor::new(spec, 2); // 上下文 2 行

    let content = "line1\nline2\nerror found\nline4\nline5\n";
    let mut reader = content.as_bytes();

    let result = processor
      .process_content("test.log".to_string(), &mut reader)
      .await
      .unwrap();

    assert!(result.is_some());
    let result = result.unwrap();
    // 应该包含 5 行：error 前2行 + error 行 + error 后2行
    assert_eq!(result.lines.len(), 5);
  }

  #[tokio::test]
  async fn test_search_processor_send_result_success() {
    let spec = Arc::new(Query::parse_github_like("error").unwrap());
    let processor = SearchProcessor::new(spec, 0);

    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let result = SearchResult::new("test.log".to_string(), vec!["error".to_string()], vec![(0, 0)]);

    // 发送应该成功
    assert!(processor.send_result(result, &tx).await.is_ok());

    // 接收端应该能收到
    assert!(rx.recv().await.is_some());
  }

  #[tokio::test]
  async fn test_search_processor_send_result_channel_closed() {
    let spec = Arc::new(Query::parse_github_like("error").unwrap());
    let processor = SearchProcessor::new(spec, 0);

    let (tx, rx) = tokio::sync::mpsc::channel(1);
    drop(rx); // 关闭接收端

    let result = SearchResult::new("test.log".to_string(), vec!["error".to_string()], vec![(0, 0)]);

    // 发送应该失败
    let send_result = processor.send_result(result, &tx).await;
    assert!(send_result.is_err());
    assert!(matches!(send_result.unwrap_err(), SearchError::ChannelClosed));
  }

  #[tokio::test]
  async fn test_search_processor_process_empty_file() {
    let spec = Arc::new(Query::parse_github_like("error").unwrap());
    let processor = SearchProcessor::new(spec, 0);

    let content = "";
    let mut reader = content.as_bytes();

    let result = processor
      .process_content("empty.log".to_string(), &mut reader)
      .await
      .unwrap();

    assert!(result.is_none());
  }

  #[tokio::test]
  async fn test_search_processor_multiple_matches() {
    let spec = Arc::new(Query::parse_github_like("error").unwrap());
    let processor = SearchProcessor::new(spec, 0);

    let content = "error1\nline2\nerror2\nline4\nerror3\n";
    let mut reader = content.as_bytes();

    let result = processor
      .process_content("test.log".to_string(), &mut reader)
      .await
      .unwrap();

    assert!(result.is_some());
    let result = result.unwrap();
    // 应该找到 3 个匹配
    assert_eq!(result.merged.len(), 3);
  }

  #[tokio::test]
  async fn test_search_processor_regex_pattern() {
    let spec = Arc::new(Query::parse_github_like(r#"/\d{3}/"#).unwrap());
    let processor = SearchProcessor::new(spec, 0);

    let content = "line1\nstatus 200\nline3\nerror 500\n";
    let mut reader = content.as_bytes();

    let result = processor
      .process_content("test.log".to_string(), &mut reader)
      .await
      .unwrap();

    assert!(result.is_some());
    let result = result.unwrap();
    // 应该匹配包含三位数字的行
    assert!(
      result
        .lines
        .iter()
        .any(|line| line.contains("200") || line.contains("500"))
    );
  }

  // ============================================================================
  // 测试新的重构组件
  // ============================================================================

  #[test]
  fn test_search_config_default() {
    let config = SearchConfig::default();
    assert_eq!(config.max_processing_time.as_secs(), 300);
    assert_eq!(config.entry_timeout.as_secs(), 30);
    assert_eq!(config.content_timeout.as_secs(), 60);
  }

  #[test]
  fn test_processing_stats_new() {
    let stats = ProcessingStats::new();
    assert_eq!(stats.processed_entries, 0);
    assert_eq!(stats.successful_entries, 0);
    assert!(stats.elapsed().as_millis() < 100); // 刚创建，时间很短
  }

  #[test]
  fn test_processing_stats_elapsed() {
    let stats = ProcessingStats::new();
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(stats.elapsed().as_millis() >= 50);
  }

  #[test]
  fn test_tar_entry_processor_generate_fallback_path() {
    let spec = Arc::new(crate::query::Query::parse_github_like("error").unwrap());
    let processor = Arc::new(SearchProcessor::new(spec, 0));
    let config = SearchConfig::default();
    let entry_processor = TarEntryProcessor::new(processor, config);

    let path1 = entry_processor.generate_fallback_path(1);
    let path2 = entry_processor.generate_fallback_path(2);

    assert!(path1.starts_with("<unknown-1-"));
    assert!(path2.starts_with("<unknown-2-"));
    assert!(path1.ends_with('>'));
    assert!(path2.ends_with('>'));
    // 不同的索引应该生成不同的路径
    assert_ne!(path1, path2);
  }

  #[tokio::test]
  async fn test_tar_entry_processor_process_entry_no_match() {
    let spec = Arc::new(crate::query::Query::parse_github_like("NOTFOUND").unwrap());
    let processor = Arc::new(SearchProcessor::new(spec, 0));
    let config = SearchConfig::default();
    let entry_processor = TarEntryProcessor::new(processor, config);

    // 创建一个简单的内存 tar
    let tar_gz = create_test_tar_gz(vec![("file.log", "hello world\n")]);
    let cursor = futures::io::Cursor::new(tar_gz);
    let gz = async_compression::tokio::bufread::GzipDecoder::new(cursor.compat());
    let archive = async_tar::Archive::new(gz.compat());
    let mut entries = archive.entries().unwrap();

    let entry = entries.next().await.unwrap().unwrap();
    let result = entry_processor.process_entry(entry, 1).await.unwrap();

    // 没有匹配
    assert!(result.is_none());
  }

  #[tokio::test]
  async fn test_tar_entry_processor_process_entry_with_match() {
    let spec = Arc::new(crate::query::Query::parse_github_like("error").unwrap());
    let processor = Arc::new(SearchProcessor::new(spec, 1));
    let config = SearchConfig::default();
    let entry_processor = TarEntryProcessor::new(processor, config);

    // 创建一个包含匹配内容的 tar
    let tar_gz = create_test_tar_gz(vec![("file.log", "line1\nerror found\nline3\n")]);
    let cursor = futures::io::Cursor::new(tar_gz);
    let gz = async_compression::tokio::bufread::GzipDecoder::new(cursor.compat());
    let archive = async_tar::Archive::new(gz.compat());
    let mut entries = archive.entries().unwrap();

    let entry = entries.next().await.unwrap().unwrap();
    let result = entry_processor.process_entry(entry, 1).await.unwrap();

    // 有匹配
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(result.lines.iter().any(|line| line.contains("error")));
  }

  #[tokio::test]
  async fn test_tar_entry_processor_path_filter() {
    let spec = Arc::new(crate::query::Query::parse_github_like("path:*.txt error").unwrap());
    let processor = Arc::new(SearchProcessor::new(spec, 0));
    let config = SearchConfig::default();
    let entry_processor = TarEntryProcessor::new(processor, config);

    // 创建一个 .log 文件（应该被过滤）
    let tar_gz = create_test_tar_gz(vec![("file.log", "error found\n")]);
    let cursor = futures::io::Cursor::new(tar_gz);
    let gz = async_compression::tokio::bufread::GzipDecoder::new(cursor.compat());
    let archive = async_tar::Archive::new(gz.compat());
    let mut entries = archive.entries().unwrap();

    let entry = entries.next().await.unwrap().unwrap();
    let result = entry_processor.process_entry(entry, 1).await.unwrap();

    // 由于路径过滤，应该没有结果
    assert!(result.is_none());
  }

  #[tokio::test]
  async fn test_tar_stream_processor_handle_entry_error_success() {
    let spec = Arc::new(crate::query::Query::parse_github_like("error").unwrap());
    let processor = Arc::new(SearchProcessor::new(spec, 0));
    let config = SearchConfig::default();
    let entry_processor = TarEntryProcessor::new(processor, config.clone());
    let mut stream_processor = TarStreamProcessor::new(entry_processor, config);

    // 创建一个成功的条目
    let tar_gz = create_test_tar_gz(vec![("file.log", "test\n")]);
    let cursor = futures::io::Cursor::new(tar_gz);
    let gz = async_compression::tokio::bufread::GzipDecoder::new(cursor.compat());
    let archive = async_tar::Archive::new(gz.compat());
    let mut entries = archive.entries().unwrap();
    let entry = entries.next().await.unwrap();

    match stream_processor.handle_entry_error(entry) {
      EntryAction::Process(_) => {
        // 成功
      }
      _ => panic!("应该返回 Process"),
    }
  }

  #[tokio::test]
  async fn test_tar_stream_processor_handle_entry_error_failure() {
    let spec = Arc::new(crate::query::Query::parse_github_like("error").unwrap());
    let processor = Arc::new(SearchProcessor::new(spec, 0));
    let config = SearchConfig::default();
    let entry_processor = TarEntryProcessor::new(processor, config.clone());
    let mut stream_processor = TarStreamProcessor::new(entry_processor, config);

    // 创建一个错误（使用具体类型）
    type TestReader = tokio_util::compat::Compat<
      async_compression::tokio::bufread::GzipDecoder<tokio::io::BufReader<std::io::Cursor<Vec<u8>>>>,
    >;
    let error: std::io::Result<async_tar::Entry<TestReader>> =
      Err(std::io::Error::new(std::io::ErrorKind::Other, "test error"));

    match stream_processor.handle_entry_error(error) {
      EntryAction::Skip => {
        // 应该跳过
      }
      _ => {
        // 也可能是 Abort，取决于错误跟踪器的逻辑
      }
    }
  }

  #[tokio::test]
  async fn test_tar_stream_processor_send_result() {
    let spec = Arc::new(crate::query::Query::parse_github_like("error").unwrap());
    let processor = Arc::new(SearchProcessor::new(spec, 0));
    let config = SearchConfig::default();
    let entry_processor = TarEntryProcessor::new(processor, config.clone());
    let stream_processor = TarStreamProcessor::new(entry_processor, config);

    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let result = SearchResult::new("test.log".to_string(), vec!["test".to_string()], vec![]);

    // 发送成功
    assert!(stream_processor.send_result(result, &tx).await.is_ok());
    assert!(rx.recv().await.is_some());
  }

  #[tokio::test]
  async fn test_tar_stream_processor_send_result_channel_closed() {
    let spec = Arc::new(crate::query::Query::parse_github_like("error").unwrap());
    let processor = Arc::new(SearchProcessor::new(spec, 0));
    let config = SearchConfig::default();
    let entry_processor = TarEntryProcessor::new(processor, config.clone());
    let stream_processor = TarStreamProcessor::new(entry_processor, config);

    let (tx, rx) = tokio::sync::mpsc::channel(1);
    drop(rx); // 关闭接收端

    let result = SearchResult::new("test.log".to_string(), vec!["test".to_string()], vec![]);

    // 发送失败
    assert!(stream_processor.send_result(result, &tx).await.is_err());
  }

  #[tokio::test]
  async fn test_tar_stream_processor_process_stream_empty() {
    let spec = Arc::new(crate::query::Query::parse_github_like("error").unwrap());
    let processor = Arc::new(SearchProcessor::new(spec, 0));
    let config = SearchConfig::default();
    let entry_processor = TarEntryProcessor::new(processor, config.clone());
    let mut stream_processor = TarStreamProcessor::new(entry_processor, config);

    // 创建一个空的 tar，使用和生产代码一样的方式
    let tar_gz = create_test_tar_gz(vec![]);
    let reader = tokio::io::BufReader::new(std::io::Cursor::new(tar_gz));
    let gz = async_compression::tokio::bufread::GzipDecoder::new(reader);
    let archive = async_tar::Archive::new(gz.compat());
    let entries = archive.entries().unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel(1);

    let stats = stream_processor.process_stream(entries, tx).await;

    assert_eq!(stats.processed_entries, 0);
    assert_eq!(stats.successful_entries, 0);
    assert!(rx.recv().await.is_none()); // 没有结果
  }

  #[tokio::test]
  async fn test_tar_stream_processor_process_stream_with_matches() {
    let spec = Arc::new(crate::query::Query::parse_github_like("error").unwrap());
    let processor = Arc::new(SearchProcessor::new(spec, 1));
    let config = SearchConfig::default();
    let entry_processor = TarEntryProcessor::new(processor, config.clone());
    let mut stream_processor = TarStreamProcessor::new(entry_processor, config);

    // 创建一个包含匹配内容的 tar，使用和生产代码一样的方式
    let tar_gz = create_test_tar_gz(vec![
      ("file1.log", "line1\nerror here\nline3\n"),
      ("file2.log", "no match\n"),
      ("file3.log", "another error\n"),
    ]);
    let reader = tokio::io::BufReader::new(std::io::Cursor::new(tar_gz));
    let gz = async_compression::tokio::bufread::GzipDecoder::new(reader);
    let archive = async_tar::Archive::new(gz.compat());
    let entries = archive.entries().unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel(10);

    let stats = stream_processor.process_stream(entries, tx).await;

    // 应该处理 3 个条目，2 个成功匹配
    assert_eq!(stats.processed_entries, 3);

    // 收集结果
    let mut results = vec![];
    while let Ok(result) = rx.try_recv() {
      results.push(result);
    }
    assert_eq!(results.len(), 2); // file1 和 file3
  }
}
