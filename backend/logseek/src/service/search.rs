use std::{
  io::{self},
  sync::Arc,
};

// use futures::io::AsyncReadExt as FuturesAsyncReadExt;
use chardetng::EncodingDetector;
use encoding_rs::{BIG5, EUC_KR, Encoding, GBK, SHIFT_JIS, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, BufReader};
use tracing::{debug, warn};

#[derive(Debug, Error)]
pub enum SearchError {
  #[error("IO错误: path={path}, error={error}")]
  Io { path: String, error: String },
  #[error("Channel 已关闭: 接收端已断开连接")]
  ChannelClosed,
}

// 为 io::Error 提供自动转换（需要提供路径上下文）
impl From<io::Error> for SearchError {
  fn from(err: io::Error) -> Self {
    SearchError::Io {
      path: "unknown".to_string(), // 如果没有路径信息，使用默认值
      error: err.to_string(),
    }
  }
}

use crate::query::Query;

// ============================================================================
// 配置和辅助类型（已简化：删除未使用的旧配置/统计结构）
// ============================================================================

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
pub struct SearchProcessor {
  pub spec: Arc<Query>,
  pub context_lines: usize,
  pub encoding: Option<String>, // 指定的编码名称（如 "GBK", "UTF-8" 等）
}

impl SearchProcessor {
  /// 创建新的搜索处理器
  pub fn new(spec: Arc<Query>, context_lines: usize) -> Self {
    Self {
      spec,
      context_lines,
      encoding: None,
    }
  }

  /// 创建新的搜索处理器（带编码指定）
  pub fn new_with_encoding(spec: Arc<Query>, context_lines: usize, encoding: Option<String>) -> Self {
    Self {
      spec,
      context_lines,
      encoding,
    }
  }

  /// 检查路径是否应该被处理（纯函数，易于测试）
  ///
  /// # Examples
  /// ```ignore
  /// let processor = SearchProcessor::new(...);
  /// assert!(processor.should_process_path("file.log"));
  /// assert!(!processor.should_process_path("file.txt"));
  /// ```
  pub fn should_process_path(&self, path: &str) -> bool {
    self.spec.path_filter.is_allowed(path)
  }

  /// 检查路径是否应该被处理（扩展：支持额外的路径过滤谓词，与用户查询的 path: 规则做 AND）
  ///
  /// - extra 为 None 时，行为与 should_process_path 完全一致
  /// - extra 为 Some 时，先检查 extra.is_allowed(path)，若不通过则直接拒绝
  pub fn should_process_path_with(&self, path: &str, extra: Option<&crate::query::PathFilter>) -> bool {
    if let Some(f) = extra
      && !f.is_allowed(path)
    {
      return false;
    }
    self.should_process_path(path)
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
  pub async fn process_content<R: AsyncRead + Unpin>(
    &self,
    path: String,
    reader: &mut R,
  ) -> Result<Option<SearchResult>, SearchError> {
    match grep_context(reader, &self.spec, self.context_lines, self.encoding.as_deref()).await? {
      Some((lines, merged, encoding)) => {
        debug!("找到匹配: {} ({} 行)", path, merged.len());
        Ok(Some(SearchResult::new(path, lines, merged, encoding)))
      }
      None => Ok(None),
    }
  }

  /// 发送结果到 channel（可单独测试）
  ///
  /// # 返回
  /// - `Ok(())`: 发送成功
  /// - `Err(SearchError::ChannelClosed)`: 接收端已关闭
  pub async fn send_result(
    &self,
    result: SearchResult,
    tx: &tokio::sync::mpsc::Sender<SearchResult>,
  ) -> Result<(), SearchError> {
    tx.send(result).await.map_err(|_| SearchError::ChannelClosed)?;
    Ok(())
  }
}

/// 返回检测到的编码，如果无法确定则返回 None
fn detect_encoding(sample: &[u8]) -> Option<&'static Encoding> {
  // 检查 BOM（字节顺序标记）- 最可靠的检测方式
  if sample.len() >= 2 {
    match &sample[0..2] {
      [0xFF, 0xFE] => {
        // UTF-16 LE BOM
        debug!("检测到 UTF-16 LE BOM");
        return Some(UTF_16LE);
      }
      [0xFE, 0xFF] => {
        // UTF-16 BE BOM
        debug!("检测到 UTF-16 BE BOM");
        return Some(UTF_16BE);
      }
      _ => {}
    }
  }

