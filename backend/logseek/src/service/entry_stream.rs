use std::{io, path::PathBuf, sync::Arc, time::Duration};

use futures::{StreamExt, stream::FuturesUnordered};
use num_cpus;
use tokio::io::{AsyncRead, AsyncReadExt};
use tracing::{trace, warn};

use opsbox_core::SqlitePool;
use opsbox_core::fs::{
  EntrySource, EntryStream, FsEntryStream, MultiFileEntryStream, create_archive_stream_from_reader,
};

use super::search::{SearchEvent, SearchProcessor};

// 统一读取并发度：使用 ENTRY_CONCURRENCY（范围 1-128）
// 对于 CPU 密集型任务（同机部署），根据 CPU 核心数动态调整，充分利用多核 CPU
fn entry_concurrency() -> usize {
  // 优先使用环境变量
  if let Ok(val) = std::env::var("ENTRY_CONCURRENCY")
    && let Ok(parsed) = val.parse::<usize>()
  {
    return parsed.clamp(1, 128);
  }

  // 默认值：根据 CPU 核心数动态计算
  // 对于 CPU 密集型任务（解压、搜索），建议设置为 CPU 核心数的 2-4 倍
  // 这样可以充分利用多核，同时避免过多的上下文切换
  let cpu_count = num_cpus::get();
  let default = (cpu_count * 2).clamp(8, 32); // 至少 8，最多 32，默认 2 倍核心数

  default.clamp(1, 128)
}

// 已删除 try_resolve_minio_local_path 函数
// 原因：MinIO 使用 Erasure Coding，数据被分片存储，无法直接通过 bucket/key 映射到文件系统路径
// 即使在同一台机器上，也必须通过 S3 API 访问，以确保数据一致性和安全性

/// 预读结果：小文件完整内容，或大文件的已读取部分
enum PreloadResult {
  /// 小文件：完整内容已读取
  Complete(Vec<u8>),
  /// 大文件：已读取部分内容（reader 已被部分消费）
  Partial(Vec<u8>),
}

/// 组合 Reader：先读取 prefix，然后读取 inner
/// 用于处理预读时已读取的部分内容
struct ChainedReader<R> {
  prefix: std::io::Cursor<Vec<u8>>,
  inner: R,
  prefix_done: bool,
}

// 确保 ChainedReader 实现 Unpin（如果 R 是 Unpin 的）
impl<R: Unpin> Unpin for ChainedReader<R> {}

impl<R: AsyncRead + Unpin> ChainedReader<R> {
  fn new(prefix: Vec<u8>, inner: R) -> Self {
    Self {
      prefix: std::io::Cursor::new(prefix),
      inner,
      prefix_done: false,
    }
  }
}

impl<R: AsyncRead + Unpin> AsyncRead for ChainedReader<R> {
  fn poll_read(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
    buf: &mut tokio::io::ReadBuf<'_>,
  ) -> std::task::Poll<io::Result<()>> {
    // 先读取 prefix 部分
    if !self.prefix_done {
      let pos = self.prefix.position() as usize;
      let prefix_len = self.prefix.get_ref().len();
      if pos < prefix_len {
        let remaining = prefix_len - pos;
        let to_read = remaining.min(buf.remaining());
        if to_read > 0 {
          let mut temp = vec![0u8; to_read];
          match std::io::Read::read(&mut self.prefix, &mut temp) {
            Ok(n) if n > 0 => {
              buf.put_slice(&temp[..n]);
              return std::task::Poll::Ready(Ok(()));
            }
            Ok(_) => {
              self.prefix_done = true;
            }
            Err(e) => return std::task::Poll::Ready(Err(e)),
          }
        }
      }
      self.prefix_done = true;
    }

    // prefix 读取完毕，继续读取 inner
    std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
  }
}

/// 预读文件条目到内存
/// 返回：
/// - Complete(content): 文件完全读取（小文件）
/// - Partial(content): 文件太大，只读取了部分（reader 已被部分消费）
async fn preload_entry(reader: &mut (dyn AsyncRead + Send + Unpin), max_size: usize) -> io::Result<PreloadResult> {
  let mut buffer = Vec::with_capacity(64 * 1024); // 64KB 初始容量
  let mut temp = vec![0u8; 64 * 1024];

  loop {
    let n = reader.read(&mut temp).await?;
    if n == 0 {
      // EOF，文件完全读取
      return Ok(PreloadResult::Complete(buffer));
    }
    buffer.extend_from_slice(&temp[..n]);

    // 如果超过最大大小，返回已读取的部分
    if buffer.len() > max_size {
      return Ok(PreloadResult::Partial(buffer));
    }
  }
}

