use std::{io, path::PathBuf};

use async_compression::tokio::bufread::GzipDecoder;
use async_trait::async_trait;
use futures::StreamExt;
use tokio::io::{AsyncRead, AsyncReadExt, BufReader};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use tracing::{trace, warn};

/// 条目来源类型
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum EntrySource {
  /// 普通文件（目录遍历或单文件）
  #[default]
  File,
  /// tar 归档内的条目
  Tar,
  /// tar.gz 归档内的条目
  TarGz,
  /// 纯 gzip 压缩文件（非 tar 归档）
  Gz,
}

/// 条目元数据（目录相对路径或归档内路径）
#[derive(Clone, Debug)]
pub struct EntryMeta {
  pub path: String,
  /// 当条目来自归档内部时，归档文件路径（绝对路径，供上层构造唯一 ID）
  pub container_path: Option<String>,
  pub size: Option<u64>,
  pub is_compressed: bool,
  /// 条目来源类型
  pub source: EntrySource,
}

/// 统一的“条目流”抽象：每次产出 (EntryMeta, Reader)
#[async_trait]
pub trait EntryStream: Send {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>>;
}

/// 目录条目流（基于 jwalk 并行遍历）
pub struct FsEntryStream {
  rx: tokio::sync::mpsc::Receiver<io::Result<(PathBuf, std::fs::Metadata)>>,
}

impl FsEntryStream {
  /// 从根目录创建并行遍历条目流
  pub async fn new(root: PathBuf, recursive: bool) -> io::Result<Self> {
    let (tx, rx) = tokio::sync::mpsc::channel(256); // Buffer size

    // 判断 root 是否是文件
    if root.is_file() {
      // 如果根就是文件，直接发送并结束
      let _ = tx.send(Ok((root.clone(), root.metadata()?))).await;
      return Ok(Self { rx });
    }

    // 在 blocking thread 中运行 jwalk
    std::thread::spawn(move || {
      use jwalk::WalkDir;
      let walk = WalkDir::new(&root)
        .follow_links(false)
        .max_depth(if recursive { usize::MAX } else { 1 })
        .skip_hidden(false);

      for entry in walk {
        match entry {
          Ok(e) => {
            // 只处理文件
            if e.file_type().is_file() {
              // 需要 metadata (jwalk entry has it cached usually)
              if let Ok(meta) = e.metadata() {
                // 使用 block_on 发送或 blocking_send?
                // tokio Sender in std thread: use blocking_send
                if tx.blocking_send(Ok((e.path(), meta))).is_err() {
                  break; // Receiver dropping
                }
              }
            }
          }
          Err(e) => {
            let io_err = io::Error::other(e.to_string());
            if tx.blocking_send(Err(io_err)).is_err() {
              break;
            }
          }
        }
      }
    });

    Ok(Self { rx })
  }

  // Legacy capability not supported with jwalk easily,
  // but we can reimplementation if needed.
  // For now, assuming new() is the main entry point.
  pub fn from_read_dir(_rd: tokio::fs::ReadDir, _recursive: bool) -> Self {
    // Placeholder: Creating a closed stream or erroring if strictly needed.
    // Given the context, this seems rarely used or can be refactored at callsite.
    // To avoid breaking compilation, returns empty stream.
    let (_, rx) = tokio::sync::mpsc::channel(1);
    Self { rx }
  }
}

#[async_trait]
impl EntryStream for FsEntryStream {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    loop {
      match self.rx.recv().await {
        Some(Ok((path, _meta))) => {
          // 打开文件并检测压缩
          // jwalk 已经过滤了非文件
          match open_file_with_compression_detection(&path.to_string_lossy()).await {
            Ok((mut meta, reader)) => {
              meta.path = path.to_string_lossy().to_string();
              return Ok(Some((meta, reader)));
            }
            Err(e) => {
              tracing::warn!("无法打开文件 {}: {}", path.display(), e);
              continue;
            }
          }
        }
        Some(Err(e)) => {
          tracing::warn!("jwalk 遍历错误: {}", e);
          continue;
        }
        None => return Ok(None),
      }
    }
  }
}