  if sample.len() >= 3
    && let [0xEF, 0xBB, 0xBF] = &sample[0..3]
  {
    debug!("检测到 UTF-8 BOM");
    return Some(UTF_8);
  }

  // 优先检测是否为有效的 UTF-8
  // 处理样本可能在多字节字符中间截断的情况
  match std::str::from_utf8(sample) {
    Ok(_) => {
      // 样本完全是有效的 UTF-8
      debug!("样本是有效的 UTF-8，使用 UTF-8 编码");
      return Some(UTF_8);
    }
    Err(e) => {
      // 检查是否只是因为末尾截断导致的错误
      let valid_up_to = e.valid_up_to();

      // 如果大部分内容是有效的 UTF-8，只是末尾可能被截断
      // 我们认为这是 UTF-8 文件（允许末尾最多3个字节的不完整字符）
      if valid_up_to > 0 && sample.len() - valid_up_to <= 3 {
        // 验证前面的部分确实是有效的 UTF-8
        if std::str::from_utf8(&sample[..valid_up_to]).is_ok() {
          debug!(
            "样本前 {} 字节是有效的 UTF-8（末尾 {} 字节可能被截断），使用 UTF-8 编码",
            valid_up_to,
            sample.len() - valid_up_to
          );
          return Some(UTF_8);
        }
      }
      // 如果有效部分太少，说明不是 UTF-8，继续使用 chardetng 检测
    }
  }

  // 使用 chardetng 进行编码检测
  let mut detector = EncodingDetector::new();
  detector.feed(sample, true); // last=true 表示这是最后一块数据
  let detected_encoding = detector.guess(None, true); // tld=None, allow_utf8=true

  debug!("chardetng 检测到编码: {}", detected_encoding.name());
  Some(detected_encoding)
}

/// 读取 UTF-8 编码的文件行
async fn read_lines_utf8<R: AsyncRead + Unpin>(
  buf_reader: &mut BufReader<R>,
  sample: Vec<u8>,
) -> Result<Vec<String>, SearchError> {
  use tokio::io::AsyncBufReadExt as _;
  let mut lines: Vec<String> = Vec::new();

  // 将样本转换为字符串并处理其中的行
  let sample_str = match String::from_utf8(sample.clone()) {
    Ok(s) => s,
    Err(e) => {
      // 检查是否只是末尾被截断
      let valid_up_to = e.utf8_error().valid_up_to();
      if valid_up_to > 0 && sample.len() - valid_up_to <= 3 {
        // 只使用有效的部分，丢弃末尾不完整的字节
        debug!(
          "样本末尾 {} 字节被截断，使用前 {} 字节",
          sample.len() - valid_up_to,
          valid_up_to
        );
        String::from_utf8(sample[..valid_up_to].to_vec()).expect("valid_up_to 应该保证这部分是有效的 UTF-8")
      } else {
        // 如果不是末尾截断问题，使用 lossy 转换
        warn!("样本包含无效 UTF-8，使用 lossy 转换");
        String::from_utf8_lossy(&e.into_bytes()).into_owned()
      }
    }
  };

  // 处理样本中的完整行
  let mut sample_lines: Vec<&str> = sample_str.lines().collect();
  let last_line_incomplete = !sample_str.ends_with('\n') && !sample_str.ends_with('\r');

  // 如果样本最后一行不完整，需要与后续读取的内容合并
  let mut incomplete_line = if last_line_incomplete {
    sample_lines.pop().map(|s| s.to_string())
  } else {
    None
  };

  // 添加样本中的完整行
  for line in sample_lines {
    lines.push(line.to_string());
  }

  // 继续读取剩余行（使用字节读取以处理可能的UTF-8错误）
  let mut line = incomplete_line.take().unwrap_or_default();
  loop {
    let mut temp_bytes = Vec::new();
    let n = buf_reader.read_until(b'\n', &mut temp_bytes).await?;
    if n == 0 {
      if !line.is_empty() {
        lines.push(line.trim_end_matches(['\r', '\n']).to_string());
      }
      break;
    }

    // 尝试将字节转换为字符串
    let temp_line = match String::from_utf8(temp_bytes.clone()) {
      Ok(s) => s,
      Err(e) => {
        // 处理末尾截断的情况
        let valid_up_to = e.utf8_error().valid_up_to();
        if valid_up_to > 0 && temp_bytes.len() - valid_up_to <= 3 {
          // 只使用有效的部分
          String::from_utf8(temp_bytes[..valid_up_to].to_vec())
            .unwrap_or_else(|_| String::from_utf8_lossy(&temp_bytes).into_owned())
        } else {
          // 使用 lossy 转换
          String::from_utf8_lossy(&temp_bytes).into_owned()
        }
      }
    };

    line.push_str(&temp_line);
    let trimmed = line.trim_end_matches(['\r', '\n']);
    if trimmed != line {
      // 找到完整行
      lines.push(trimmed.to_string());
      line.clear();
    }
  }

  Ok(lines)
}