/// 统一条目流处理器：消费 EntryStream，调用 SearchProcessor 处理内容
pub struct EntryStreamProcessor {
  processor: Arc<SearchProcessor>,
  content_timeout: Duration,
  // 额外路径过滤器（可选），与用户查询中的 path: 规则做 AND
  extra_path_filter: Option<crate::query::PathFilter>,
  cancel_token: Option<tokio_util::sync::CancellationToken>,
  // 基础路径（可选）：如果设置，过滤时将先去除该前缀（用于支持相对路径 glob）
  base_path: Option<PathBuf>,
}

impl EntryStreamProcessor {
  pub fn new(processor: Arc<SearchProcessor>) -> Self {
    Self {
      processor,
      content_timeout: Duration::from_secs(60),
      extra_path_filter: None,
      cancel_token: None,
      base_path: None,
    }
  }

  /// 设置取消令牌
  pub fn with_cancel_token(mut self, token: tokio_util::sync::CancellationToken) -> Self {
    self.cancel_token = Some(token);
    self
  }

  /// 设置基础路径（用于相对路径过滤）
  pub fn with_base_path(mut self, base_path: impl Into<PathBuf>) -> Self {
    self.base_path = Some(base_path.into());
    self
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
      // 检查取消
      if tx.is_closed() {
        trace!("探测到下游通道已关闭，主动终止扫描任务");
        break;
      }

      if let Some(token) = &self.cancel_token
        && token.is_cancelled()
      {
        break;
      }

      // 如果并发达到上限，先等待一个任务完成
      if in_flight.len() >= max_conc {
        if let Some(handle) = in_flight.next().await {
          let _ = handle; // JoinHandle 本身就是 future，已经在 FuturesUnordered 中等待
        }
        continue;
      }

      // 拉取下一个条目
      let next = entries.next_entry().await.map_err(|e| e.to_string())?;
      let Some((meta, mut reader)) = next else {
        break;
      };

      // 路径过滤（仅在主循环进行，任务内无需再次判断）
      let path_to_check_p = if let Some(base) = &self.base_path {
        std::path::Path::new(&meta.path)
          .strip_prefix(base)
          .unwrap_or(std::path::Path::new(&meta.path))
      } else {
        std::path::Path::new(&meta.path)
      };

      if !self
        .processor
        .should_process_path_with(&path_to_check_p.to_string_lossy(), self.extra_path_filter.as_ref())
      {
        trace!("路径不匹配，跳过: {}", &meta.path);
        continue;
      }

      if meta.is_compressed || meta.source == EntrySource::Tar || meta.source == EntrySource::TarGz {
        // tar.gz 等共享底层读取器的来源：必须保证串行读取，但可以预读小文件到内存后并发处理
        // 优化：对于小文件（< 10MB），预读到内存后允许并发处理，充分利用多核 CPU
        const MAX_PRELOAD_SIZE: usize = 10 * 1024 * 1024; // 10MB

        // 尝试预读文件到内存
        match preload_entry(&mut reader, MAX_PRELOAD_SIZE).await {
          Ok(PreloadResult::Complete(content)) => {
            // 小文件完全读取，可以并发处理
            trace!(
              "归档条目预读成功（完整），允许并发处理: {} ({} bytes)",
              meta.path,
              content.len()
            );
            let proc_clone = processor.clone();
            let tx_clone = tx.clone();
            let path = meta.path.clone();
            let container_path = meta.container_path.clone();

            // 使用 spawn 创建任务，统一类型
            let handle = tokio::spawn(async move {
              let mut mem_reader = std::io::Cursor::new(content);
              match tokio::time::timeout(
                content_timeout,
                proc_clone.process_content(path.clone(), &mut mem_reader),
              )
              .await
              {
                Ok(Ok(Some(mut result))) => {
                  result.archive_path = container_path;
                  let _ = tx_clone.send(SearchEvent::Success(result)).await;
                }
                Ok(Ok(None)) => {}
                Ok(Err(e)) => {
                  warn!("处理预读条目内容失败: {}", e);
                  let error_msg = format!("内容处理失败: {}", e);
                  let _ = tx_clone
                    .send(SearchEvent::Error {
                      source: "条目流#preload".to_string(),
                      message: error_msg,
                      recoverable: true,
                    })
                    .await;
                }
                Err(_) => {
                  warn!("处理预读条目超时: {}", path);
                }
              }
            });
            in_flight.push(handle);
          }
          Ok(PreloadResult::Partial(prefix)) => {
            // 大文件：已读取部分内容，使用 PrefixedReader 组合已读取部分和剩余 reader
            trace!(
              "归档条目过大，使用流式处理: {} (已读取 {} bytes)",
              meta.path,
              prefix.len()
            );
            while let Some(handle) = in_flight.next().await {
              let _ = handle; // 等待所有并发任务完成
            }

            // 使用 ChainedReader 组合已读取的部分和剩余的 reader
            let combined_reader = ChainedReader::new(prefix, reader);
            let container_path = meta.container_path.clone();

            // 串行处理大文件（大文件必须串行，因为 reader 已被部分消费）
            match tokio::time::timeout(
              content_timeout,
              processor.process_content(meta.path.clone(), &mut Box::pin(combined_reader)),
            )
            .await
            {
              Ok(Ok(Some(mut result))) => {
                result.archive_path = container_path;
                if tx.send(SearchEvent::Success(result)).await.is_err() {
                  warn!("下游接收已关闭，终止条目流处理");
                  break;
                }
              }
              Ok(Ok(None)) => {}
              Ok(Err(e)) => {
                warn!("处理大文件条目内容失败: {}", e);
                let error_msg = format!("内容处理失败: {}", e);
                let _ = tx
                  .send(SearchEvent::Error {
                    source: "条目流#large".to_string(),
                    message: error_msg,
                    recoverable: true,
                  })
                  .await;
              }
              Err(_) => warn!("处理大文件条目超时: {}", meta.path),
            }
          }
          Err(e) => {
            // 预读失败（IO 错误）
            warn!("归档条目预读失败: {}: {}", meta.path, e);
            let error_msg = format!("预读失败: {}", e);
            let _ = tx
              .send(SearchEvent::Error {
                source: "条目流#preload-error".to_string(),
                message: error_msg,
                recoverable: true,
              })
              .await;
          }
        }
      } else {
        // 本地文件等独立 Reader：可以并发处理
        let proc_clone = processor.clone();
        let tx_clone = tx.clone();
        let path = meta.path.clone();
        let container_path = meta.container_path.clone();
        // 使用 spawn 创建任务，统一类型
        let handle = tokio::spawn(async move {
          match tokio::time::timeout(content_timeout, proc_clone.process_content(path.clone(), &mut reader)).await {
            Ok(Ok(Some(mut result))) => {
              result.archive_path = container_path;
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
        in_flight.push(handle);
      }
    }

    // 等待所有在途任务完成
    while let Some(handle) = in_flight.next().await {
      let _ = handle; // JoinHandle 本身就是 future，已经在 FuturesUnordered 中等待
    }

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
        Ok(Box::new(stream) as Box<dyn EntryStream>)
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
        Ok(Box::new(MultiFileEntryStream::new(files)) as Box<dyn EntryStream>)
      }
      (Endpoint::Local { root }, Target::Archive { path, entry: _entry }) => {
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
      (Endpoint::S3 { profile, bucket }, Target::Archive { path, entry: _entry }) => {
        // 加载 Profile
        let profile_row = crate::repository::s3::load_s3_profile(&self.db_pool, profile)
          .await
          .map_err(|e| format!("加载 S3 Profile 失败: {:?}", e))?
          .ok_or_else(|| format!("S3 Profile 不存在: {}", profile))?;

        // 注意：虽然 MinIO 数据存储在本地文件系统，但由于使用 Erasure Coding
        // 数据会被分片存储，无法直接通过 bucket/key 映射到文件系统路径
        // 因此必须通过 S3 API 访问，即使在同一台机器上

        // 构造读取器（S3 API 路径）
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
      (Endpoint::S3 { profile, bucket }, Target::Files { paths }) => {
        // 加载 Profile
        let profile_row = crate::repository::s3::load_s3_profile(&self.db_pool, profile)
          .await
          .map_err(|e| format!("加载 S3 Profile 失败: {:?}", e))?
          .ok_or_else(|| format!("S3 Profile 不存在: {}", profile))?;

        // 构造 S3 客户端
        use crate::utils::storage::{ReaderProvider as _, S3ReaderProvider, get_or_create_s3_client};
        let _ = get_or_create_s3_client(&profile_row.endpoint, &profile_row.access_key, &profile_row.secret_key)
          .map_err(|e| format!("创建 S3 客户端失败: {:?}", e))?;

        if let Some(path) = paths.first() {
          let provider = S3ReaderProvider::new(
            &profile_row.endpoint,
            &profile_row.access_key,
            &profile_row.secret_key,
            bucket,
            path,
          );
          let reader = provider
            .open()
            .await
            .map_err(|e| format!("打开 S3 对象失败: {:?}", e))?;

          Ok(Box::new(S3FileEntryStream {
            reader: Some(reader),
            path: path.clone(),
          }) as Box<dyn EntryStream>)
        } else {
          Err("S3 文件列表为空".to_string())
        }
      }
      (Endpoint::S3 { .. }, _) => Err("S3 仅支持 archive/files 目标".to_string()),
      (Endpoint::Agent { agent_id, .. }, Target::Archive { path, entry: _entry }) => {
        // Agent 归档：从 Agent 下载归档文件，然后创建归档流
        let client = crate::agent::create_agent_client_by_id(agent_id.clone())
          .await
          .map_err(|e| format!("无法创建 Agent 客户端: {}", e))?;

        // 下载归档文件
        let url = format!("/api/v1/file_raw?path={}", urlencoding::encode(path));
        tracing::debug!("从 Agent 下载归档: agent_id={}, path={}", agent_id, path);

        let response = client
          .get_raw(&url)
          .await
          .map_err(|e| format!("从 Agent 下载归档失败: {}", e))?;

        // 将响应转换为字节流
        use futures_util::TryStreamExt;
        let bytes_stream = response.bytes_stream().map_err(std::io::Error::other);

        // 使用 StreamReader 将字节流转换为 AsyncRead
        let reader = tokio_util::io::StreamReader::new(bytes_stream);

        // 创建归档流
        create_archive_stream_from_reader(reader, Some(path)).await
      }
      (Endpoint::Agent { .. }, _) => Err("Agent 来源请通过远程 SearchService 处理".to_string()),
    }
  }
}

/// S3 单文件流（临时定义，建议移至 opsbox_core）
pub struct S3FileEntryStream<R> {
  reader: Option<R>,
  path: String,
}

#[async_trait::async_trait]
impl<R: AsyncRead + Send + Unpin + 'static> EntryStream for S3FileEntryStream<R> {
  async fn next_entry(
    &mut self,
  ) -> io::Result<Option<(opsbox_core::fs::EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    if let Some(reader) = self.reader.take() {
      let meta = opsbox_core::fs::EntryMeta {
        path: self.path.clone(),
        container_path: None,
        size: None,
        is_compressed: false,
        source: opsbox_core::fs::EntrySource::File,
      };
      Ok(Some((meta, Box::new(reader))))
    } else {
      Ok(None)
    }
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
        return Ok(Box::new(MultiFileEntryStream::new(paths)) as Box<dyn EntryStream>);
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
        return Ok(Box::new(stream) as Box<dyn EntryStream>);
      }
      Target::Archive { path, entry: _entry } => {
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
    Ok(meta) if meta.is_file() => {
      Ok(Box::new(MultiFileEntryStream::new(vec![root_or_file.to_string()])) as Box<dyn EntryStream>)
    }
    Ok(meta) if meta.is_dir() => {
      // 默认递归（与 Server 端 Target::Dir 的默认行为一致）
      let stream = FsEntryStream::new(PathBuf::from(root_or_file), true)
        .await
        .map_err(|e| format!("无法读取目录 {}: {}", root_or_file, e))?;
      Ok(Box::new(stream) as Box<dyn EntryStream>)
    }
    Ok(_) => Err(format!("不支持的文件类型: {}", root_or_file)),
    Err(e) => Err(format!("无法访问路径 {}: {}", root_or_file, e)),
  }
}

/// 通用条目流处理函数（支持基于回调的结果处理）
///
/// 提供统一的条目流处理方式，可被 Server 和 Agent 复用，避免重复实现核心处理逻辑。
/// 事件通过回调函数返回，调用方可灵活处理（发送到 channel、生成消息等）。
pub async fn process_entry_stream_with_callback<F>(
  stream: Box<dyn EntryStream>,
  processor: Arc<crate::service::search::SearchProcessor>,
  extra_path_filter: Option<crate::query::PathFilter>,
  cancel_token: Option<tokio_util::sync::CancellationToken>,
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
  if let Some(token) = cancel_token {
    stream_processor = stream_processor.with_cancel_token(token);
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
  use std::fs::File;
  use std::io::Write;
  use tempfile::tempdir;

  #[tokio::test]
  async fn test_build_local_entry_stream() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.log");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "hello").unwrap();

    // 测试文件路径
    let stream_res = build_local_entry_stream(&file_path.to_string_lossy(), None).await;
    assert!(stream_res.is_ok(), "文件路径应该能正常创建流: {:?}", stream_res.err());

    // 测试目录路径
    let stream_res = build_local_entry_stream(&dir.path().to_string_lossy(), None).await;
    assert!(stream_res.is_ok(), "目录路径应该能正常创建流: {:?}", stream_res.err());
  }

  #[tokio::test]
  async fn test_build_local_entry_stream_with_target() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.log");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "hello").unwrap();

    use crate::domain::config::Target;

    // 测试 Target::Files
    let target = Target::Files {
      paths: vec![file_path.to_string_lossy().to_string()],
    };
    let stream_res = build_local_entry_stream(&file_path.to_string_lossy(), Some(target)).await;
    assert!(stream_res.is_ok(), "Target::Files 应该能正常创建流");

    // 测试 Target::Dir
    let target = Target::Dir {
      path: ".".to_string(),
      recursive: true,
    };
    let stream_res = build_local_entry_stream(&dir.path().to_string_lossy(), Some(target)).await;
    assert!(stream_res.is_ok(), "Target::Dir 应该能正常创建流");
  }
}