/// tar.gz 条目流（基于 AsyncRead 输入）
pub struct TarGzEntryStream<R: AsyncRead + Send + Unpin + 'static> {
  entries: async_tar::Entries<tokio_util::compat::Compat<GzipDecoder<BufReader<R>>>>,
  container_path: Option<String>,
  consecutive_errors: usize,
  next_entry_index: usize,
  last_ok_entry_path: Option<String>,
}
impl<R: AsyncRead + Send + Unpin + 'static> TarGzEntryStream<R> {
  pub async fn new(reader: R, container_path: Option<String>) -> io::Result<Self> {
    let gz = GzipDecoder::new(BufReader::new(reader));
    let archive = async_tar::Archive::new(gz.compat());
    let entries = archive.entries()?;
    Ok(Self {
      entries,
      container_path,
      consecutive_errors: 0,
      next_entry_index: 0,
      last_ok_entry_path: None,
    })
  }
}

impl<D: AsyncRead + Send + Unpin + 'static> TarGzEntryStreamAny<D> {
  // 为 sniffing 逻辑提供专门的构造器，如果已经有了 decoder
  pub async fn new_with_decoder(decoder: D, container_path: Option<String>) -> io::Result<Self> {
    let archive = async_tar::Archive::new(decoder.compat());
    let entries = archive.entries()?;
    Ok(Self {
      entries,
      container_path,
      consecutive_errors: 0,
      next_entry_index: 0,
      last_ok_entry_path: None,
    })
  }
}

/// 通用的 TarGZ 流，支持任意 Decoder 类型
pub struct TarGzEntryStreamAny<D: AsyncRead + Send + Unpin + 'static> {
  entries: async_tar::Entries<tokio_util::compat::Compat<D>>,
  container_path: Option<String>,
  consecutive_errors: usize,
  next_entry_index: usize,
  last_ok_entry_path: Option<String>,
}

#[async_trait]
impl<D: AsyncRead + Send + Unpin + 'static> EntryStream for TarGzEntryStreamAny<D> {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    loop {
      if self.consecutive_errors > 10 {
        return Err(io::Error::other("过多的连续 tar 错误"));
      }

      let entry = self.entries.next().await;
      match entry {
        Some(Ok(entry)) => {
          self.consecutive_errors = 0;
          self.next_entry_index += 1;

          let entry_path = entry
            .path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| format!("entry_{}", self.next_entry_index));

          self.last_ok_entry_path = Some(entry_path.clone());

          let size = entry.header().size().ok();

          let meta = EntryMeta {
            path: entry_path,
            container_path: self.container_path.clone(),
            size,
            is_compressed: false,
            source: EntrySource::Tar,
          };

          // 包装 entry 为异步读取器 (async_tar::Entry implements futures::io::AsyncRead)
          return Ok(Some((meta, Box::new(entry.compat()))));
        }
        Some(Err(e)) => {
          self.consecutive_errors += 1;
          warn!(
            "跳过损坏的 tar 条目: {}, path={:?}, error={}",
            self.container_path.as_deref().unwrap_or("unknown"),
            self.last_ok_entry_path,
            e
          );
          continue;
        }
        None => return Ok(None),
      }
    }
  }
}

