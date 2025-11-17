use std::{io, path::PathBuf, sync::Arc, time::Duration};

use async_compression::tokio::bufread::GzipDecoder;
use async_trait::async_trait;
use futures::{StreamExt, stream::FuturesUnordered};
use tokio::io::{AsyncRead, BufReader};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use tracing::{debug, info, warn};

// 统一读取并发度：使用 ENTRY_CONCURRENCY（范围 1-64，默认 8）
fn entry_concurrency() -> usize {
  std::env::var("ENTRY_CONCURRENCY")
    .ok()
    .and_then(|s| s.parse::<usize>().ok())
    .unwrap_or(8)
    .clamp(1, 64)
}

use super::search::{SearchEvent, SearchProcessor};
use opsbox_core::SqlitePool;

/// 条目元数据（目录相对路径或归档内路径）
#[derive(Clone, Debug)]
pub struct EntryMeta {
  pub path: String,
  pub size: Option<u64>,
  pub is_compressed: bool,
}

/// 统一的“条目流”抽象：每次产出 (EntryMeta, Reader)
#[async_trait]
pub trait EntryStream: Send {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>>;
}

/// 目录条目流（DFS 遍历）
pub struct FsEntryStream {
  stack: Vec<tokio::fs::ReadDir>,
  root: Option<PathBuf>,
  recursive: bool,
}

impl FsEntryStream {
  /// 从根目录创建条目流
  pub async fn new(root: PathBuf, recursive: bool) -> io::Result<Self> {
    let rd = tokio::fs::read_dir(&root).await?;
    Ok(Self {
      stack: vec![rd],
      root: Some(root),
      recursive,
    })
  }

  /// 直接从已存在的 ReadDir 创建（无根路径信息）
  pub fn from_read_dir(rd: tokio::fs::ReadDir, recursive: bool) -> Self {
    Self {
      stack: vec![rd],
      root: None,
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
          let rel = if let Some(root) = &self.root {
            path_abs
              .strip_prefix(root)
              .unwrap_or(&path_abs)
              .to_string_lossy()
              .to_string()
          } else {
            path_abs.to_string_lossy().to_string()
          };
          let file = tokio::fs::File::open(&path_abs).await?;
          let reader = BufReader::new(file);
          let meta = EntryMeta {
            path: rel,
            size: None,
            is_compressed: false,
          };
          return Ok(Some((meta, Box::new(reader))));
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
}

impl<R: AsyncRead + Send + Unpin + 'static> TarGzEntryStream<R> {
  pub async fn new(reader: R) -> io::Result<Self> {
    // gzip 解压 + 适配为 futures::io::AsyncRead
    let gz = GzipDecoder::new(BufReader::new(reader));
    let archive = async_tar::Archive::new(gz.compat());
    let entries = archive.entries()?; // 注意：entries 拥有 archive
    Ok(Self { entries })
  }
}

#[async_trait]
impl<R: AsyncRead + Send + Unpin + 'static> EntryStream for TarGzEntryStream<R> {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    loop {
      match self.entries.next().await {
        Some(Ok(entry)) => {
          let raw = entry
            .path()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "<unknown>".into());
          let path = normalize_archive_entry_path(&raw);
          let reader = entry.compat(); // 转为 tokio AsyncRead
          let meta = EntryMeta {
            path,
            size: None,
            is_compressed: true, // tar.gz 内部条目：共享底层解压/读取器，必须串行读取
          };
          return Ok(Some((meta, Box::new(reader))));
        }
        Some(Err(e)) => {
          // 记录错误但继续处理下一个条目（使用debug级别避免日志泛滥）
          tracing::debug!("跳过损坏的 tar.gz 条目: {}", e);
          continue;
        }
        None => return Ok(None),
      }
    }
  }
}

/// 统一条目流处理器：消费 EntryStream，调用 SearchProcessor 处理内容
pub struct EntryStreamProcessor {
  processor: Arc<SearchProcessor>,
  content_timeout: Duration,
  // 额外路径过滤器（可选），与用户查询中的 path: 规则做 AND
  extra_path_filter: Option<crate::query::PathFilter>,
}

impl EntryStreamProcessor {
  pub fn new(processor: Arc<SearchProcessor>) -> Self {
    Self {
      processor,
      content_timeout: Duration::from_secs(60),
      extra_path_filter: None,
    }
  }

  /// 设置额外路径过滤器（与用户 path: 规则做 AND）
  pub fn with_extra_path_filter(mut self, filter: crate::query::PathFilter) -> Self {
    self.extra_path_filter = Some(filter);
    self
  }

  #[allow(dead_code)]
  pub fn with_content_timeout(mut self, timeout: Duration) -> Self {
    self.content_timeout = timeout;
    self
  }