/// 读取 UTF-16 编码的文件行（LE 或 BE）
async fn read_lines_utf16<R: AsyncRead + Unpin>(
  buf_reader: &mut BufReader<R>,
  encoding: &'static Encoding,
  sample: Vec<u8>,
) -> Result<Vec<String>, SearchError> {
  let mut lines: Vec<String> = Vec::new();
  let mut buffer = Vec::new();

  // 处理样本（跳过 BOM，如果存在）
  let sample_start = if sample.len() >= 2 && (sample[0..2] == [0xFF, 0xFE] || sample[0..2] == [0xFE, 0xFF]) {
    2 // 跳过 BOM
  } else {
    0
  };
  buffer.extend_from_slice(&sample[sample_start..]);

  // 读取剩余数据
  let mut temp_buf = vec![0u8; 8192];
  loop {
    let n = buf_reader.read(&mut temp_buf).await?;
    if n == 0 {
      break;
    }
    buffer.extend_from_slice(&temp_buf[..n]);
  }

  // UTF-16 需要确保字节数是偶数（每个字符 2 字节）
  if buffer.len() % 2 != 0 {
    warn!("UTF-16 文件字节数不是偶数，可能不完整");
    buffer.pop(); // 移除最后一个字节
  }

  // 解码整个缓冲区
  let (decoded, _, had_errors) = encoding.decode(&buffer);

  if had_errors {
    warn!("UTF-16 解码过程中遇到错误，但继续处理");
  }

  // 按行分割
  for line in decoded.lines() {
    lines.push(line.to_string());
  }

  // 处理最后一行（可能没有换行符）
  let decoded_str = decoded.as_ref();
  if !decoded_str.ends_with('\n')
    && !decoded_str.ends_with('\r')
    && let Some(last_line) = decoded_str.lines().last()
    && !last_line.is_empty()
  {
    // 如果最后一行已经在 lines 中，不需要重复添加
    if lines.last().is_none() || lines.last() != Some(&last_line.to_string()) {
      lines.push(last_line.to_string());
    }
  }

  Ok(lines)
}

/// 读取非 UTF-8 编码的文件行（如 GBK）
async fn read_lines_with_encoding<R: AsyncRead + Unpin>(
  buf_reader: &mut BufReader<R>,
  encoding: &'static Encoding,
  sample: Vec<u8>,
) -> Result<Vec<String>, SearchError> {
  let mut lines: Vec<String> = Vec::new();
  let mut buffer = Vec::new();

  // 处理样本
  buffer.extend_from_slice(&sample);

  // 读取剩余数据
  let mut temp_buf = vec![0u8; 8192];
  loop {
    let n = buf_reader.read(&mut temp_buf).await?;
    if n == 0 {
      break;
    }
    buffer.extend_from_slice(&temp_buf[..n]);
  }

  // 解码整个缓冲区
  let (decoded, _, had_errors) = encoding.decode(&buffer);

  if had_errors {
    warn!("解码过程中遇到错误，但继续处理");
  }

  // 按行分割
  for line in decoded.lines() {
    lines.push(line.to_string());
  }

  // 处理最后一行（可能没有换行符）
  let decoded_str = decoded.as_ref();
  if !decoded_str.ends_with('\n')
    && !decoded_str.ends_with('\r')
    && let Some(last_line) = decoded_str.lines().last()
    && !last_line.is_empty()
  {
    // 如果最后一行已经在 lines 中，不需要重复添加
    if lines.last().is_none() || lines.last() != Some(&last_line.to_string()) {
      lines.push(last_line.to_string());
    }
  }

  Ok(lines)
}