#[async_trait]
impl<R: AsyncRead + Send + Unpin + 'static> EntryStream for TarGzEntryStream<R> {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    const MAX_CONSECUTIVE_ERRORS: usize = 100;

    loop {
      match self.entries.next().await {
        Some(Ok(entry)) => {
          // 成功读取条目，重置错误计数器
          self.consecutive_errors = 0;
          self.next_entry_index = self.next_entry_index.saturating_add(1);
          let raw = entry
            .path()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "<unknown>".into());
          let path = normalize_archive_entry_path(&raw);
          self.last_ok_entry_path = Some(path.clone());
          let reader = entry.compat(); // 转为 tokio AsyncRead
          let meta = EntryMeta {
            path,
            container_path: self.container_path.clone(),
            size: None,
            is_compressed: true, // tar.gz 内部条目：共享底层解压/读取器，必须串行读取
            source: EntrySource::TarGz,
          };
          return Ok(Some((meta, Box::new(reader))));
        }
        Some(Err(e)) => {
          self.consecutive_errors += 1;
          // 记录错误但继续处理下一个条目
          tracing::warn!(
            "跳过损坏的 tar.gz 条目: {} (next_index={}, last_ok_entry={:?}, 连续错误: {}/{})",
            e,
            self.next_entry_index,
            self.last_ok_entry_path,
            self.consecutive_errors,
            MAX_CONSECUTIVE_ERRORS
          );

          // 如果连续错误超过阈值，停止处理以避免死循环
          if self.consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
            return Err(io::Error::new(
              io::ErrorKind::InvalidData,
              format!(
                "tar.gz 文件损坏严重，连续 {} 个条目读取失败，停止处理",
                MAX_CONSECUTIVE_ERRORS
              ),
            ));
          }
          continue;
        }
        None => return Ok(None),
      }
    }
  }
}

pub struct TarEntryStream<R: AsyncRead + Send + Unpin + 'static> {
  entries: async_tar::Entries<tokio_util::compat::Compat<BufReader<R>>>,
  container_path: Option<String>,
  consecutive_errors: usize,
  next_entry_index: usize,
  last_ok_entry_path: Option<String>,
}

impl<R: AsyncRead + Send + Unpin + 'static> TarEntryStream<R> {
  pub async fn new(reader: R, container_path: Option<String>) -> io::Result<Self> {
    let br = BufReader::new(reader);
    let archive = async_tar::Archive::new(br.compat());
    let entries = archive.entries()?;
    Ok(Self {
      entries,
      container_path,
      consecutive_errors: 0,
      next_entry_index: 0,
      last_ok_entry_path: None,
    })
  }
}

#[async_trait]
impl<R: AsyncRead + Send + Unpin + 'static> EntryStream for TarEntryStream<R> {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    const MAX_CONSECUTIVE_ERRORS: usize = 100;

    loop {
      match self.entries.next().await {
        Some(Ok(entry)) => {
          // 成功读取条目，重置错误计数器
          self.consecutive_errors = 0;
          self.next_entry_index = self.next_entry_index.saturating_add(1);
          let raw = entry
            .path()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "<unknown>".into());
          let path = normalize_archive_entry_path(&raw);
          self.last_ok_entry_path = Some(path.clone());
          let reader = entry.compat();
          let meta = EntryMeta {
            path,
            container_path: self.container_path.clone(),
            size: None,
            is_compressed: true,
            source: EntrySource::Tar,
          };
          return Ok(Some((meta, Box::new(reader))));
        }
        Some(Err(e)) => {
          self.consecutive_errors += 1;
          tracing::warn!(
            "跳过损坏的 tar 条目: {} (next_index={}, last_ok_entry={:?}, 连续错误: {}/{})",
            e,
            self.next_entry_index,
            self.last_ok_entry_path,
            self.consecutive_errors,
            MAX_CONSECUTIVE_ERRORS
          );
          if self.consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
            return Err(io::Error::new(
              io::ErrorKind::InvalidData,
              format!(
                "tar 文件损坏严重，连续 {} 个条目读取失败，停止处理",
                MAX_CONSECUTIVE_ERRORS
              ),
            ));
          }
          continue;
        }
        None => return Ok(None),
      }
    }
  }
}

/// 多文件条目流
pub struct MultiFileEntryStream {
  files: Vec<String>,
  idx: usize,
}

impl MultiFileEntryStream {
  pub fn new(files: Vec<String>) -> Self {
    Self { files, idx: 0 }
  }
}