  /// 並发处理条目（有畊并发，默认并发度 8，可通过 ENTRY_CONCURRENCY 环境变量调整，范围 1-64）
  pub async fn process_stream(
    &mut self,
    entries: &mut dyn EntryStream,
    tx: tokio::sync::mpsc::Sender<SearchEvent>,
  ) -> Result<(), String> {
    let processor = self.processor.clone();
    let content_timeout = self.content_timeout;
    let mut in_flight: FuturesUnordered<_> = FuturesUnordered::new();
    let max_conc = entry_concurrency();

    loop {
      // 如果并发达到上限，先等待一个任务完成
      if in_flight.len() >= max_conc {
        let _ = in_flight.next().await; // 丢弃一个完成结果
        continue;
      }

      // 拉取下一个条目
      let next = entries.next_entry().await.map_err(|e| e.to_string())?;
      let Some((meta, mut reader)) = next else {
        break;
      };

      // 路径过滤（仅在主循环进行，任务内无需再次判断）
      if !self
        .processor
        .should_process_path_with(&meta.path, self.extra_path_filter.as_ref())
      {
        debug!("路径不匹配，跳过: {}", &meta.path);
        continue;
      }

      if meta.is_compressed {
        // tar.gz 等共享底层读取器的来源：必须保证串行处理，避免并发读取导致解码错乱
        while in_flight.next().await.is_some() {}
        match tokio::time::timeout(
          content_timeout,
          processor.process_content(meta.path.clone(), &mut reader),
        )
        .await
        {
          Ok(Ok(Some(result))) => {
            if tx.send(SearchEvent::Success(result)).await.is_err() {
              warn!("下游接收已关闭，终止条目流处理");
              break;
            }
          }
          Ok(Ok(None)) => {}
          Ok(Err(e)) => {
            warn!("处理条目内容失败: {}", e);
            let error_msg = format!("内容处理失败: {}", e);
            let _ = tx
              .send(SearchEvent::Error {
                source: "条目流#1".to_string(),
                message: error_msg,
                recoverable: true,
              })
              .await;
          }
          Err(_) => warn!("处理条目超时: {}", meta.path),
        }
      } else {
        // 本地文件等独立 Reader：可以并发处理
        let proc_clone = processor.clone();
        let tx_clone = tx.clone();
        let path = meta.path.clone();
        in_flight.push(async move {
          match tokio::time::timeout(content_timeout, proc_clone.process_content(path.clone(), &mut reader)).await {
            Ok(Ok(Some(result))) => {
              let _ = tx_clone.send(SearchEvent::Success(result)).await;
            }
            Ok(Ok(None)) => {}
            Ok(Err(e)) => {
              warn!("处理条目内容失败: {}", e);
              let error_msg = format!("内容处理失败: {}", e);
              let _ = tx_clone
                .send(SearchEvent::Error {
                  source: "条目流#2".to_string(),
                  message: error_msg,
                  recoverable: true,
                })
                .await;
            }
            Err(_) => {
              warn!("处理条目超时: {}", path);
            }
          }
        });
      }
    }

    // 等待所有在途任务完成
    while in_flight.next().await.is_some() {}

    Ok(())
  }
}

/// 条目流工厂：根据 SourceConfig 构造 Box<dyn EntryStream>
pub struct EntryStreamFactory {
  db_pool: SqlitePool,
}

impl EntryStreamFactory {
  pub fn new(db_pool: SqlitePool) -> Self {
    Self { db_pool }
  }

