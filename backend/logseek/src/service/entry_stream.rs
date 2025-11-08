use std::{io, path::PathBuf, sync::Arc, time::Duration};

use async_compression::tokio::bufread::GzipDecoder;
use async_trait::async_trait;
use futures::{StreamExt, stream::FuturesUnordered};
use log::{debug, warn};
use tokio::io::{AsyncRead, BufReader};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

// 统一读取并发度：使用 ENTRY_CONCURRENCY（范围 1-64，默认 8）
fn entry_concurrency() -> usize {
  std::env::var("ENTRY_CONCURRENCY")
    .ok()
    .and_then(|s| s.parse::<usize>().ok())
    .unwrap_or(8)
    .clamp(1, 64)
}

use super::search::{SearchProcessor, SearchResult};
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
}

impl FsEntryStream {
  /// 从根目录创建条目流
  pub async fn new(root: PathBuf) -> io::Result<Self> {
    let rd = tokio::fs::read_dir(&root).await?;
    Ok(Self {
      stack: vec![rd],
      root: Some(root),
    })
  }

  /// 直接从已存在的 ReadDir 创建（无根路径信息）
  pub fn from_read_dir(rd: tokio::fs::ReadDir) -> Self {
    Self {
      stack: vec![rd],
      root: None,
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
            if let Ok(sub) = tokio::fs::read_dir(entry.path()).await {
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

/// 单文件条目流（只产出一次）
pub struct SingleFileEntryStream {
  path: PathBuf,
  yielded: bool,
}

impl SingleFileEntryStream {
  /// 创建单文件条目流
  pub fn new(path: PathBuf) -> Self {
    Self { path, yielded: false }
  }
}

#[async_trait]
impl EntryStream for SingleFileEntryStream {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    if self.yielded {
      return Ok(None);
    }
    // 打开单个文件并产生一次条目
    let file = tokio::fs::File::open(&self.path).await?;
    let reader = BufReader::new(file);
    let name = self
      .path
      .file_name()
      .and_then(|s| s.to_str())
      .map(|s| s.to_string())
      .unwrap_or_else(|| self.path.to_string_lossy().to_string());
    let meta = EntryMeta {
      path: name,
      size: None,
      is_compressed: false,
    };
    self.yielded = true;
    Ok(Some((meta, Box::new(reader))))
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
        Ok(Some((meta, Box::new(reader))))
      }
      Some(Err(e)) => Err(e),
      None => Ok(None),
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

  /// 并发处理条目（有界并发，默认并发度 8，可通过 ENTRY_CONCURRENCY 环境变量调整，范围 1-64）
  pub async fn process_stream(
    &mut self,
    entries: &mut dyn EntryStream,
    tx: tokio::sync::mpsc::Sender<SearchResult>,
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
            if processor.send_result(result, &tx).await.is_err() {
              warn!("下游接收已关闭，终止条目流处理");
              break;
            }
          }
          Ok(Ok(None)) => {}
          Ok(Err(e)) => warn!("处理条目内容失败: {}", e),
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
              let _ = proc_clone.send_result(result, &tx_clone).await;
            }
            Ok(Ok(None)) => {}
            Ok(Err(e)) => {
              warn!("处理条目内容失败: {}", e);
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

  /// 构建本地来源条目流（单文件/目录/tar.gz，支持 scope=TarGz 相对路径）
  pub async fn build_local_entry_stream(
    &self,
    root_or_file: &str,
    scope: Option<crate::agent::SearchScope>,
  ) -> Result<Box<dyn EntryStream>, String> {
    use crate::agent::SearchScope;
    if let Some(SearchScope::TarGz { path: rel }) = scope {
      let targz_path = PathBuf::from(root_or_file).join(rel);
      let file = tokio::fs::File::open(&targz_path)
        .await
        .map_err(|e| format!("无法打开 tar.gz 文件 {}: {}", targz_path.display(), e))?;
let stream = TarGzEntryStream::new(file)
        .await
        .map_err(|e| format!("无法读取 tar.gz 文件 {}: {}", targz_path.display(), e))?;
      return Ok(Box::new(stream));
    }

    let lower = root_or_file.to_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
      let file = tokio::fs::File::open(root_or_file)
        .await
        .map_err(|e| format!("无法打开 tar.gz 文件 {}: {}", root_or_file, e))?;
let stream = TarGzEntryStream::new(file)
        .await
        .map_err(|e| format!("无法读取 tar.gz 文件 {}: {}", root_or_file, e))?;
      return Ok(Box::new(stream));
    }

    match tokio::fs::metadata(root_or_file).await {
      Ok(meta) if meta.is_file() => Ok(Box::new(SingleFileEntryStream::new(PathBuf::from(root_or_file)))),
      Ok(meta) if meta.is_dir() => {
        let stream = FsEntryStream::new(PathBuf::from(root_or_file))
          .await
          .map_err(|e| format!("无法读取目录 {}: {}", root_or_file, e))?;
        Ok(Box::new(stream))
      }
      Ok(_) => Err(format!("不支持的文件类型: {}", root_or_file)),
      Err(e) => Err(format!("无法访问路径 {}: {}", root_or_file, e)),
    }
  }

  /// 从来源配置创建条目流（不含 Agent）
  ///
  /// - Local: Dir/Files/Archive（自动探测 tar/tar.gz/gz/zip；zip 暂不支持）
  /// - S3: Archive（自动探测；zip 暂不支持）
  pub async fn create_stream(&self, source: crate::domain::config::Source) -> Result<Box<dyn EntryStream>, String> {
    use crate::domain::config::{Endpoint, Target};
    match (&source.endpoint, &source.target) {
      (Endpoint::Local { root }, Target::Dir { path, .. }) => {
        let joined = if path == "." {
          root.clone()
        } else {
          format!("{}/{}", root, path)
        };
        crate::service::entry_stream::build_local_entry_stream(&joined, None).await
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
        let profile_row = crate::repository::settings::load_s3_profile(&self.db_pool, profile)
          .await
          .map_err(|e| format!("加载 S3 Profile 失败: {:?}", e))?
          .ok_or_else(|| format!("S3 Profile 不存在: {}", profile))?;
        if &profile_row.bucket != bucket {
          log::warn!(
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
      (Endpoint::Local { root }, Target::All) => {
        crate::service::entry_stream::build_local_entry_stream(root, None).await
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

/// 构建本地来源条目流（单文件/目录/归档，支持 scope=TarGz 相对路径）
pub async fn build_local_entry_stream(
  root_or_file: &str,
  scope: Option<crate::agent::SearchScope>,
) -> Result<Box<dyn EntryStream>, String> {
  use crate::agent::SearchScope;
  if let Some(SearchScope::TarGz { path: rel }) = scope {
    let targz_path = PathBuf::from(root_or_file).join(rel);
    let file = tokio::fs::File::open(&targz_path)
      .await
      .map_err(|e| format!("无法打开 tar.gz 文件 {}: {}", targz_path.display(), e))?;
let stream = TarGzEntryStream::new(file)
      .await
      .map_err(|e| format!("无法读取 tar.gz 文件 {}: {}", targz_path.display(), e))?;
    return Ok(Box::new(stream));
  }

  // 自动基于魔数识别归档类型
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

  match tokio::fs::metadata(root_or_file).await {
    Ok(meta) if meta.is_file() => Ok(Box::new(SingleFileEntryStream::new(PathBuf::from(root_or_file)))),
    Ok(meta) if meta.is_dir() => {
      let stream = FsEntryStream::new(PathBuf::from(root_or_file))
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
      return ArchiveKind::Zip;
    }
  }
  if head.len() >= 2 && head[0] == 0x1F && head[1] == 0x8B {
    return ArchiveKind::Gzip;
  }
  if head.len() >= 512 && &head[257..257 + 5] == b"ustar" {
    return ArchiveKind::Tar;
  }
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
        Ok(Some((meta, Box::new(reader))))
      }
      Some(Err(e)) => Err(e),
      None => Ok(None),
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
/// 结果通过回调函数返回，调用方可灵活处理（发送到 channel、生成消息等）。
///
/// # 参数
/// - stream: 条目流
/// - processor: 搜索处理器
/// - extra_path_filter: 额外路径过滤器（与用户查询的 path: 规则做 AND）
/// - result_callback: 结果回调函数，返回 true 继续处理，返回 false 停止处理
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
  F: FnMut(crate::service::search::SearchResult) -> bool + Send,
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