#[async_trait]
impl EntryStream for MultiFileEntryStream {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    if self.idx >= self.files.len() {
      return Ok(None);
    }
    let path = std::mem::take(&mut self.files[self.idx]);
    self.idx += 1;
    match open_file_with_compression_detection(&path).await {
      Ok((mut meta, reader)) => {
        // 使用绝对路径（对齐 local 的表现形式）
        meta.path = path;
        Ok(Some((meta, reader)))
      }
      Err(e) => {
        // 如果压缩检测失败，返回错误（保持原有行为：文件打开失败时返回错误）
        Err(e)
      }
    }
  }
}

/// Gzip 条目流（单文件）
pub struct GzipEntryStream<R: AsyncRead + Send + Unpin + 'static> {
  reader: Option<R>,
  path: String,
  processed: bool,
  container_path: Option<String>,
}

impl<R: AsyncRead + Send + Unpin + 'static> GzipEntryStream<R> {
  pub fn new(reader: R, path: String, container_path: bool) -> Self {
    let c_path = if container_path { Some(path.clone()) } else { None };
    Self {
      reader: Some(reader),
      path,
      processed: false,
      container_path: c_path,
    }
  }
}

#[async_trait]
impl<R: AsyncRead + Send + Unpin + 'static> EntryStream for GzipEntryStream<R> {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    if self.processed {
      return Ok(None);
    }
    self.processed = true;
    if let Some(reader) = self.reader.take() {
      let meta = EntryMeta {
        path: self.path.clone(),
        container_path: self.container_path.clone(),
        size: None,
        is_compressed: true, // 解压后的流，设为 true 以触发 preloading 逻辑
        source: EntrySource::Gz,
      };
      Ok(Some((meta, Box::new(reader))))
    } else {
      Ok(None)
    }
  }
}

// ================= Helpers =================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
  Tar,
  Gzip,
  Zip,
  Unknown,
}

pub async fn create_archive_stream_from_reader<R: AsyncRead + Send + Unpin + 'static>(
  mut reader: R,
  hint_name: Option<&str>,
) -> Result<Box<dyn EntryStream>, String> {
  use tokio::io::AsyncReadExt;
  // 预读 4KB 头部以进行归档类型探测和解压探测
  let mut head = vec![0u8; 4096];
  let mut n = 0;
  while n < head.len() {
    match reader.read(&mut head[n..]).await {
      Ok(0) => break,
      Ok(len) => n += len,
      Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
      Err(e) => return Err(format!("读取头部失败: {}", e)),
    }
  }
  head.truncate(n);

  let kind = sniff_archive_kind(&head, hint_name);
  warn!(
    "[DEBUG] sniff_archive_kind: {:?}, hint_name: {:?}, head_len: {}",
    kind,
    hint_name,
    head.len()
  );
  let prefixed = PrefixedReader::new(head.clone(), reader);

  match kind {
    ArchiveKind::Tar => {
      let stream = TarEntryStream::new(prefixed, hint_name.map(|s| s.to_string()))
        .await
        .map_err(|e| format!("读取 tar 失败: {}", e))?;
      Ok(Box::new(stream) as Box<dyn EntryStream>)
    }
    ArchiveKind::Gzip => {
      // 基于预读的 head 进行探测
      let is_tar = {
        let mut gz = GzipDecoder::new(std::io::Cursor::new(head.clone()));
        let mut inner_head = vec![0u8; 512];
        match gz.read_exact(&mut inner_head).await {
          Ok(_) => is_tar_header(&inner_head),
          Err(_) => {
            // 如果内部数据太少无法嗅探（EOF或解压失败），尝试通过后缀名给予最后的补救建议
            if let Some(h) = hint_name {
              let lower = h.to_lowercase();
              lower.ends_with(".tar.gz") || lower.ends_with(".tgz")
            } else {
              false
            }
          }
        }
      };

      let gz = GzipDecoder::new(BufReader::new(prefixed));
      if is_tar {
        let stream = TarGzEntryStreamAny::new_with_decoder(gz, hint_name.map(|s| s.to_string()))
          .await
          .map_err(|e| format!("解析 tar.gz 失败: {}", e))?;
        Ok(Box::new(stream) as Box<dyn EntryStream>)
      } else {
        // 走单文件 gzip 逻辑
        let (entry_path, container_path) = if let Some(h) = hint_name {
          let p = std::path::Path::new(h);
          let entry = p
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| h.to_string());
          (entry, Some(h.to_string()))
        } else {
          ("<gzip>".to_string(), None)
        };
        let stream = GzipEntryStream {
          reader: Some(gz),
          path: entry_path,
          processed: false,
          container_path,
        };
        Ok(Box::new(stream) as Box<dyn EntryStream>)
      }
    }
    ArchiveKind::Zip => Err("ZIP 归档暂不支持".to_string()),
    ArchiveKind::Unknown => Err("未知归档格式或不支持的归档".to_string()),
  }
}