  /// 从来源配置创建条目流（不含 Agent）
  ///
  /// - Local: Dir/Files/Archive（自动探测 tar/tar.gz/gz/zip；zip 暂不支持）
  /// - S3: Archive（自动探测；zip 暂不支持）
  pub async fn create_stream(&self, source: crate::domain::config::Source) -> Result<Box<dyn EntryStream>, String> {
    use crate::domain::config::{Endpoint, Target};
    match (&source.endpoint, &source.target) {
      (Endpoint::Local { root }, Target::Dir { path, recursive }) => {
        let joined = if path == "." {
          root.clone()
        } else {
          format!("{}/{}", root, path)
        };
        // 使用 FsEntryStream 并尊重 recursive 标志
        let stream = FsEntryStream::new(PathBuf::from(joined), *recursive)
          .await
          .map_err(|e| format!("无法读取目录: {}", e))?;
        Ok(Box::new(stream))
      }
      (Endpoint::Local { root }, Target::Files { paths }) => {
        let files: Vec<String> = paths
          .iter()
          .map(|p| {
            if p.starts_with('/') {
              p.clone()
            } else {
              format!("{}/{}", root, p)
            }
          })
          .collect();
        Ok(Box::new(MultiFileEntryStream::new(files)))
      }
      (Endpoint::Local { root }, Target::Archive { path }) => {
        let full = if path.starts_with('/') {
          path.clone()
        } else {
          format!("{}/{}", root, path)
        };
        let file = tokio::fs::File::open(&full)
          .await
          .map_err(|e| format!("无法打开归档文件 {}: {}", full, e))?;
        create_archive_stream_from_reader(file, Some(&full)).await
      }
      (Endpoint::S3 { profile, bucket }, Target::Archive { path }) => {
        // 加载 Profile
        let profile_row = crate::repository::s3::load_s3_profile(&self.db_pool, profile)
          .await
          .map_err(|e| format!("加载 S3 Profile 失败: {:?}", e))?
          .ok_or_else(|| format!("S3 Profile 不存在: {}", profile))?;
        if &profile_row.bucket != bucket {
          tracing::warn!(
            "S3 配置中的桶与脚本提供不一致：db='{}' script='{}'，以脚本为准",
            profile_row.bucket,
            bucket
          );
        }
        // 构造读取器
        let reader = {
          use crate::utils::storage::{ReaderProvider as _, S3ReaderProvider, get_or_create_s3_client};
          let _ = get_or_create_s3_client(&profile_row.endpoint, &profile_row.access_key, &profile_row.secret_key)
            .map_err(|e| format!("创建 S3 客户端失败: {:?}", e))?;

          let provider = S3ReaderProvider::new(
            &profile_row.endpoint,
            &profile_row.access_key,
            &profile_row.secret_key,
            bucket,
            path,
          );
          provider
            .open()
            .await
            .map_err(|e| format!("打开 S3 对象失败: {:?}", e))?
        };
        create_archive_stream_from_reader(reader, Some(path)).await
      }
      (Endpoint::S3 { .. }, _) => Err("S3 仅支持 archive 目标".to_string()),
      (Endpoint::Agent { .. }, _) => Err("Agent 来源请通过远程 SearchService 处理".to_string()),
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
    let file = tokio::fs::File::open(&path).await?;
    let reader = BufReader::new(file);
    let name = std::path::Path::new(&path)
      .file_name()
      .and_then(|s| s.to_str())
      .map(|s| s.to_string())
      .unwrap_or_else(|| path.clone());
    let meta = EntryMeta {
      path: name,
      size: None,
      is_compressed: false,
    };
    Ok(Some((meta, Box::new(reader))))
  }
}

/// 构建本地来源条目流（单文件/目录/归档，支持 target 提示）
///
/// 根据 target 类型优先使用明确处理，否则自动检测路径类型
pub async fn build_local_entry_stream(
  root_or_file: &str,
  target: Option<crate::domain::config::Target>,
) -> Result<Box<dyn EntryStream>, String> {
  use crate::domain::config::Target;

  // 根据 target 类型进行精确处理（与 Server 端对齐）
  if let Some(target) = target {
    match target {
      Target::Files { paths } => {
        // Files 类型：直接使用 MultiFileEntryStream，与 Server 端一致
        return Ok(Box::new(MultiFileEntryStream::new(paths)));
      }
      Target::Dir { path, recursive } => {
        // Dir 类型：直接使用 FsEntryStream，与 Server 端一致
        // path 为 "." 时使用 root_or_file 作为根目录
        let dir_path = if path == "." {
          PathBuf::from(root_or_file)
        } else {
          PathBuf::from(root_or_file).join(path)
        };
        let stream = FsEntryStream::new(dir_path, recursive)
          .await
          .map_err(|e| format!("无法读取目录 {}: {}", root_or_file, e))?;
        return Ok(Box::new(stream));
      }
      Target::Archive { path } => {
        // Archive 类型：处理归档文件
        // 如果 root_or_file 本身已经是归档文件（通过扩展名判断），直接使用它
        // 否则拼接 path（适用于 root_or_file 是目录的情况）
        let archive_path = {
          let root_path = PathBuf::from(root_or_file);
          let root_lower = root_or_file.to_lowercase();
          if root_lower.ends_with(".tar")
            || root_lower.ends_with(".tar.gz")
            || root_lower.ends_with(".tgz")
            || root_lower.ends_with(".gz")
          {
            // root_or_file 本身就是归档文件，直接使用
            root_path
          } else {
            // root_or_file 是目录，需要拼接 path
            root_path.join(path)
          }
        };
        let file = tokio::fs::File::open(&archive_path)
          .await
          .map_err(|e| format!("无法打开归档文件 {}: {}", archive_path.display(), e))?;
        return create_archive_stream_from_reader(file, Some(&archive_path.to_string_lossy())).await;
      }
    }
  }

  // 自动检测路径类型（当 target 为 None 时）
  // 先检查是否为归档文件（基于扩展名）
  let lower = root_or_file.to_lowercase();
  if lower.ends_with(".tar")
    || lower.ends_with(".tar.gz")
    || lower.ends_with(".tgz")
    || lower.ends_with(".gz")
    || lower.ends_with(".zip")
  {
    let file = tokio::fs::File::open(root_or_file)
      .await
      .map_err(|e| format!("无法打开归档文件 {}: {}", root_or_file, e))?;
    return create_archive_stream_from_reader(file, Some(root_or_file)).await;
  }

  // 通过 metadata 检测文件或目录
  match tokio::fs::metadata(root_or_file).await {
    Ok(meta) if meta.is_file() => Ok(Box::new(MultiFileEntryStream::new(vec![root_or_file.to_string()]))),
    Ok(meta) if meta.is_dir() => {
      // 默认递归（与 Server 端 Target::Dir 的默认行为一致）
      let stream = FsEntryStream::new(PathBuf::from(root_or_file), true)
        .await
        .map_err(|e| format!("无法读取目录 {}: {}", root_or_file, e))?;
      Ok(Box::new(stream))
    }
    Ok(_) => Err(format!("不支持的文件类型: {}", root_or_file)),
    Err(e) => Err(format!("无法访问路径 {}: {}", root_or_file, e)),
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
  Tar,
  Gzip,
  Zip,
  Unknown,
}

async fn create_archive_stream_from_reader<R: AsyncRead + Send + Unpin + 'static>(
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
  let kind = sniff_archive_kind(&head);
  let prefixed = PrefixedReader::new(head, reader);
  match kind {
    ArchiveKind::Tar => {
      let stream = TarEntryStream::new(prefixed)
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
        let stream = TarEntryStream::new(gz_prefixed)
          .await
          .map_err(|e| format!("读取 tar(解压后) 失败: {}", e))?;
        Ok(Box::new(stream))
      } else {
        let name = hint_name
          .map(|s| {
            std::path::Path::new(s)
              .file_stem()
              .and_then(|x| x.to_str())
              .unwrap_or("<gzip>")
          })
          .unwrap_or("<gzip>")
          .to_string();
        let stream = GzipEntryStream::new(gz_prefixed, name, false);
        Ok(Box::new(stream))
      }
    }
    ArchiveKind::Zip => Err("ZIP 归档暂不支持".to_string()),
    ArchiveKind::Unknown => Err("未知归档格式或不支持的归档".to_string()),
  }
}

fn sniff_archive_kind(head: &[u8]) -> ArchiveKind {
  if head.len() >= 4 {
    let sig = &head[..4];
    if sig == [0x50, 0x4B, 0x03, 0x04] || sig == [0x50, 0x4B, 0x05, 0x06] || sig == [0x50, 0x4B, 0x07, 0x08] {
      info!("检测到归档类型: Zip");
      return ArchiveKind::Zip;
    }
  }
  // 优先检查 tar（tar 头在固定位置 257-262，更可靠）
  // 这样可以避免纯 tar 文件被误判为 gzip（如果前2字节恰好是 0x1F 0x8B）
  if head.len() >= 512 && &head[257..257 + 5] == b"ustar" {
    info!("检测到归档类型: Tar");
    return ArchiveKind::Tar;
  }
  // 然后检查 gzip（前2字节）
  if head.len() >= 2 && head[0] == 0x1F && head[1] == 0x8B {
    info!("检测到归档类型: Gzip");
    return ArchiveKind::Gzip;
  }
  info!("检测到归档类型: Unknown");
  ArchiveKind::Unknown
}

fn is_tar_header(head: &[u8]) -> bool {
  head.len() >= 512 && &head[257..257 + 5] == b"ustar"
}

struct PrefixedReader<R> {
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
  fn new(prefix: Vec<u8>, inner: R) -> Self {
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

pub struct TarEntryStream<R: AsyncRead + Send + Unpin + 'static> {
  entries: async_tar::Entries<tokio_util::compat::Compat<BufReader<R>>>,
}

impl<R: AsyncRead + Send + Unpin + 'static> TarEntryStream<R> {
  pub async fn new(reader: R) -> io::Result<Self> {
    let br = BufReader::new(reader);
    let archive = async_tar::Archive::new(br.compat());
    let entries = archive.entries()?;
    Ok(Self { entries })
  }
}

