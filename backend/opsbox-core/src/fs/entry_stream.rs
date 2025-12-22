use std::{io, path::PathBuf};

use async_compression::tokio::bufread::GzipDecoder;
use async_trait::async_trait;
use futures::StreamExt;
use tokio::io::{AsyncRead, AsyncReadExt, BufReader};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use tracing::trace;

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

/// 目录条目流（DFS 遍历）
pub struct FsEntryStream {
  stack: Vec<tokio::fs::ReadDir>,
  recursive: bool,
}

impl FsEntryStream {
  /// 从根目录创建条目流
  pub async fn new(root: PathBuf, recursive: bool) -> io::Result<Self> {
    let rd = tokio::fs::read_dir(&root).await?;
    Ok(Self {
      stack: vec![rd],
      recursive,
    })
  }

  /// 直接从已存在的 ReadDir 创建（无根路径信息）
  pub fn from_read_dir(rd: tokio::fs::ReadDir, recursive: bool) -> Self {
    Self {
      stack: vec![rd],
      recursive,
    }
  }
}

#[async_trait]
impl EntryStream for FsEntryStream {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    loop {
      let Some(current) = self.stack.last_mut() else {
        return Ok(None);
      };
      match current.next_entry().await {
        Ok(Some(entry)) => {
          let ft = match entry.file_type().await {
            Ok(t) => t,
            Err(_) => continue,
          };
          if ft.is_symlink() {
            continue;
          }
          if ft.is_dir() {
            if self.recursive
              && let Ok(sub) = tokio::fs::read_dir(entry.path()).await
            {
              self.stack.push(sub);
            }
            continue;
          }
          if !ft.is_file() {
            continue;
          }

          let path_abs = entry.path();
          match open_file_with_compression_detection(&path_abs.to_string_lossy()).await {
            Ok((mut meta, reader)) => {
              // 使用绝对路径（对齐 local 的表现形式）
              meta.path = path_abs.to_string_lossy().to_string();
              return Ok(Some((meta, reader)));
            }
            Err(e) => {
              tracing::warn!("无法处理文件 {}: {}", path_abs.display(), e);
              continue;
            }
          }
        }
        Ok(None) => {
          self.stack.pop(); /* 回溯 */
        }
        Err(_) => {
          self.stack.pop(); /* 跳过该目录 */
        }
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
    // gzip 解压 + 适配为 futures::io::AsyncRead
    let gz = GzipDecoder::new(BufReader::new(reader));
    let archive = async_tar::Archive::new(gz.compat());
    let entries = archive.entries()?; // 注意：entries 拥有 archive
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
        is_compressed: false, // 解压后的流
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
  let mut head = vec![0u8; 560];
  let n = reader
    .read(&mut head)
    .await
    .map_err(|e| format!("读取头部失败: {}", e))?;
  head.truncate(n);
  let kind = sniff_archive_kind(&head, hint_name);
  let prefixed = PrefixedReader::new(head, reader);
  match kind {
    ArchiveKind::Tar => {
      let stream = TarEntryStream::new(prefixed, hint_name.map(|s| s.to_string()))
        .await
        .map_err(|e| format!("读取 tar 失败: {}", e))?;
      Ok(Box::new(stream))
    }
    ArchiveKind::Gzip => {
      // 二次嗅探：先解压一段头部，判断是否为 tar 归档，再决定走 tar.gz 还是单文件 .gz
      let mut gz = GzipDecoder::new(BufReader::new(prefixed));
      let mut inner_head = vec![0u8; 560];
      let n = tokio::io::AsyncReadExt::read(&mut gz, &mut inner_head)
        .await
        .map_err(|e| format!("读取 gzip 内容头部失败: {}", e))?;
      inner_head.truncate(n);
      let is_tar = is_tar_header(&inner_head);
      let gz_prefixed = PrefixedReader::new(inner_head, gz);
      if is_tar {
        let stream = TarEntryStream::new(gz_prefixed, hint_name.map(|s| s.to_string()))
          .await
          .map_err(|e| format!("读取 tar(解压后) 失败: {}", e))?;
        Ok(Box::new(stream))
      } else {
        let name = hint_name.unwrap_or("<gzip>").to_string();
        // 修正：GzipEntryStream 构造函数参数匹配
        let stream = GzipEntryStream {
          reader: Some(gz_prefixed),
          path: name,
          processed: false,
          container_path: None,
        };
        Ok(Box::new(stream))
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

  // 3. 重新创建 Reader（包含已读取的头部）
  let prefixed = PrefixedReader::new(head, BufReader::new(file));

  // 4. 根据类型返回适当的 Reader
  match kind {
    ArchiveKind::Gzip => {
      // 二次嗅探：判断是否为 tar 归档
      let mut gz = GzipDecoder::new(BufReader::new(prefixed));
      let mut inner_head = vec![0u8; 560];
      let n = gz.read(&mut inner_head).await?;
      inner_head.truncate(n);

      let is_tar = is_tar_header(&inner_head);
      let gz_prefixed = PrefixedReader::new(inner_head, gz);

      if is_tar {
        // tar.gz 文件：按普通文件处理（用户要求不展开 tar 归档）
        create_regular_file_reader(path).await
      } else {
        // 纯 gzip 文件：返回解压后的流
        let meta = EntryMeta {
          path: path.to_string(),
          container_path: None,
          size: None,
          is_compressed: false, // 解压后的流可并行处理
          source: EntrySource::Gz,
        };
        Ok((meta, Box::new(gz_prefixed)))
      }
    }
    _ => {
      // 普通文件或其他归档格式（tar/zip）
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