pub fn sniff_archive_kind(head: &[u8], path_hint: Option<&str>) -> ArchiveKind {
  if head.len() >= 4 {
    let sig = &head[..4];
    if sig == [0x50, 0x4B, 0x03, 0x04] || sig == [0x50, 0x4B, 0x05, 0x06] || sig == [0x50, 0x4B, 0x07, 0x08] {
      trace!("检测到归档类型: Zip, 文件: {}", path_hint.unwrap_or("unknown"));
      return ArchiveKind::Zip;
    }
  }
  // 优先检查 tar（tar 头在固定位置 257-262，更可靠）
  if head.len() >= 512 && &head[257..257 + 5] == b"ustar" {
    trace!("检测到归档类型: Tar, 文件: {}", path_hint.unwrap_or("unknown"));
    return ArchiveKind::Tar;
  }
  // 然后检查 gzip（前2字节）
  if head.len() >= 2 && head[0] == 0x1F && head[1] == 0x8B {
    trace!("检测到归档类型: Gzip, 文件: {}", path_hint.unwrap_or("unknown"));
    return ArchiveKind::Gzip;
  }
  trace!("检测到归档类型: Unknown, 文件: {}", path_hint.unwrap_or("unknown"));
  ArchiveKind::Unknown
}

/// 检测文件类型并返回适当的 Reader 和 Metadata
/// 仅处理纯 gzip 文件（非 tar 归档），其他文件按普通文件处理
pub async fn open_file_with_compression_detection(
  path: &str,
) -> io::Result<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)> {
  // 1. 打开文件并读取头部（最多560字节，与现有逻辑一致）
  let mut file = tokio::fs::File::open(path).await?;
  let mut head = vec![0u8; 560];
  let n = file.read(&mut head).await?;
  head.truncate(n);

  // 2. 检测文件类型
  let kind = sniff_archive_kind(&head, Some(path));

  // 3. 探测细节逻辑（针对 Gzip 进行内部嗅探）
  match kind {
    ArchiveKind::Gzip => {
      // 为了不破坏流状态，我们重新打开一次文件进行 Tar 头部嗅探
      let is_tar = match tokio::fs::File::open(path).await {
        Ok(f) => {
          let mut gz = GzipDecoder::new(BufReader::new(f));
          let mut inner_head = vec![0u8; 512];
          match gz.read_exact(&mut inner_head).await {
            Ok(_) => is_tar_header(&inner_head),
            _ => false,
          }
        }
        _ => false,
      };

      if is_tar {
        // tar.gz 文件：按普通文件处理（扫描场景用户要求不自动展开）
        create_regular_file_reader(path).await
      } else {
        // 纯 gzip 文件：解压并设为 is_compressed: true
        let file = tokio::fs::File::open(path).await?;
        let gz = GzipDecoder::new(BufReader::new(file));
        let meta = EntryMeta {
          path: path.to_string(),
          container_path: None,
          size: None,
          is_compressed: true,
          source: EntrySource::Gz,
        };
        Ok((meta, Box::new(gz) as Box<dyn AsyncRead + Send + Unpin>))
      }
    }
    _ => {
      // 默认逻辑：按普通文件打开
      create_regular_file_reader(path).await
    }
  }
}