#[async_trait]
impl<R: AsyncRead + Send + Unpin + 'static> EntryStream for TarEntryStream<R> {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    loop {
      match self.entries.next().await {
        Some(Ok(entry)) => {
          let raw = entry
            .path()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "<unknown>".into());
          let path = normalize_archive_entry_path(&raw);
          let reader = entry.compat();
          let meta = EntryMeta {
            path,
            size: None,
            is_compressed: true,
          };
          return Ok(Some((meta, Box::new(reader))));
        }
        Some(Err(e)) => {
          // 记录错误但继续处理下一个条目
          tracing::warn!("跳过损坏的 tar 条目: {}", e);
          continue;
        }
        None => return Ok(None),
      }
    }
  }
}

pub struct GzipEntryStream<R: AsyncRead + Send + Unpin + 'static> {
  reader: Option<R>,
  name: String,
  is_compressed: bool,
}

impl<R: AsyncRead + Send + Unpin + 'static> GzipEntryStream<R> {
  pub fn new(reader: R, name: String, is_compressed: bool) -> Self {
    Self {
      reader: Some(reader),
      name,
      is_compressed,
    }
  }
}

#[async_trait]
impl<R: AsyncRead + Send + Unpin + 'static> EntryStream for GzipEntryStream<R> {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    if let Some(rd) = self.reader.take() {
      let meta = EntryMeta {
        path: self.name.clone(),
        size: None,
        is_compressed: self.is_compressed,
      };
      Ok(Some((meta, Box::new(rd))))
    } else {
      Ok(None)
    }
  }
}