fn is_probably_text_bytes(sample: &[u8]) -> bool {
  if sample.is_empty() {
    return true;
  }
  // 包含 null 字节的文件通常不是文本文件
  if sample.contains(&0) {
    return false;
  }

  // 先检查 UTF-8，因为 UTF-8 可能包含多字节字符（如 emoji），可打印字符比例可能较低
  if std::str::from_utf8(sample).is_ok() {
    return true;
  }

  // 计算可打印字符比例
  let printable = sample
    .iter()
    .filter(|b| matches!(**b, 0x09 | 0x0A | 0x0D | 0x20..=0x7E))
    .count();
  let ratio = printable as f32 / sample.len() as f32;

  // 如果可打印字符比例 >= 95%，肯定是文本
  if ratio >= 0.95 {
    return true;
  }

  // 如果可打印字符比例太低（< 50%），不太可能是文本文件
  if ratio < 0.5 {
    return false;
  }

  // 使用 chardetng 检测编码
  // chardetng 可以检测多种编码，如果置信度高，通常是文本文件
  let mut detector = EncodingDetector::new();
  detector.feed(sample, true);
  let (_, confidence) = detector.guess_assess(None, true);

  // 如果置信度高，认为是文本文件
  confidence
}

pub async fn grep_context<R: AsyncRead + Unpin>(
  reader: &mut R,
  spec: &crate::query::Query,
  context_lines: usize,
  encoding_qualifier: Option<&str>,
) -> Result<Option<(Vec<String>, Vec<(usize, usize)>, Option<String>)>, SearchError> {
  debug!(
    "开始文本搜索，上下文行数: {}, 搜索条件数: {}",
    context_lines,
    spec.terms.len()
  );

  // 第一步：读取样本进行编码检测
  let mut buf_reader = BufReader::new(reader);
  let mut sample = Vec::with_capacity(4096);
  let mut temp_buf = vec![0u8; 4096];
  let mut total_read = 0;

  // 读取前 4096 字节作为样本
  while total_read < 4096 {
    let n = buf_reader.read(&mut temp_buf[total_read..]).await?;
    if n == 0 {
      break;
    }
    let end = total_read + n;
    sample.extend_from_slice(&temp_buf[total_read..end]);
    total_read = end;
  }

  // 检查是否为文本文件
  if !is_probably_text_bytes(&sample) {
    debug!("文件不是文本格式，跳过搜索");
    return Ok(None);
  }

  // 检测编码：如果指定了 encoding 限定词，使用指定的编码；否则自动检测
  let (encoding, encoding_name) = if let Some(enc_name) = encoding_qualifier {
    // 用户指定了编码，尝试查找对应的编码
    let enc_opt = Encoding::for_label(enc_name.as_bytes()).or_else(|| {
      // 尝试一些常见的别名
      match enc_name.to_uppercase().as_str() {
        "UTF8" | "UTF-8" => Some(UTF_8),
        "GBK" => Some(GBK),
        "BIG5" | "BIG-5" => Some(BIG5),
        "SHIFT-JIS" | "SHIFT_JIS" | "SJIS" => Some(SHIFT_JIS),
        "EUC-KR" | "EUC_KR" => Some(EUC_KR),
        "WINDOWS-1252" | "WINDOWS_1252" | "CP1252" => Some(WINDOWS_1252),
        "ISO-8859-1" | "ISO_8859_1" | "LATIN1" | "LATIN-1" => Encoding::for_label(b"ISO-8859-1"),
        "UTF-16LE" | "UTF16LE" | "UTF-16-LE" => Some(UTF_16LE),
        "UTF-16BE" | "UTF16BE" | "UTF-16-BE" => Some(UTF_16BE),
        _ => None,
      }
    });
    match enc_opt {
      Some(enc) => {
        debug!("使用指定的编码: {} ({})", enc_name, enc.name());
        (enc, enc.name().to_string())
      }
      None => {
        warn!("无法识别的编码名称: {}，回退到自动检测", enc_name);
        // 回退到自动检测
        match detect_encoding(&sample) {
          Some(enc) => {
            let name = enc.name().to_string();
            debug!("自动检测到编码: {}", name);
            (enc, name)
          }
          None => {
            debug!("无法确定文件编码，跳过搜索");
            return Ok(None);
          }
        }
      }
    }
  } else {
    // 未指定编码，自动检测
    match detect_encoding(&sample) {
      Some(enc) => {
        let name = enc.name().to_string();
        debug!("自动检测到编码: {}", name);
        (enc, name)
      }
      None => {
        debug!("无法确定文件编码，跳过搜索");
        return Ok(None);
      }
    }
  };

  // 如果检测到非 UTF-8 编码，需要转换
  let lines = if encoding == UTF_8 {
    // UTF-8 编码，直接使用 read_line
    read_lines_utf8(&mut buf_reader, sample).await?
  } else if encoding == UTF_16LE || encoding == UTF_16BE {
    // UTF-16 编码需要特殊处理（双字节编码）
    read_lines_utf16(&mut buf_reader, encoding, sample).await?
  } else {
    // 其他非 UTF-8 编码，需要转换
    read_lines_with_encoding(&mut buf_reader, encoding, sample).await?
  };

  // 始终返回编码名称
  let detected_encoding = Some(encoding_name);

  if lines.is_empty() {
    return Ok(None);
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

  debug!("找到{}行匹配结果，开始生成上下文区间", matched_lines.len());

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

  Ok(Some((lines, merged, detected_encoding)))
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
  pub path: String,
  pub lines: Vec<String>,
  pub merged: Vec<(usize, usize)>,
  /// 文件编码（如果不是 UTF-8，则包含编码名称，如 "GBK"）
  pub encoding: Option<String>,
}

impl SearchResult {
  fn new(path: String, lines: Vec<String>, merged: Vec<(usize, usize)>, encoding: Option<String>) -> Self {
    Self {
      path,
      lines,
      merged,
      encoding,
    }
  }
}

/// 流式搜索事件
///
/// 用于在 NDJSON 流中表示不同类型的事件：
/// - Success: 搜索结果成功
/// - Error: 搜索过程中发生错误（错误不会立即终止搜索）
/// - Complete: 单个来源搜索完成
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum SearchEvent {
  /// 搜索成功的结果
  #[serde(rename = "result")]
  Success(SearchResult),

  /// 搜索过程中的错误
  #[serde(rename = "error")]
  Error {
    /// 错误来源（如源索引、Agent ID 等）
    source: String,
    /// 错误信息
    message: String,
    /// 是否继续搜索其他源（true 表示错误非致命）
    recoverable: bool,
  },

  /// 来源搜索完成
  #[serde(rename = "complete")]
  Complete {
    /// 来源索引或标识
    source: String,
    /// 搜索的总耗时（毫秒）
    elapsed_ms: u64,
  },
}

// 旧的 Tar* 处理器与错误跟踪器已移除（改用 EntryStream 抽象）

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
    grep_context(&mut r, &spec, ctx, None)
      .await
      .ok()
      .flatten()
      .map(|(lines, merged, _)| (lines, merged))
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
    let res = grep_context(&mut r, &spec, 1, None).await.ok().flatten();
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
      None,
    );
    assert_eq!(result.path, "test.log");
    assert_eq!(result.lines.len(), 2);
    assert_eq!(result.merged.len(), 1);
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

  // 测试辅助：运行基于 tar.gz 字节的搜索并收集所有结果
  async fn run_tar_search_bytes(tar_gz: Vec<u8>, spec: &Query, ctx: usize) -> Vec<SearchResult> {
    use futures::io::Cursor;
    use tokio::sync::mpsc;
    use tokio_util::compat::FuturesAsyncReadCompatExt;

    let cursor = Cursor::new(tar_gz).compat();
    let mut stream = crate::service::entry_stream::TarGzEntryStream::new(cursor)
      .await
      .unwrap();
    let proc = Arc::new(SearchProcessor::new(Arc::new(spec.clone()), ctx));
    let mut esp = crate::service::entry_stream::EntryStreamProcessor::new(proc);
    let (tx, mut rx) = mpsc::channel::<SearchEvent>(64);

    let handle = tokio::spawn(async move {
      let _ = esp.process_stream(&mut stream, tx).await;
    });

    let mut results = Vec::new();
    while let Some(event) = rx.recv().await {
      if let SearchEvent::Success(result) = event {
        results.push(result);
      }
    }
    let _ = handle.await;
    results
  }

  #[tokio::test]
  async fn test_search_trait_basic() {
    use crate::query::Query;

    // 创建包含两个文件的 tar.gz
    let tar_gz = create_test_tar_gz(vec![
      ("file1.log", "line1\nerror found here\nline3\n"),
      ("file2.log", "normal line\nanother error\nlast line\n"),
    ]);

    // 解析查询
    let spec = Query::parse_github_like("error").unwrap();

    let results = run_tar_search_bytes(tar_gz, &spec, 1).await;

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

    // 创建不包含目标字符串的文件
    let tar_gz = create_test_tar_gz(vec![
      ("file1.log", "line1\nline2\nline3\n"),
      ("file2.log", "normal line\nanother line\nlast line\n"),
    ]);

    let spec = Query::parse_github_like("notfound").unwrap();

    let results = run_tar_search_bytes(tar_gz, &spec, 1).await;

    // 验证：没有匹配结果
    assert_eq!(results.len(), 0);
  }

  #[tokio::test]
  async fn test_search_trait_with_context() {
    use crate::query::Query;

    let tar_gz = create_test_tar_gz(vec![("file1.log", "line1\nline2\nerror here\nline4\nline5\n")]);

    let spec = Query::parse_github_like("error").unwrap();

    // context = 2，应该包含前后各2行
    let results = run_tar_search_bytes(tar_gz, &spec, 2).await;

    let result = results.into_iter().next().unwrap();

    // 验证上下文：应该包含 5 行 (error 前2行 + error 行 + error 后2行)
    assert_eq!(result.lines.len(), 5);
    assert!(result.lines[2].contains("error"));
  }

  #[tokio::test]
  async fn test_search_trait_multiple_matches_in_one_file() {
    use crate::query::Query;

    let tar_gz = create_test_tar_gz(vec![("file1.log", "error1\nline2\nline3\nerror2\nline5\n")]);

    let spec = Query::parse_github_like("error").unwrap();

    let results = run_tar_search_bytes(tar_gz, &spec, 0).await;

    let result = results.into_iter().next().unwrap();

    // 验证：找到两行匹配
    assert_eq!(result.merged.len(), 2);
  }

  #[tokio::test]
  async fn test_search_trait_regex_pattern() {
    use crate::query::Query;

    let tar_gz = create_test_tar_gz(vec![("file1.log", "error123\nline2\nwarn456\n")]);

    // 使用正则匹配 error 或 warn
    let spec = Query::parse_github_like("/error|warn/").unwrap();

    let results = run_tar_search_bytes(tar_gz, &spec, 0).await;

    let result = results.into_iter().next().unwrap();

    // 验证：找到两行匹配
    assert_eq!(result.lines.len(), 3); // 包含上下文
  }

  #[tokio::test]
  async fn test_search_trait_empty_tar() {
    use crate::query::Query;

    // 创建空的 tar.gz
    let tar_gz = create_test_tar_gz(vec![]);

    let spec = Query::parse_github_like("error").unwrap();

    let results = run_tar_search_bytes(tar_gz, &spec, 0).await;

    // 验证：没有结果
    assert_eq!(results.len(), 0);
  }

  #[tokio::test]
  async fn test_search_trait_binary_file_skipped() {
    use crate::query::Query;

    // 创建包含二进制内容的文件
    let binary_content = "\x00\x01\x02\x03error\x04\x05\x06";
    let tar_gz = create_test_tar_gz(vec![
      ("binary.dat", binary_content),
      ("text.log", "this is text error\n"),
    ]);

    let spec = Query::parse_github_like("error").unwrap();

    let results = run_tar_search_bytes(tar_gz, &spec, 0).await;

    // 验证：二进制文件被跳过，只有文本文件
    assert_eq!(results.len(), 1);
    assert!(results[0].path.contains("text.log"));
  }

  #[tokio::test]
  async fn test_search_trait_many_files() {
    use crate::query::Query;

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

    let spec = Query::parse_github_like("error").unwrap();

    let results = run_tar_search_bytes(tar_gz, &spec, 0).await;

    // 验证：找到所有 10 个文件
    assert_eq!(results.len(), 10);
  }

  #[tokio::test]
  async fn test_search_trait_complex_query() {
    use crate::query::Query;

    let tar_gz = create_test_tar_gz(vec![
      ("file1.log", "error and warning\nline2\n"),
      ("file2.log", "only error here\nline2\n"),
      ("file3.log", "only warning here\nline2\n"),
    ]);

    // 同时包含 error 和 warning
    let spec = Query::parse_github_like("error warning").unwrap();

    let results = run_tar_search_bytes(tar_gz, &spec, 0).await;

    // 验证：只有 file1.log 同时包含两个词
    assert_eq!(results.len(), 1);
    assert!(results[0].path.contains("file1.log"));
  }

  #[tokio::test]
  async fn test_search_trait_path_with_directory() {
    use crate::query::Query;

    let tar_gz = create_test_tar_gz(vec![
      ("logs/app/file1.log", "error in app\n"),
      ("logs/system/file2.log", "error in system\n"),
    ]);

    let spec = Query::parse_github_like("error").unwrap();

    let results = run_tar_search_bytes(tar_gz, &spec, 0).await;

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
    let result = SearchResult::new("test.log".to_string(), vec!["error".to_string()], vec![(0, 0)], None);

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

    let result = SearchResult::new("test.log".to_string(), vec!["error".to_string()], vec![(0, 0)], None);

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

  #[tokio::test]
  async fn test_search_processor_with_encoding() {
    let spec = Arc::new(Query::parse_github_like("test").unwrap());
    let processor = SearchProcessor::new_with_encoding(spec, 0, Some("UTF-8".to_string()));

    let content = "test line\n";
    let mut reader = content.as_bytes();

    let result = processor
      .process_content("test.log".to_string(), &mut reader)
      .await
      .unwrap();

    assert!(result.is_some());
    let result = result.unwrap();
    assert_eq!(result.encoding, Some("UTF-8".to_string()));
  }

  #[tokio::test]
  async fn test_search_processor_should_process_path_with() {
    let spec = Arc::new(Query::parse_github_like("test").unwrap());
    let processor = SearchProcessor::new(spec, 0);

    // 没有额外过滤器
    assert!(processor.should_process_path_with("test.log", None));

    // 有额外过滤器
    let extra_filter = crate::query::path_glob_to_filter("*.log").unwrap();
    assert!(processor.should_process_path_with("test.log", Some(&extra_filter)));
    assert!(!processor.should_process_path_with("test.txt", Some(&extra_filter)));
  }

  #[test]
  fn test_search_error_from_io_error() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let search_err: SearchError = io_err.into();

    match search_err {
      SearchError::Io { path, error } => {
        assert_eq!(path, "unknown");
        assert!(error.contains("file not found"));
      }
      _ => panic!("Expected Io error"),
    }
  }

  #[test]
  fn test_search_error_display() {
    let err = SearchError::Io {
      path: "/test/path".to_string(),
      error: "permission denied".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("/test/path"));
    assert!(msg.contains("permission denied"));

    let err = SearchError::ChannelClosed;
    assert_eq!(err.to_string(), "Channel 已关闭: 接收端已断开连接");
  }

  #[tokio::test]
  async fn test_search_processor_process_content_error() {
    let spec = Arc::new(Query::parse_github_like("test").unwrap());
    let processor = SearchProcessor::new(spec, 0);

    // 创建一个会失败的 reader
    struct FailingReader;
    impl tokio::io::AsyncRead for FailingReader {
      fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
      ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Err(std::io::Error::other("read error")))
      }
    }

    let mut reader = FailingReader;
    let result = processor.process_content("test.log".to_string(), &mut reader).await;

    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_search_processor_send_result_error() {
    let spec = Arc::new(Query::parse_github_like("test").unwrap());
    let processor = SearchProcessor::new(spec, 0);

    let (tx, rx) = tokio::sync::mpsc::channel(1);
    drop(rx); // 关闭接收端

    let result = SearchResult {
      path: "test.log".to_string(),
      lines: vec!["test line".to_string()],
      merged: vec![(1, 1)],
      encoding: Some("UTF-8".to_string()),
    };

    let send_result = processor.send_result(result, &tx).await;
    assert!(send_result.is_err());
    assert!(matches!(send_result.unwrap_err(), SearchError::ChannelClosed));
  }

  #[tokio::test]
  async fn test_grep_context_with_large_context() {
    let content = "line1\nline2\nerror\nline4\nline5\n";
    let mut reader = content.as_bytes();
    let spec = Query::parse_github_like("error").unwrap();

    // 使用非常大的上下文
    let result = grep_context(&mut reader, &spec, 100, None).await.unwrap();

    assert!(result.is_some());
    let (lines, merged, _) = result.unwrap();
    // 应该包含所有行
    assert!(lines.len() >= 3);
    assert!(!merged.is_empty());
  }

  #[tokio::test]
  async fn test_grep_context_with_encoding_override() {
    let content = "测试内容\n";
    let mut reader = content.as_bytes();
    let spec = Query::parse_github_like("测试").unwrap();

    // 指定编码
    let result = grep_context(&mut reader, &spec, 0, Some("UTF-8")).await.unwrap();

    assert!(result.is_some());
    let (_, _, encoding) = result.unwrap();
    assert_eq!(encoding, Some("UTF-8".to_string()));
  }

  #[tokio::test]
  async fn test_search_result_clone() {
    let result = SearchResult {
      path: "test.log".to_string(),
      lines: vec!["line1".to_string()],
      merged: vec![(1, 1)],
      encoding: Some("UTF-8".to_string()),
    };

    let cloned = result.clone();
    assert_eq!(result.path, cloned.path);
    assert_eq!(result.lines, cloned.lines);
    assert_eq!(result.merged, cloned.merged);
    assert_eq!(result.encoding, cloned.encoding);
  }

  #[tokio::test]
  async fn test_search_event_variants() {
    let success = SearchEvent::Success(SearchResult {
      path: "test.log".to_string(),
      lines: vec!["line".to_string()],
      merged: vec![(1, 1)],
      encoding: Some("UTF-8".to_string()),
    });

    match success {
      SearchEvent::Success(_) => {}
      _ => panic!("Expected Success variant"),
    }

    let error = SearchEvent::Error {
      source: "test".to_string(),
      message: "error".to_string(),
      recoverable: true,
    };

    match error {
      SearchEvent::Error { recoverable, .. } => {
        assert!(recoverable);
      }
      _ => panic!("Expected Error variant"),
    }

    let complete = SearchEvent::Complete {
      source: "test".to_string(),
      elapsed_ms: 100,
    };

    match complete {
      SearchEvent::Complete { elapsed_ms, .. } => {
        assert_eq!(elapsed_ms, 100);
      }
      _ => panic!("Expected Complete variant"),
    }
  }

  #[tokio::test]
  async fn test_grep_context_empty_file() {
    let content = "";
    let mut reader = content.as_bytes();
    let spec = Query::parse_github_like("test").unwrap();

    let result = grep_context(&mut reader, &spec, 0, None).await.unwrap();
    assert!(result.is_none());
  }

  #[tokio::test]
  async fn test_grep_context_no_newline_at_end() {
    let content = "line1\nline2\nerror";
    let mut reader = content.as_bytes();
    let spec = Query::parse_github_like("error").unwrap();

    let result = grep_context(&mut reader, &spec, 0, None).await.unwrap();
    assert!(result.is_some());
    let (lines, _, _) = result.unwrap();
    assert!(lines.iter().any(|line| line.contains("error")));
  }

  #[tokio::test]
  async fn test_search_processor_new_methods() {
    let spec = Arc::new(Query::parse_github_like("test").unwrap());

    // new
    let processor1 = SearchProcessor::new(spec.clone(), 5);
    assert_eq!(processor1.context_lines, 5);
    assert!(processor1.encoding.is_none());

    // new_with_encoding
    let processor2 = SearchProcessor::new_with_encoding(spec.clone(), 3, Some("GBK".to_string()));
    assert_eq!(processor2.context_lines, 3);
    assert_eq!(processor2.encoding, Some("GBK".to_string()));

    let processor3 = SearchProcessor::new_with_encoding(spec, 1, None);
    assert_eq!(processor3.context_lines, 1);
    assert!(processor3.encoding.is_none());
  }
}