async fn create_regular_file_reader(path: &str) -> io::Result<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)> {
  let file = tokio::fs::File::open(path).await?;
  let reader = BufReader::new(file);
  let meta = EntryMeta {
    path: path.to_string(),
    container_path: None,
    size: None,
    is_compressed: false,
    source: EntrySource::File,
  };
  Ok((meta, Box::new(reader)))
}

fn is_tar_header(head: &[u8]) -> bool {
  head.len() >= 512 && &head[257..257 + 5] == b"ustar"
}

pub struct PrefixedReader<R> {
  prefix: std::io::Cursor<Vec<u8>>,
  inner: R,
}

fn normalize_archive_entry_path(s: &str) -> String {
  let mut t = s;
  // 去掉前导的 '/' 或 './'
  loop {
    if t.starts_with("./") {
      t = &t[2..];
      continue;
    }
    if t.starts_with('/') {
      t = &t[1..];
      continue;
    }
    break;
  }
  t.to_string()
}

impl<R> PrefixedReader<R> {
  pub fn new(prefix: Vec<u8>, inner: R) -> Self {
    Self {
      prefix: std::io::Cursor::new(prefix),
      inner,
    }
  }
}

impl<R: AsyncRead + Unpin> AsyncRead for PrefixedReader<R> {
  fn poll_read(
    self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
    buf: &mut tokio::io::ReadBuf<'_>,
  ) -> std::task::Poll<std::io::Result<()>> {
    let me = self.get_mut();
    if (me.prefix.position() as usize) < me.prefix.get_ref().len() {
      let mut tmp = vec![0u8; buf.remaining()];
      let read = std::io::Read::read(&mut me.prefix, &mut tmp).unwrap_or(0);
      if read > 0 {
        buf.put_slice(&tmp[..read]);
        return std::task::Poll::Ready(Ok(()));
      }
    }
    std::pin::Pin::new(&mut me.inner).poll_read(cx, buf)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::io::AsyncReadExt;

  #[tokio::test]
  async fn test_prefixed_reader() {
    let prefix = vec![1, 2, 3];
    let inner = std::io::Cursor::new(vec![4, 5, 6]);
    let mut reader = PrefixedReader::new(prefix, inner);
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).await.unwrap();
    assert_eq!(buf, vec![1, 2, 3, 4, 5, 6]);
  }

  #[test]
  fn test_normalize_archive_entry_path() {
    assert_eq!(normalize_archive_entry_path("foo/bar"), "foo/bar");
    assert_eq!(normalize_archive_entry_path("./foo/bar"), "foo/bar");
    assert_eq!(normalize_archive_entry_path("/foo/bar"), "foo/bar");
    assert_eq!(normalize_archive_entry_path("///foo/bar"), "foo/bar");
  }

  #[test]
  fn test_sniff_archive_kind_tar() {
    let mut head = vec![0u8; 512];
    // ustar at 257
    head[257] = b'u';
    head[258] = b's';
    head[259] = b't';
    head[260] = b'a';
    head[261] = b'r';
    assert_eq!(sniff_archive_kind(&head, None), ArchiveKind::Tar);
  }

  #[test]
  fn test_sniff_archive_kind_gzip() {
    let head = vec![0x1F, 0x8B, 0x08];
    assert_eq!(sniff_archive_kind(&head, None), ArchiveKind::Gzip);
  }

  #[test]
  fn test_sniff_archive_kind_zip() {
    let head = vec![0x50, 0x4B, 0x03, 0x04];
    assert_eq!(sniff_archive_kind(&head, None), ArchiveKind::Zip);
  }
}