/// 通用条目流处理函数（支持基于回调的结果处理）
///
/// 提供统一的条目流处理方式，可被 Server 和 Agent 复用，避免重复实现核心处理逻辑。
/// 事件通过回调函数返回，调用方可灵活处理（发送到 channel、生成消息等）。
///
/// # 參数
/// - stream: 条目流
/// - processor: 搜索处理器
/// - extra_path_filter: 额外路径过滤器（与用户查询的 path: 规则做 AND）
/// - result_callback: 事件回调函数，返回 true 继续处理，返回 false 停止处理
///
/// # 返回
/// 返回 (处理文件数, 匹配文件数)
pub async fn process_entry_stream_with_callback<F>(
  stream: Box<dyn EntryStream>,
  processor: Arc<crate::service::search::SearchProcessor>,
  extra_path_filter: Option<crate::query::PathFilter>,
  mut result_callback: F,
) -> Result<(usize, usize), String>
where
  F: FnMut(crate::service::search::SearchEvent) -> bool + Send,
{
  // 创建条目流处理器
  let mut stream_processor = EntryStreamProcessor::new(processor);
  if let Some(filter) = extra_path_filter {
    stream_processor = stream_processor.with_extra_path_filter(filter);
  }

  // 创建结果通道
  let (tx, mut rx) = tokio::sync::mpsc::channel(128);

  // 后台条目流处理任务
  let handle = tokio::spawn(async move {
    let mut stream = stream;
    let _ = stream_processor.process_stream(&mut *stream, tx).await;
  });

  let mut processed_count = 0;
  let mut matched_count = 0;

  // 收集结果并调用回调
  while let Some(result) = rx.recv().await {
    processed_count += 1;

    // 回调返回 false 则停止处理
    if !result_callback(result) {
      break;
    }

    matched_count += 1;
  }

  let _ = handle.await;
  Ok((processed_count, matched_count))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::query::Query;
  use sqlx::{sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions};
  use std::{path::Path, str::FromStr};
  use tempfile::TempDir;

  // ============================================================================
  // 测试辅助函数
  // ============================================================================

  /// 创建测试用的内存数据库连接池
  async fn create_test_pool() -> SqlitePool {
    let connect_options = SqliteConnectOptions::from_str("sqlite::memory:")
      .unwrap()
      .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
      .max_connections(1)
      .connect_with(connect_options)
      .await
      .expect("Failed to create test pool");

    crate::init_schema(&pool).await.expect("Failed to initialize schema");

    pool
  }

  /// 创建测试用的临时目录结构
  async fn create_test_directory() -> TempDir {
    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let root = temp_dir.path();

    // 创建文件结构:
    // root/
    //   file1.log (包含 "error")
    //   file2.txt (包含 "warning")
    //   subdir/
    //     file3.log (包含 "info")
    //     nested/
    //       file4.log (包含 "debug")

    tokio::fs::write(root.join("file1.log"), "line1\nerror occurred\nline3\n")
      .await
      .expect("写入 file1.log 失败");

    tokio::fs::write(root.join("file2.txt"), "line1\nwarning message\nline3\n")
      .await
      .expect("写入 file2.txt 失败");

    tokio::fs::create_dir(root.join("subdir"))
      .await
      .expect("创建 subdir 失败");

    tokio::fs::write(root.join("subdir/file3.log"), "line1\ninfo message\nline3\n")
      .await
      .expect("写入 file3.log 失败");

    tokio::fs::create_dir(root.join("subdir/nested"))
      .await
      .expect("创建 nested 失败");

    tokio::fs::write(root.join("subdir/nested/file4.log"), "line1\ndebug message\nline3\n")
      .await
      .expect("写入 file4.log 失败");

    temp_dir
  }

  /// 创建测试用的 tar.gz 归档文件（使用同步 I/O 以确保正确性）
  fn create_test_tar_gz(dir: &Path) -> PathBuf {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    let tar_gz_path = dir.join("test.tar.gz");
    let file = std::fs::File::create(&tar_gz_path).expect("创建 tar.gz 文件失败");
    let gz = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(gz);

    // 添加两个文件到归档
    let content1 = b"line1\nerror in archive\nline3\n";
    let mut header1 = tar::Header::new_gnu();
    header1.set_size(content1.len() as u64);
    header1.set_mode(0o644);
    header1.set_cksum();
    builder
      .append_data(&mut header1, "archived1.log", &content1[..])
      .expect("添加 archived1.log 失败");

    let content2 = b"line1\nwarning in archive\nline3\n";
    let mut header2 = tar::Header::new_gnu();
    header2.set_size(content2.len() as u64);
    header2.set_mode(0o644);
    header2.set_cksum();
    builder
      .append_data(&mut header2, "archived2.log", &content2[..])
      .expect("添加 archived2.log 失败");

    // 完成 tar 构建
    builder.finish().expect("完成 tar 构建失败");

    // 完成 gzip 编码
    let mut gz = builder.into_inner().expect("获取 gzip 编码器失败");
    gz.flush().expect("刷新 gzip 编码器失败");
    gz.finish().expect("完成 gzip 编码失败");

    tar_gz_path
  }

  // ============================================================================
  // FsEntryStream 测试（目录遍历）
  // ============================================================================

  #[tokio::test]
  async fn test_fs_entry_stream_non_recursive() {
    let temp_dir = create_test_directory().await;
    let root = temp_dir.path().to_path_buf();

    let mut stream = FsEntryStream::new(root.clone(), false)
      .await
      .expect("创建 FsEntryStream 失败");

    let mut entries = Vec::new();
    while let Some((meta, _reader)) = stream.next_entry().await.expect("读取条目失败") {
      entries.push(meta.path.clone());
    }

    // 非递归模式：只应该看到根目录下的文件
    assert_eq!(entries.len(), 2, "应该有 2 个文件");
    assert!(entries.contains(&"file1.log".to_string()), "应该包含 file1.log");
    assert!(entries.contains(&"file2.txt".to_string()), "应该包含 file2.txt");
    assert!(
      !entries.iter().any(|p| p.contains("subdir")),
      "不应该包含子目录中的文件"
    );
  }

  #[tokio::test]
  async fn test_fs_entry_stream_recursive() {
    let temp_dir = create_test_directory().await;
    let root = temp_dir.path().to_path_buf();

    let mut stream = FsEntryStream::new(root.clone(), true)
      .await
      .expect("创建 FsEntryStream 失败");

    let mut entries = Vec::new();
    while let Some((meta, _reader)) = stream.next_entry().await.expect("读取条目失败") {
      entries.push(meta.path.clone());
    }

    // 递归模式：应该看到所有文件
    assert_eq!(entries.len(), 4, "应该有 4 个文件");
    assert!(entries.contains(&"file1.log".to_string()), "应该包含 file1.log");
    assert!(entries.contains(&"file2.txt".to_string()), "应该包含 file2.txt");

    // 检查子目录中的文件（路径格式可能是 "subdir/file3.log" 或 "subdir\\file3.log"）
    assert!(entries.iter().any(|p| p.contains("file3.log")), "应该包含 file3.log");
    assert!(entries.iter().any(|p| p.contains("file4.log")), "应该包含 file4.log");
  }

  // ============================================================================
  // MultiFileEntryStream 测试（文件列表）
  // ============================================================================

  #[tokio::test]
  async fn test_multi_file_entry_stream() {
    let temp_dir = create_test_directory().await;
    let root = temp_dir.path();

    let files = vec![
      root.join("file1.log").to_string_lossy().to_string(),
      root.join("file2.txt").to_string_lossy().to_string(),
    ];

    let mut stream = MultiFileEntryStream::new(files);

    let mut entries = Vec::new();
    while let Some((meta, _reader)) = stream.next_entry().await.expect("读取条目失败") {
      entries.push(meta.path.clone());
    }

    assert_eq!(entries.len(), 2, "应该有 2 个文件");
    assert!(entries.contains(&"file1.log".to_string()), "应该包含 file1.log");
    assert!(entries.contains(&"file2.txt".to_string()), "应该包含 file2.txt");
  }

  #[tokio::test]
  async fn test_multi_file_entry_stream_empty() {
    let mut stream = MultiFileEntryStream::new(vec![]);

    let entry = stream.next_entry().await.expect("读取条目失败");
    assert!(entry.is_none(), "空文件列表应该返回 None");
  }

  // ============================================================================
  // TarGzEntryStream 测试（tar.gz 归档）
  // ============================================================================

  #[tokio::test]
  async fn test_tar_gz_entry_stream() {
    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let tar_gz_path = create_test_tar_gz(temp_dir.path());

    let file = tokio::fs::File::open(&tar_gz_path).await.expect("打开 tar.gz 文件失败");

    let mut stream = TarGzEntryStream::new(file).await.expect("创建 TarGzEntryStream 失败");

    let mut entries = Vec::new();
    while let Some((meta, mut reader)) = stream.next_entry().await.expect("读取条目失败") {
      entries.push(meta.path.clone());
      assert!(meta.is_compressed, "tar.gz 条目应该标记为 compressed");

      // 必须读取内容才能移动到下一个条目（tar 格式要求）
      let mut content = Vec::new();
      tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut content)
        .await
        .expect("读取条目内容失败");
    }

    assert_eq!(entries.len(), 2, "应该有 2 个归档条目");
    assert!(entries.contains(&"archived1.log".to_string()), "应该包含 archived1.log");
    assert!(entries.contains(&"archived2.log".to_string()), "应该包含 archived2.log");
  }

  #[tokio::test]
  async fn test_tar_gz_entry_stream_read_content() {
    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let tar_gz_path = create_test_tar_gz(temp_dir.path());

    let file = tokio::fs::File::open(&tar_gz_path).await.expect("打开 tar.gz 文件失败");

    let mut stream = TarGzEntryStream::new(file).await.expect("创建 TarGzEntryStream 失败");

    // 读取第一个条目的内容
    if let Some((meta, mut reader)) = stream.next_entry().await.expect("读取条目失败") {
      assert_eq!(meta.path, "archived1.log");

      let mut content = String::new();
      tokio::io::AsyncReadExt::read_to_string(&mut reader, &mut content)
        .await
        .expect("读取内容失败");

      assert!(content.contains("error in archive"), "内容应该包含 'error in archive'");
    } else {
      panic!("应该有至少一个条目");
    }
  }

  // ============================================================================
  // build_local_entry_stream 测试（自动检测）
  // ============================================================================

  #[tokio::test]
  async fn test_build_local_entry_stream_directory() {
    let temp_dir = create_test_directory().await;
    let root = temp_dir.path().to_string_lossy().to_string();

    // 测试自动检测目录（无 target 提示）
    let mut stream = build_local_entry_stream(&root, None).await.expect("构建条目流失败");

    let mut count = 0;
    while let Some((_meta, _reader)) = stream.next_entry().await.expect("读取条目失败") {
      count += 1;
    }

    // 默认递归，应该有 4 个文件
    assert!(count >= 2, "应该至少有 2 个文件");
  }

  #[tokio::test]
  async fn test_build_local_entry_stream_with_dir_target() {
    use crate::domain::config::Target;

    let temp_dir = create_test_directory().await;
    let root = temp_dir.path().to_string_lossy().to_string();

    // 测试使用 Dir target（非递归）
    let target = Target::Dir {
      path: ".".to_string(),
      recursive: false,
    };

    let mut stream = build_local_entry_stream(&root, Some(target))
      .await
      .expect("构建条目流失败");

    let mut entries = Vec::new();
    while let Some((meta, _reader)) = stream.next_entry().await.expect("读取条目失败") {
      entries.push(meta.path.clone());
    }

    // 非递归，只应该有根目录的文件
    assert_eq!(entries.len(), 2, "非递归应该只有 2 个文件");
  }

  #[tokio::test]
  async fn test_build_local_entry_stream_with_files_target() {
    use crate::domain::config::Target;

    let temp_dir = create_test_directory().await;
    let root = temp_dir.path();

    let files = vec![
      root.join("file1.log").to_string_lossy().to_string(),
      root.join("file2.txt").to_string_lossy().to_string(),
    ];

    let target = Target::Files { paths: files.clone() };

    let mut stream = build_local_entry_stream(root.to_str().unwrap(), Some(target))
      .await
      .expect("构建条目流失败");

    let mut entries = Vec::new();
    while let Some((meta, _reader)) = stream.next_entry().await.expect("读取条目失败") {
      entries.push(meta.path.clone());
    }

    assert_eq!(entries.len(), 2, "应该有 2 个文件");
  }

  #[tokio::test]
  async fn test_build_local_entry_stream_with_archive_target() {
    use crate::domain::config::Target;

    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let tar_gz_path = create_test_tar_gz(temp_dir.path());

    let target = Target::Archive {
      path: tar_gz_path.file_name().unwrap().to_string_lossy().to_string(),
    };

    let mut stream = build_local_entry_stream(temp_dir.path().to_str().unwrap(), Some(target))
      .await
      .expect("构建条目流失败");

    let mut entries = Vec::new();
    while let Some((meta, mut reader)) = stream.next_entry().await.expect("读取条目失败") {
      entries.push(meta.path.clone());
      // 必须读取内容才能移动到下一个条目
      let mut content = Vec::new();
      tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut content)
        .await
        .expect("读取条目内容失败");
    }

    assert_eq!(entries.len(), 2, "归档应该有 2 个条目");
  }

  #[tokio::test]
  async fn test_build_local_entry_stream_auto_detect_tar_gz() {
    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let tar_gz_path = create_test_tar_gz(temp_dir.path());

    // 测试自动检测 tar.gz 文件（基于扩展名）
    let mut stream = build_local_entry_stream(&tar_gz_path.to_string_lossy(), None)
      .await
      .expect("构建条目流失败");

    let mut entries = Vec::new();
    while let Some((meta, mut reader)) = stream.next_entry().await.expect("读取条目失败") {
      entries.push(meta.path.clone());
      // 必须读取内容才能移动到下一个条目
      let mut content = Vec::new();
      tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut content)
        .await
        .expect("读取条目内容失败");
    }

    assert_eq!(entries.len(), 2, "归档应该有 2 个条目");
  }

  // ============================================================================
  // EntryStreamFactory 测试（完整集成）
  // ============================================================================

  #[tokio::test]
  async fn test_entry_stream_factory_local_dir() {
    use crate::domain::config::{Endpoint, Source, Target};

    let temp_dir = create_test_directory().await;
    let root = temp_dir.path().to_string_lossy().to_string();

    let pool = create_test_pool().await;
    let factory = EntryStreamFactory::new(pool);

    let source = Source {
      endpoint: Endpoint::Local { root: root.clone() },
      target: Target::Dir {
        path: ".".to_string(),
        recursive: true,
      },
      filter_glob: None,
      display_name: None,
    };

    let mut stream = factory.create_stream(source).await.expect("创建条目流失败");

    let mut count = 0;
    while let Some((_meta, _reader)) = stream.next_entry().await.expect("读取条目失败") {
      count += 1;
    }

    assert_eq!(count, 4, "递归模式应该有 4 个文件");
  }

  #[tokio::test]
  async fn test_entry_stream_factory_local_files() {
    use crate::domain::config::{Endpoint, Source, Target};

    let temp_dir = create_test_directory().await;
    let root = temp_dir.path().to_string_lossy().to_string();

    let pool = create_test_pool().await;
    let factory = EntryStreamFactory::new(pool);

    let files = vec!["file1.log".to_string(), "file2.txt".to_string()];

    let source = Source {
      endpoint: Endpoint::Local { root: root.clone() },
      target: Target::Files { paths: files },
      filter_glob: None,
      display_name: None,
    };

    let mut stream = factory.create_stream(source).await.expect("创建条目流失败");

    let mut entries = Vec::new();
    while let Some((meta, _reader)) = stream.next_entry().await.expect("读取条目失败") {
      entries.push(meta.path.clone());
    }

    assert_eq!(entries.len(), 2, "应该有 2 个文件");
  }

  #[tokio::test]
  async fn test_entry_stream_factory_local_archive() {
    use crate::domain::config::{Endpoint, Source, Target};

    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let tar_gz_path = create_test_tar_gz(temp_dir.path());

    let pool = create_test_pool().await;
    let factory = EntryStreamFactory::new(pool);

    let source = Source {
      endpoint: Endpoint::Local {
        root: temp_dir.path().to_string_lossy().to_string(),
      },
      target: Target::Archive {
        path: tar_gz_path.file_name().unwrap().to_string_lossy().to_string(),
      },
      filter_glob: None,
      display_name: None,
    };

    let mut stream = factory.create_stream(source).await.expect("创建条目流失败");

    let mut entries = Vec::new();
    while let Some((meta, mut reader)) = stream.next_entry().await.expect("读取条目失败") {
      entries.push(meta.path.clone());
      // 必须读取内容才能移动到下一个条目
      let mut content = Vec::new();
      tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut content)
        .await
        .expect("读取条目内容失败");
    }

    assert_eq!(entries.len(), 2, "归档应该有 2 个条目");
  }

  // ============================================================================
  // EntryStreamProcessor 测试（内容处理）
  // ============================================================================

  #[tokio::test]
  async fn test_entry_stream_processor_basic() {
    let temp_dir = create_test_directory().await;
    let root = temp_dir.path().to_path_buf();

    // 创建搜索处理器
    let query = Query::parse_github_like("error").expect("解析查询失败");
    let processor = Arc::new(crate::service::search::SearchProcessor::new(Arc::new(query), 1));

    // 创建条目流
    let mut stream = FsEntryStream::new(root, true).await.expect("创建 FsEntryStream 失败");

    // 创建结果通道
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);

    // 处理条目流
    let mut stream_processor = EntryStreamProcessor::new(processor);
    let handle = tokio::spawn(async move {
      stream_processor
        .process_stream(&mut stream, tx)
        .await
        .expect("处理条目流失败");
    });

    // 收集结果
    let mut results = Vec::new();
    while let Some(event) = rx.recv().await {
      if let crate::service::search::SearchEvent::Success(result) = event {
        results.push(result);
      }
    }

    handle.await.expect("任务失败");

    // 应该找到包含 "error" 的文件
    assert!(!results.is_empty(), "应该找到至少一个匹配");
    assert!(
      results.iter().any(|r| r.path.contains("file1.log")),
      "应该匹配 file1.log"
    );
  }

  #[tokio::test]
  async fn test_entry_stream_processor_with_path_filter() {
    let temp_dir = create_test_directory().await;
    let root = temp_dir.path().to_path_buf();

    // 创建搜索处理器（匹配所有内容，带路径过滤）
    let query = Query::parse_github_like("line path:*.log").expect("解析查询失败");
    let processor = Arc::new(crate::service::search::SearchProcessor::new(Arc::new(query), 0));

    // 创建条目流
    let mut stream = FsEntryStream::new(root, true).await.expect("创建 FsEntryStream 失败");

    // 创建结果通道
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);

    // 处理条目流
    let mut stream_processor = EntryStreamProcessor::new(processor);
    let handle = tokio::spawn(async move {
      stream_processor
        .process_stream(&mut stream, tx)
        .await
        .expect("处理条目流失败");
    });

    // 收集结果
    let mut results = Vec::new();
    while let Some(event) = rx.recv().await {
      if let crate::service::search::SearchEvent::Success(result) = event {
        results.push(result);
      }
    }

    handle.await.expect("任务失败");

    // 应该只匹配 .log 文件，不匹配 .txt 文件
    assert!(!results.is_empty(), "应该找到至少一个匹配");
    assert!(
      results.iter().all(|r| r.path.ends_with(".log")),
      "所有结果应该是 .log 文件"
    );
    assert!(
      !results.iter().any(|r| r.path.ends_with(".txt")),
      "不应该匹配 .txt 文件"
    );
  }

  // ============================================================================
  // S3 数据源测试（使用 mock）
  // ============================================================================

  /// Mock S3 Reader：模拟 S3 对象读取
  struct MockS3Reader {
    content: std::io::Cursor<Vec<u8>>,
  }

  impl MockS3Reader {
    fn new(content: Vec<u8>) -> Self {
      Self {
        content: std::io::Cursor::new(content),
      }
    }
  }

  impl AsyncRead for MockS3Reader {
    fn poll_read(
      mut self: std::pin::Pin<&mut Self>,
      _cx: &mut std::task::Context<'_>,
      buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
      let mut tmp = vec![0u8; buf.remaining()];
      let n = std::io::Read::read(&mut self.content, &mut tmp).unwrap_or(0);
      if n > 0 {
        buf.put_slice(&tmp[..n]);
      }
      std::task::Poll::Ready(Ok(()))
    }
  }

  #[tokio::test]
  async fn test_s3_archive_stream_creation() {
    // 创建 mock tar.gz 内容
    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let tar_gz_path = create_test_tar_gz(temp_dir.path());

    // 读取 tar.gz 文件内容
    let content = std::fs::read(&tar_gz_path).expect("读取 tar.gz 文件失败");

    // 使用 mock reader 创建归档流
    let mock_reader = MockS3Reader::new(content);
    let mut stream = create_archive_stream_from_reader(mock_reader, Some("test.tar.gz"))
      .await
      .expect("创建归档流失败");

    // 验证可以读取条目
    let mut entries = Vec::new();
    while let Some((meta, mut reader)) = stream.next_entry().await.expect("读取条目失败") {
      entries.push(meta.path.clone());
      assert!(meta.is_compressed, "归档条目应该标记为 compressed");

      // 读取内容
      let mut content = Vec::new();
      tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut content)
        .await
        .expect("读取条目内容失败");
    }

    assert_eq!(entries.len(), 2, "应该有 2 个归档条目");
    assert!(entries.contains(&"archived1.log".to_string()), "应该包含 archived1.log");
    assert!(entries.contains(&"archived2.log".to_string()), "应该包含 archived2.log");
  }

  #[tokio::test]
  async fn test_s3_archive_content_processing() {
    // 创建 mock tar.gz 内容
    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let tar_gz_path = create_test_tar_gz(temp_dir.path());

    // 读取 tar.gz 文件内容
    let content = std::fs::read(&tar_gz_path).expect("读取 tar.gz 文件失败");

    // 使用 mock reader 创建归档流
    let mock_reader = MockS3Reader::new(content);
    let mut stream = create_archive_stream_from_reader(mock_reader, Some("test.tar.gz"))
      .await
      .expect("创建归档流失败");

    // 创建搜索处理器
    let query = Query::parse_github_like("error").expect("解析查询失败");
    let processor = Arc::new(crate::service::search::SearchProcessor::new(Arc::new(query), 1));

    // 创建结果通道
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);

    // 处理条目流
    let mut stream_processor = EntryStreamProcessor::new(processor);
    let handle = tokio::spawn(async move {
      stream_processor
        .process_stream(&mut *stream, tx)
        .await
        .expect("处理条目流失败");
    });

    // 收集结果
    let mut results = Vec::new();
    while let Some(event) = rx.recv().await {
      if let crate::service::search::SearchEvent::Success(result) = event {
        results.push(result);
      }
    }

    handle.await.expect("任务失败");

    // 应该找到包含 "error" 的归档条目
    assert!(!results.is_empty(), "应该找到至少一个匹配");
    assert!(
      results.iter().any(|r| r.path == "archived1.log"),
      "应该匹配 archived1.log"
    );
  }

  #[tokio::test]
  async fn test_s3_reader_error_handling() {
    /// Mock 失败的 S3 Reader：模拟读取错误
    struct FailingS3Reader {
      fail_after: usize,
      read_count: usize,
    }

    impl FailingS3Reader {
      fn new(fail_after: usize) -> Self {
        Self {
          fail_after,
          read_count: 0,
        }
      }
    }

    impl AsyncRead for FailingS3Reader {
      fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
      ) -> std::task::Poll<std::io::Result<()>> {
        self.read_count += 1;
        if self.read_count > self.fail_after {
          return std::task::Poll::Ready(Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionAborted,
            "模拟 S3 连接失败",
          )));
        }
        // 返回空数据（EOF）
        std::task::Poll::Ready(Ok(()))
      }
    }

    // 测试读取失败的情况
    let failing_reader = FailingS3Reader::new(0);
    let result = create_archive_stream_from_reader(failing_reader, Some("test.tar.gz")).await;

    // 应该返回错误（因为无法读取足够的头部数据）
    assert!(result.is_err(), "应该返回错误");
  }

  #[tokio::test]
  async fn test_s3_single_file_archive() {
    // 创建只包含一个文件的 tar.gz 归档
    use flate2::Compression;
    use flate2::write::GzEncoder;

    let mut gz_data = Vec::new();
    {
      let gz = GzEncoder::new(&mut gz_data, Compression::default());
      let mut builder = tar::Builder::new(gz);

      // 添加一个小文件
      let content = b"single file content\n";
      let mut header = tar::Header::new_gnu();
      header.set_size(content.len() as u64);
      header.set_mode(0o644);
      header.set_cksum();
      builder
        .append_data(&mut header, "single.log", &content[..])
        .expect("添加文件失败");

      builder.finish().expect("完成 tar 构建失败");
      let mut gz = builder.into_inner().expect("获取 gzip 编码器失败");
      use std::io::Write;
      gz.flush().expect("刷新 gzip 编码器失败");
      gz.finish().expect("完成 gzip 编码失败");
    }

    // 使用 mock reader 创建归档流
    let mock_reader = MockS3Reader::new(gz_data);
    let mut stream = create_archive_stream_from_reader(mock_reader, Some("single.tar.gz"))
      .await
      .expect("创建归档流失败");

    // 验证只有一个条目
    let mut entry_count = 0;
    while let Some((meta, mut reader)) = stream.next_entry().await.expect("读取条目失败") {
      entry_count += 1;
      assert_eq!(meta.path, "single.log", "文件名应该是 single.log");

      // 读取内容
      let mut content = String::new();
      tokio::io::AsyncReadExt::read_to_string(&mut reader, &mut content)
        .await
        .expect("读取条目内容失败");
      assert_eq!(content, "single file content\n", "内容应该匹配");
    }
    assert_eq!(entry_count, 1, "应该只有一个文件条目");
  }

  #[tokio::test]
  async fn test_s3_large_archive_entry() {
    // 创建包含大文件的 tar.gz 归档
    use flate2::Compression;
    use flate2::write::GzEncoder;

    let mut gz_data = Vec::new();
    {
      let gz = GzEncoder::new(&mut gz_data, Compression::default());
      let mut builder = tar::Builder::new(gz);

      // 创建 1MB 的内容
      let large_content = vec![b'x'; 1024 * 1024];
      let mut header = tar::Header::new_gnu();
      header.set_size(large_content.len() as u64);
      header.set_mode(0o644);
      header.set_cksum();
      builder
        .append_data(&mut header, "large.log", &large_content[..])
        .expect("添加大文件失败");

      builder.finish().expect("完成 tar 构建失败");
      let mut gz = builder.into_inner().expect("获取 gzip 编码器失败");
      use std::io::Write;
      gz.flush().expect("刷新 gzip 编码器失败");
      gz.finish().expect("完成 gzip 编码失败");
    }

    // 使用 mock reader 创建归档流
    let mock_reader = MockS3Reader::new(gz_data);
    let mut stream = create_archive_stream_from_reader(mock_reader, Some("large.tar.gz"))
      .await
      .expect("创建归档流失败");

    // 验证可以读取大文件
    if let Some((meta, mut reader)) = stream.next_entry().await.expect("读取条目失败") {
      assert_eq!(meta.path, "large.log");

      // 读取内容
      let mut content = Vec::new();
      tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut content)
        .await
        .expect("读取大文件内容失败");

      assert_eq!(content.len(), 1024 * 1024, "内容大小应该是 1MB");
    } else {
      panic!("应该有一个条目");
    }
  }
}
